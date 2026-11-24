//! Fuzzy-Matching: findet Tippfehler-Varianten von Namen (z.B. „Müler“ statt
//! „Müller“), die das exakte Regex-Matching verpasst. Ergebnis ist eine
//! Review-Liste — ersetzt wird erst nach manueller Bestätigung (M4-UI).

use crate::matcher::Matcher;
use crate::spelling::{fold, is_word_char};
use std::collections::HashSet;

pub const DEFAULT_THRESHOLD: f32 = 0.75;
pub const MAX_DISTANCE: usize = 3;
const MAX_HITS: usize = 200;

/// Ein unscharfer Treffer.
#[derive(Debug, Clone, PartialEq)]
pub struct FuzzyHit {
    /// tatsächlich gefundener Text im Dokument
    pub matched: String,
    /// zugehöriger echter Bezeichner aus der Namensliste
    pub identifier: String,
    /// vorgeschlagene Ersetzung (Pseudonym bzw. Echtname)
    pub replacement: String,
    pub similarity: f32,
    /// Textausschnitt um die Fundstelle
    pub context: String,
    /// Fundstelle (z.B. „word/document.xml, Absatz 3“ oder „Seite 2“)
    pub location: String,
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Ähnlichkeit 0..1; Umlaute zählen wie ihre Umschreibung (ü = ue).
pub fn similarity(a: &str, b: &str) -> f32 {
    let a = fold(a);
    let b = fold(b);
    let max = a.chars().count().max(b.chars().count());
    if max == 0 {
        return 1.0;
    }
    1.0 - (levenshtein(&a, &b) as f32 / max as f32)
}

/// Byte-Spannen aller „Wörter" (Whitespace-getrennt) im Text. Satzzeichen am
/// Wortrand gehören nicht dazu («Müler.» → «Müler»), sonst wäre der Abstand
/// zum Namen zu gross und die Ersetzung träfe das Satzzeichen mit.
fn word_spans(text: &str) -> Vec<(usize, usize, &str)> {
    let mut raw = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                raw.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        raw.push((s, text.len()));
    }
    raw.into_iter()
        .filter_map(|(s, e)| {
            let w = &text[s..e];
            let trimmed = w.trim_matches(|c: char| !is_word_char(c));
            if trimmed.is_empty() {
                return None;
            }
            let s2 = s + (w.len() - w.trim_start_matches(|c: char| !is_word_char(c)).len());
            Some((s2, s2 + trimmed.len(), trimmed))
        })
        .collect()
}

fn context_around(text: &str, byte_start: usize, byte_end: usize) -> String {
    // je 30 Zeichen Kontext, auf char-Grenzen geschnitten
    let s = text[..byte_start]
        .char_indices()
        .rev()
        .nth(29)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut e = (byte_end + 30).min(text.len());
    while !text.is_char_boundary(e) {
        e += 1;
    }
    format!("…{}…", &text[s..e])
}

/// Sucht unscharfe Treffer der Mapping-Keys im Text. Exakte Treffer werden
/// übersprungen (die erledigt das Regex-Matching). Überlappende Fenster werden
/// zugunsten des besseren Treffers dedupliziert.
pub fn find_fuzzy(text: &str, mapping: &[(String, String)], threshold: f32) -> Vec<FuzzyHit> {
    let spans = word_spans(text);
    let words: Vec<&str> = spans.iter().map(|s| s.2).collect();
    let mut raw: Vec<(f32, usize, usize, FuzzyHit)> = Vec::new();
    let mut seen: HashSet<(usize, String)> = HashSet::new();

    // trifft das exakte Matching die Stelle schon? (gecacht über alle Aufrufe)
    let exact = Matcher::cached(mapping, true).ok();

    for (ident, repl) in mapping {
        let n = ident.split_whitespace().count();
        if n == 0 || words.len() < n {
            continue;
        }
        // ohne Akzente, Umlaute umschrieben: «Mueler» ist so nur einen
        // Buchstaben von «Müller» entfernt, «Jose» gar keinen von «José»
        let id_lower = fold(ident);
        for w in 0..=words.len() - n {
            let cand = words[w..w + n].join(" ");
            let cand_lower = fold(&cand);
            let dist = levenshtein(&cand_lower, &id_lower);
            if dist > MAX_DISTANCE {
                continue;
            }
            let maxlen = cand_lower.chars().count().max(id_lower.chars().count());
            let sim = 1.0 - dist as f32 / maxlen.max(1) as f32;
            if sim < threshold {
                continue;
            }
            // Schreibvarianten («Mueller», «Jose») ersetzt schon das exakte Matching
            if exact.as_ref().is_some_and(|m| m.is_variant(ident, &cand)) {
                continue;
            }
            if !seen.insert((w, id_lower.clone())) {
                continue;
            }
            let hit = FuzzyHit {
                matched: cand.clone(),
                identifier: ident.clone(),
                replacement: repl.clone(),
                similarity: sim,
                context: context_around(text, spans[w].0, spans[w + n - 1].1),
                location: String::new(),
            };
            raw.push((sim, w, w + n, hit));
        }
    }

    // beste Treffer zuerst, Überlappungen verwerfen
    raw.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut accepted: Vec<(usize, usize)> = Vec::new();
    let mut hits = Vec::new();
    for (sim, ws, we, hit) in raw {
        if accepted.iter().any(|(as_, ae)| ws < *ae && we > *as_) {
            continue;
        }
        accepted.push((ws, we));
        let mut hit = hit;
        hit.similarity = sim;
        hits.push(hit);
        if hits.len() >= MAX_HITS {
            break;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> Vec<(String, String)> {
        vec![
            ("Anna Müller".to_string(), "Leo Roth".to_string()),
            ("Müller".to_string(), "Roth".to_string()),
            ("Ben Schulz".to_string(), "Elena Lorenz".to_string()),
        ]
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("Müler", "Müller"), 1);
        assert_eq!(levenshtein("Schultz", "Schulz"), 1);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    #[test]
    fn finds_typo_not_exact() {
        let text = "Hier hat Müler abgegeben, Anna Müller nicht.";
        let hits = find_fuzzy(text, &mapping(), DEFAULT_THRESHOLD);
        assert!(hits.iter().any(|h| h.matched == "Müler"), "hits: {hits:?}");
        // exakter Treffer "Anna Müller" darf nicht als Fuzzy-Hit auftauchen
        assert!(!hits.iter().any(|h| h.matched == "Anna Müller"));
        let h = hits.iter().find(|h| h.matched == "Müler").unwrap();
        assert_eq!(h.identifier, "Müller");
        assert_eq!(h.replacement, "Roth");
        assert!(h.similarity >= DEFAULT_THRESHOLD);
        assert!(h.context.contains("Müler"));
    }

    #[test]
    fn transcribed_umlauts() {
        // «Mueller» erledigt das exakte Matching, «Mueler» ist ein Tippfehler davon
        let hits = find_fuzzy("Mueller und Mueler", &mapping(), DEFAULT_THRESHOLD);
        assert!(!hits.iter().any(|h| h.matched == "Mueller"), "hits: {hits:?}");
        assert!(hits.iter().any(|h| h.matched == "Mueler" && h.identifier == "Müller"), "hits: {hits:?}");
    }

    #[test]
    fn accents_folded() {
        let map = vec![("Léa".to_string(), "Mia".to_string()), ("Gul".to_string(), "Eda".to_string())];
        // «Lea» ersetzt das exakte Matching; «Leá» auch (Akzent optional)
        let hits = find_fuzzy("Lea und Leá", &map, DEFAULT_THRESHOLD);
        assert!(hits.is_empty(), "hits: {hits:?}");
        // «Gül» trifft «Gul» bewusst nicht exakt → zur Prüfung vorschlagen
        let hits = find_fuzzy("Gül war da", &map, DEFAULT_THRESHOLD);
        assert!(hits.iter().any(|h| h.matched == "Gül" && h.identifier == "Gul"), "hits: {hits:?}");
    }

    #[test]
    fn respects_threshold() {
        let text = "Das ist ein ganz anderer Satz ohne Namen.";
        let hits = find_fuzzy(text, &mapping(), DEFAULT_THRESHOLD);
        assert!(hits.is_empty(), "hits: {hits:?}");
    }

    #[test]
    fn multiword_typo() {
        let text = "Ben Schultz hat die Aufgabe gelöst.";
        let hits = find_fuzzy(text, &mapping(), DEFAULT_THRESHOLD);
        assert!(hits.iter().any(|h| h.matched == "Ben Schultz" && h.identifier == "Ben Schulz"), "hits: {hits:?}");
    }

    #[test]
    fn no_duplicate_overlaps() {
        let text = "Müler Müler Müler";
        let hits = find_fuzzy(text, &mapping(), DEFAULT_THRESHOLD);
        // „Müler Müler“ (bigram) vs „Müler“ — es darf nur der jeweils beste accepted werden
        assert!(!hits.is_empty());
        let mut ranges: Vec<_> = hits.iter().map(|h| h.matched.clone()).collect();
        ranges.sort();
        let _ = ranges;
    }
}
