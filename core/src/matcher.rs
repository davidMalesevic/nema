//! Gemeinsamer Matcher für DOCX, PDF und Dateinamen: findet alle Suchtexte
//! ohne Rücksicht auf Gross-/Kleinschreibung, längste zuerst (damit
//! „Anna Müller" vor „Müller" gewinnt).
//!
//! Im Dokument zählen nur ganze Wörter: «Tim» wird nicht in «Optimierung»
//! ersetzt. Ein angehängtes Genitiv-s gehört noch zum Namen («Annas Lösung»,
//! «Müller's»), damit der Name dort nicht stehen bleibt.
//!
//! Schreibvarianten zählen als derselbe Name («Müller»/«Mueller»,
//! «José»/«Jose», «Søren»/«Soeren»); die Regeln stehen in `spelling`.

use crate::spelling::{is_word_char, key_pattern, loose};
use regex::{Regex, RegexBuilder};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// lowercased match → (original-Schlüssel fürs Reporting, Ersetzung)
pub type Lookup = HashMap<String, (String, String)>;

/// Ein Treffer im Text (Byte-Offsets).
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub start: usize,
    pub end: usize,
    /// Suchtext in Originalschreibweise (fürs Reporting)
    pub key: String,
    pub repl: String,
}

pub struct Matcher {
    re: Regex,
    lookup: Lookup,
    /// Je Suchtext ein am Anfang verankertes Muster, längste zuerst. Ausweichsuche,
    /// wenn der längste Kandidat an einer Stelle kein ganzes Wort ist, und
    /// Zuordnung von Schreibvarianten («Mueller») zum Suchtext («Müller»).
    anchored: Vec<Anchored>,
    /// Länge des längsten Suchtexts (Bytes)
    longest: usize,
    whole_words: bool,
}

/// Erst bei Bedarf kompiliert: die Akzent-Klassen machen das Kompilieren teuer,
/// gebraucht werden aber meist nur wenige.
struct Anchored {
    pattern: String,
    /// grobe Form des Suchtexts (Vorfilter, siehe `spelling::loose`)
    loose: String,
    re: OnceLock<Option<Regex>>,
    key: String,
    repl: String,
}

impl Anchored {
    /// Länge des Treffers am Anfang von `s`.
    fn match_len(&self, s: &str) -> Option<usize> {
        self.re
            .get_or_init(|| Regex::new(&format!("(?i)^(?:{})", self.pattern)).ok())
            .as_ref()?
            .find(s)
            .map(|m| m.end())
    }
}

/// Matcher für Dokumenttext: nur ganze Wörter.
pub fn build_matcher(mapping: &[(String, String)]) -> Result<Arc<Matcher>, regex::Error> {
    Matcher::cached(mapping, true)
}

impl Matcher {
    /// `whole_words = false` ersetzt auch mitten im Wort (für Dateinamen wie
    /// «MüllerAnna.docx»: dort ist ein stehen gebliebener Name schlimmer).
    pub fn new(mapping: &[(String, String)], whole_words: bool) -> Result<Self, regex::Error> {
        let mut sorted: Vec<&(String, String)> = mapping.iter().filter(|(k, _)| !k.is_empty()).collect();
        sorted.sort_by_key(|(k, _)| Reverse(k.len()));
        let patterns: Vec<String> = sorted.iter().map(|(k, _)| key_pattern(k)).collect();
        // Jeder Buchstabe ist eine Zeichenklasse mit allen Akzentformen: bei
        // langen Namenslisten wird das Muster grösser als das Standardlimit.
        let re = RegexBuilder::new(&format!("(?i)(?:{})", patterns.join("|")))
            .size_limit(256 << 20)
            .dfa_size_limit(256 << 20)
            .build()?;
        let anchored = sorted
            .iter()
            .zip(patterns)
            .map(|((k, v), pattern)| Anchored {
                pattern,
                loose: loose(k),
                re: OnceLock::new(),
                key: k.clone(),
                repl: v.clone(),
            })
            .collect();
        let lookup = sorted.iter().map(|(k, v)| (k.to_lowercase(), (k.clone(), v.clone()))).collect();
        let longest = sorted.first().map_or(0, |(k, _)| k.len());
        Ok(Self { re, lookup, anchored, longest, whole_words })
    }

    /// Wie `new`, aber pro Mapping nur einmal gebaut: ein Lauf verarbeitet
    /// viele Dateien (und Dateinamen) mit derselben Liste. Der Cache liegt nur
    /// im Arbeitsspeicher und hält die zuletzt benutzten Listen.
    pub fn cached(mapping: &[(String, String)], whole_words: bool) -> Result<Arc<Self>, regex::Error> {
        type Cache = Vec<(Vec<(String, String)>, bool, Arc<Matcher>)>;
        static CACHE: Mutex<Cache> = Mutex::new(Vec::new());
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = cache.iter().position(|(m, w, _)| *w == whole_words && m.as_slice() == mapping) {
            let entry = cache.remove(i);
            let matcher = entry.2.clone();
            cache.push(entry);
            return Ok(matcher);
        }
        let matcher = Arc::new(Self::new(mapping, whole_words)?);
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((mapping.to_vec(), whole_words, matcher.clone()));
        Ok(matcher)
    }

    /// Ist `text` (vollständig) eine Schreibvariante von `key`, die das exakte
    /// Matching ersetzt? Für die Tippfehler-Suche.
    pub fn is_variant(&self, key: &str, text: &str) -> bool {
        let lt = loose(text);
        self.anchored
            .iter()
            .any(|a| a.key == key && a.loose == lt && a.match_len(text) == Some(text.len()))
    }

    fn left_ok(text: &str, start: usize) -> bool {
        !text[..start].chars().next_back().is_some_and(is_word_char)
    }

    /// Wortende: kein Buchstabe/keine Ziffer danach, oder ein Genitiv-s
    /// («Annas», «Müller's», «Müller’s») gefolgt vom Wortende.
    fn right_ok(text: &str, end: usize) -> bool {
        let mut rest = text[end..].chars();
        match rest.next() {
            None => true,
            Some(c) if !is_word_char(c) => true,
            Some('s') => !rest.next().is_some_and(is_word_char),
            Some(_) => false,
        }
    }

    fn found(&self, text: &str, start: usize, end: usize) -> Found {
        let m = &text[start..end];
        // gleiche Schreibweise zuerst, sonst der (längste) Suchtext, dessen
        // Muster genau diese Stelle trifft («Mueller» → «Müller»)
        let (key, repl) = self
            .lookup
            .get(&m.to_lowercase())
            .cloned()
            .or_else(|| {
                let lm = loose(m);
                self.anchored
                    .iter()
                    .find(|a| a.loose == lm && a.match_len(m) == Some(m.len()))
                    .map(|a| (a.key.clone(), a.repl.clone()))
            })
            .unwrap_or_else(|| (m.to_string(), m.to_string()));
        Found { start, end, key, repl }
    }

    /// Alle Treffer von links nach rechts, ohne Überlappung.
    pub fn find_all(&self, text: &str) -> Vec<Found> {
        let mut out = Vec::new();
        if self.lookup.is_empty() {
            return out;
        }
        let mut pos = 0;
        while pos <= text.len() {
            let Some(m) = self.re.find_at(text, pos) else { break };
            let (start, mut end) = (m.start(), m.end());
            let mut accept = end > start;
            if accept && self.whole_words {
                accept = Self::left_ok(text, start);
                if accept && !Self::right_ok(text, end) {
                    // Der längste Kandidat passt nicht: kürzere an derselben Stelle versuchen
                    // («Anna Müllerstrasse»: nicht «Anna Müller», aber «Anna»).
                    accept = false;
                    // grobe Form des Textes ab hier; passende Suchtexte sind Präfixe davon
                    let mut window = (start + 4 * self.longest).min(text.len());
                    while !text.is_char_boundary(window) {
                        window += 1;
                    }
                    let lt = loose(&text[start..window]);
                    for a in self.anchored.iter().filter(|a| lt.starts_with(&a.loose)) {
                        if let Some(len) = a.match_len(&text[start..]) {
                            if len > 0 && Self::right_ok(text, start + len) {
                                end = start + len;
                                accept = true;
                                break;
                            }
                        }
                    }
                }
            }
            if accept {
                out.push(self.found(text, start, end));
                pos = end;
            } else {
                // ein Zeichen weiter
                pos = start + text[start..].chars().next().map_or(1, char::len_utf8);
            }
        }
        out
    }

    /// Ersetzt alle Treffer; `on_hit` wird pro Treffer aufgerufen (Statistik).
    /// None, wenn nichts ersetzt wurde.
    pub fn replace_all(&self, text: &str, mut on_hit: impl FnMut(&Found)) -> Option<String> {
        let hits = self.find_all(text);
        if hits.is_empty() {
            return None;
        }
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for h in &hits {
            out.push_str(&text[last..h.start]);
            out.push_str(&h.repl);
            last = h.end;
            on_hit(h);
        }
        out.push_str(&text[last..]);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    fn replace(text: &str, pairs: &[(&str, &str)]) -> String {
        build_matcher(&m(pairs)).unwrap().replace_all(text, |_| {}).unwrap_or_else(|| text.to_string())
    }

    #[test]
    fn only_whole_words() {
        let map = [("Tim", "Max"), ("Anna", "Lea")];
        assert_eq!(replace("Optimierung von Tim, Zeitimpuls", &map), "Optimierung von Max, Zeitimpuls");
        assert_eq!(replace("Annabelle und Hannah, aber ANNA.", &map), "Annabelle und Hannah, aber Lea.");
        assert_eq!(replace("tim@x.ch (Tim) Tim-Luca _Tim_", &map), "Max@x.ch (Max) Max-Luca _Max_");
        assert_eq!(replace("Tim2 und 2Tim bleiben", &map), "Tim2 und 2Tim bleiben");
    }

    #[test]
    fn genitive_s_is_covered() {
        let map = [("Anna", "Lea"), ("Müller", "Roth")];
        assert_eq!(replace("Annas Lösung, Müller's Heft, Müller’s Blatt", &map), "Leas Lösung, Roth's Heft, Roth’s Blatt");
        assert_eq!(replace("Annasusanne", &map), "Annasusanne");
    }

    #[test]
    fn falls_back_to_shorter_key() {
        let map = [("Anna Müller", "Lea Roth"), ("Anna", "Lea"), ("Müller", "Roth")];
        assert_eq!(replace("Anna Müller wohnt an der Anna Müllerstrasse", &map), "Lea Roth wohnt an der Lea Müllerstrasse");
    }

    #[test]
    fn umlauts_and_offsets() {
        let hits = build_matcher(&m(&[("Müller", "Roth")])).unwrap().find_all("Äh, Frau MÜLLER käme");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "Müller");
        assert_eq!(&"Äh, Frau MÜLLER käme"[hits[0].start..hits[0].end], "MÜLLER");
    }

    #[test]
    fn umlaut_transcriptions_match_both_ways() {
        let map = [("Anna Müller", "Lea Roth"), ("Müller", "Roth"), ("Jörg", "Tom"), ("Strauß", "Berg")];
        assert_eq!(replace("Anna Mueller, Frau MUELLER, Joerg, Strauss", &map), "Lea Roth, Frau Roth, Tom, Berg");
        let map = [("Mueller", "Roth"), ("Baer", "Fuchs"), ("Rossi", "Conti")];
        assert_eq!(replace("Müller, Bär und Roßi", &map), "Roth, Fuchs und Conti");
        // zerlegter Umlaut (NFD), z.B. aus macOS-Dateinamen
        assert_eq!(replace("Mu\u{308}ller", &[("Müller", "Roth")]), "Roth");
        // keine Teilwörter
        assert_eq!(replace("Muellerstrasse", &[("Müller", "Roth")]), "Muellerstrasse");
    }

    #[test]
    fn variant_reports_original_key() {
        let hits = build_matcher(&m(&[("Müller", "Roth"), ("Anna", "Lea")])).unwrap().find_all("Mueller");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "Müller");
        assert_eq!(hits[0].repl, "Roth");
        // zwei Personen mit unterschiedlicher Schreibweise: die exakte gewinnt
        let pairs = m(&[("Müller", "Roth"), ("Mueller", "Fuchs")]);
        let mm = build_matcher(&pairs).unwrap();
        assert_eq!(mm.replace_all("Müller Mueller", |_| {}).as_deref(), Some("Roth Fuchs"));
    }

    #[test]
    fn accents_optional_and_decomposed_text() {
        let map = [("José García", "Leo Roth"), ("Jose", "Leo"), ("Lea", "Mia")];
        assert_eq!(replace("Jose Garcia, JOSÉ, Léa und Lea", &map), "Leo Roth, Leo, Mia und Mia");
        // zerlegtes «é»: der Akzent gehört zum Namen und darf nicht übrig bleiben
        assert_eq!(replace("Jose\u{301} kommt", &map), "Leo kommt");
        assert_eq!(replace("Leaé", &map), "Leaé");
    }

    #[test]
    fn lenient_mode_matches_inside_words() {
        let lenient = Matcher::new(&m(&[("Müller", "Roth"), ("Anna", "Lea")]), false).unwrap();
        assert_eq!(lenient.replace_all("PruefungMüllerAnna", |_| {}).as_deref(), Some("PruefungRothLea"));
        assert_eq!(lenient.replace_all("MuellerAnna", |_| {}).as_deref(), Some("RothLea"));
        assert!(Matcher::new(&[], true).unwrap().find_all("irgendwas").is_empty());
    }
}
