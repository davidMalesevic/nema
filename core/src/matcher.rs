//! Gemeinsamer Matcher für DOCX, PDF und Dateinamen: findet alle Suchtexte
//! ohne Rücksicht auf Gross-/Kleinschreibung, längste zuerst (damit
//! „Anna Müller" vor „Müller" gewinnt).
//!
//! Im Dokument zählen nur ganze Wörter: «Tim» wird nicht in «Optimierung»
//! ersetzt. Ein angehängtes Genitiv-s gehört noch zum Namen («Annas Lösung»,
//! «Müller's»), damit der Name dort nicht stehen bleibt.

use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;

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
    /// Je Suchtext ein am Anfang verankerter Regex, längste zuerst. Ausweichsuche,
    /// wenn der längste Kandidat an einer Stelle kein ganzes Wort ist.
    anchored: Vec<Regex>,
    whole_words: bool,
}

/// Matcher für Dokumenttext: nur ganze Wörter.
pub fn build_matcher(mapping: &[(String, String)]) -> Result<Matcher, regex::Error> {
    Matcher::new(mapping, true)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

impl Matcher {
    /// `whole_words = false` ersetzt auch mitten im Wort (für Dateinamen wie
    /// «MüllerAnna.docx»: dort ist ein stehen gebliebener Name schlimmer).
    pub fn new(mapping: &[(String, String)], whole_words: bool) -> Result<Self, regex::Error> {
        let mut sorted: Vec<&(String, String)> = mapping.iter().filter(|(k, _)| !k.is_empty()).collect();
        sorted.sort_by_key(|(k, _)| Reverse(k.len()));
        let pattern = sorted.iter().map(|(k, _)| regex::escape(k)).collect::<Vec<_>>().join("|");
        let re = Regex::new(&format!("(?i)(?:{pattern})"))?;
        let anchored = if whole_words {
            sorted
                .iter()
                .map(|(k, _)| Regex::new(&format!("(?i)^(?:{})", regex::escape(k))))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let lookup = sorted.iter().map(|(k, v)| (k.to_lowercase(), (k.clone(), v.clone()))).collect();
        Ok(Self { re, lookup, anchored, whole_words })
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
        let (key, repl) = self
            .lookup
            .get(&m.to_lowercase())
            .cloned()
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
                    for re in &self.anchored {
                        if let Some(mm) = re.find(&text[start..]) {
                            if mm.end() > 0 && Self::right_ok(text, start + mm.end()) {
                                end = start + mm.end();
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
    fn lenient_mode_matches_inside_words() {
        let lenient = Matcher::new(&m(&[("Müller", "Roth"), ("Anna", "Lea")]), false).unwrap();
        assert_eq!(lenient.replace_all("PruefungMüllerAnna", |_| {}).as_deref(), Some("PruefungRothLea"));
        assert!(Matcher::new(&[], true).unwrap().find_all("irgendwas").is_empty());
    }
}
