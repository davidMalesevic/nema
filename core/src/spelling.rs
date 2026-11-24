//! Schreibvarianten von Namen: welche Schreibweisen im Dokument als «derselbe
//! Name» gelten.
//!
//! - Akzente sind optional: «José» findet «Jose», «Lea» findet «Léa»,
//!   «Čapek» findet «Capek», «Łukasz» findet «Lukasz».
//! - Umlaute und ihre Umschreibung sind austauschbar: ä/ae, ö/oe, ü/ue, ø/oe,
//!   æ/ae, œ/oe, ß/ss, å/aa. Ein Umlaut im Suchtext findet auch den Vokal ohne
//!   Punkte («Gül» → «Gul»), umgekehrt aber nicht: sonst würde «Ole» auch das
//!   Wort «Öle» treffen und «Schon» das Wort «schön».
//! - Zerlegte Zeichen (Buchstabe + kombinierender Akzent, z.B. in
//!   macOS-Dateinamen oder PDFs) zählen wie die zusammengesetzten.

use regex::escape;
use std::collections::HashMap;
use std::sync::OnceLock;
use unicode_normalization::char::decompose_canonical;
use unicode_normalization::UnicodeNormalization;

const DIAERESIS: char = '\u{308}';

/// Kombinierende Akzente (Block U+0300–U+036F).
const MARKS: &str = r"[\x{300}-\x{36F}]*";
/// Dasselbe ohne Trema: «a» + U+0308 ist ein «ä» und kein akzentuiertes «a».
const MARKS_NO_DIAERESIS: &str = r"[\x{300}-\x{307}\x{309}-\x{36F}]*";

pub fn is_mark(c: char) -> bool {
    ('\u{300}'..='\u{36F}').contains(&c)
}

/// Gehört zum Wort (auch ein kombinierender Akzent).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_mark(c)
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Grundbuchstabe a–z eines lateinischen Buchstabens ohne Akzent.
/// ä/ö/ü (und ø) haben keinen: sie gelten als eigene Laute (siehe Moduldoku).
fn base(c: char) -> Option<char> {
    let c = lower(c);
    if c.is_ascii_lowercase() {
        return Some(c);
    }
    match c {
        'ł' => return Some('l'),
        'đ' | 'ð' => return Some('d'),
        'ı' => return Some('i'),
        'ħ' => return Some('h'),
        'ŧ' => return Some('t'),
        _ => {}
    }
    let mut first = None;
    let mut diaeresis = false;
    decompose_canonical(c, |d| {
        if first.is_none() {
            first = Some(d);
        } else if d == DIAERESIS {
            diaeresis = true;
        }
    });
    let b = first.filter(char::is_ascii_lowercase)?;
    if diaeresis && matches!(b, 'a' | 'o' | 'u') {
        return None;
    }
    Some(b)
}

/// Regex für einen Buchstaben samt aller akzentuierten Formen.
fn letter(b: char) -> String {
    static CLASSES: OnceLock<HashMap<char, String>> = OnceLock::new();
    let classes = CLASSES.get_or_init(|| {
        let mut m: HashMap<char, String> = ('a'..='z').map(|b| (b, b.to_string())).collect();
        for c in ('\u{C0}'..='\u{24F}').chain('\u{1E00}'..='\u{1EFF}') {
            if !c.is_lowercase() {
                continue; // Grossbuchstaben deckt (?i) ab
            }
            if let Some(b) = base(c).filter(|&b| b != c) {
                m.get_mut(&b).expect("a–z").push(c);
            }
        }
        m
    });
    let marks = if matches!(b, 'a' | 'o' | 'u') { MARKS_NO_DIAERESIS } else { MARKS };
    format!("[{}]{marks}", classes[&b])
}

/// Umlaut-Formen eines Vokals (ohne die Umschreibung).
fn umlauts(v: char) -> &'static str {
    match v {
        'a' => "äæ",
        'o' => "öøœ",
        _ => "ü",
    }
}

/// Umlaut im Suchtext: Umlaut, Umschreibung oder Vokal ohne Punkte.
fn umlaut(v: char) -> String {
    format!("(?:[{}]|{v}{DIAERESIS}|{}(?:{})?)", umlauts(v), letter(v), letter('e'))
}

/// Umschreibung im Suchtext («ue»): Umschreibung oder Umlaut, aber nicht der
/// blosse Vokal («Mueller» findet nicht «Muller»).
fn digraph(v: char) -> String {
    format!("(?:{}{}|[{}]|{v}{DIAERESIS})", letter(v), letter('e'), umlauts(v))
}

/// Regex-Muster (ohne Flags; gedacht für `(?i)`) für alle Schreibvarianten
/// eines Suchtexts.
pub fn key_pattern(key: &str) -> String {
    let chars: Vec<char> = key.nfc().filter(|&c| !is_mark(c)).collect();
    let lc: Vec<char> = chars.iter().map(|&c| lower(c)).collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let (piece, used) = match (lc[i], lc.get(i + 1).copied()) {
            ('ä' | 'æ', _) => (umlaut('a'), 1),
            ('ö' | 'ø' | 'œ', _) => (umlaut('o'), 1),
            ('ü', _) => (umlaut('u'), 1),
            (v @ ('a' | 'o' | 'u'), Some('e')) => (digraph(v), 2),
            ('a', Some('a')) => (format!("(?:{0}{0}|å)", letter('a')), 2),
            ('å', _) => (format!("(?:{0}{0}?)", letter('a')), 1),
            ('ß', _) => (format!("(?:ß|{0}{0})", letter('s')), 1),
            ('s', Some('s')) => (format!("(?:{0}{0}|ß)", letter('s')), 2),
            ('þ', _) => (format!("(?:þ|{}{})", letter('t'), letter('h')), 1),
            (c, _) => match base(c) {
                Some(b) => (letter(b), 1),
                None => (escape(&chars[i].to_string()), 1),
            },
        };
        out.push_str(&piece);
        i += used;
    }
    out
}

/// Vergleichsform für die Tippfehler-Suche: kleingeschrieben, ohne Akzente,
/// Umlaute umschrieben («Müller», «MUELLER», «Mu\u{308}ller» → «mueller»,
/// «José» → «jose»).
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfd() {
        if is_mark(c) {
            if c == DIAERESIS && out.ends_with(['a', 'o', 'u']) {
                out.push('e');
            }
            continue;
        }
        for l in c.to_lowercase() {
            match l {
                'ß' => out.push_str("ss"),
                'æ' => out.push_str("ae"),
                'œ' | 'ø' => out.push_str("oe"),
                'þ' => out.push_str("th"),
                _ => out.push(base(l).unwrap_or(l)),
            }
        }
    }
    out
}

/// Grobe Vergleichsform: `fold`, dazu «e» nach a/o/u weg und Doppelbuchstaben
/// einfach. Zwei Schreibweisen, die `key_pattern` gleichsetzt, haben immer
/// dieselbe grobe Form («Gül», «Gul», «Guel» → «gul»); umgekehrt nicht
/// unbedingt. Dient als billiger Vorfilter vor dem Regex.
pub fn loose(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in fold(s).chars() {
        let prev = out.chars().next_back();
        if prev == Some(c) || (c == 'e' && matches!(prev, Some('a' | 'o' | 'u'))) {
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    fn same(key: &str, text: &str) -> bool {
        Regex::new(&format!("(?i)^(?:{})$", key_pattern(key))).unwrap().is_match(text)
    }

    #[test]
    fn accents_are_optional_both_ways() {
        for (k, t) in [
            ("José", "Jose"),
            ("Jose", "José"),
            ("Jose", "Jose\u{301}"),
            ("Lea", "LÉA"),
            ("García", "Garcia"),
            ("Čapek", "Capek"),
            ("Capek", "Čapek"),
            ("Łukasz", "Lukasz"),
            ("Zoë", "Zoe"),
            ("Zoe", "Zoë"),
            ("Şahin", "Sahin"),
            ("Nguyễn", "Nguyen"),
            ("François", "Francois"),
        ] {
            assert!(same(k, t), "{k} ~ {t}");
        }
    }

    #[test]
    fn umlauts_and_transcriptions() {
        for (k, t) in [
            ("Müller", "Mueller"),
            ("Müller", "Muller"),
            ("Müller", "Mu\u{308}ller"),
            ("Mueller", "Müller"),
            ("Gül", "Gul"),
            ("Søren", "Soeren"),
            ("Søren", "Soren"),
            ("Soeren", "Søren"),
            ("Bjørn", "Björn"),
            ("Ærø", "Aeroe"),
            ("Ångström", "Aangstroem"),
            ("Aalborg", "Ålborg"),
            ("Strauß", "Strauss"),
            ("Rossi", "Roßi"),
        ] {
            assert!(same(k, t), "{k} ~ {t}");
        }
    }

    #[test]
    fn bare_vowel_does_not_match_umlaut() {
        for (k, t) in [("Ole", "Öle"), ("Schon", "schön"), ("Kuhn", "kühn"), ("Mueller", "Muller"), ("Ole", "Ølé")] {
            assert!(!same(k, t), "{k} !~ {t}");
        }
        assert!(!same("Kuhn", "Ku\u{308}hn"));
        assert!(!same("Anna", "Anne"));
    }

    #[test]
    fn loose_is_coarser_than_pattern() {
        for (k, t) in [("Gül", "Gul"), ("Gül", "Guel"), ("Mueller", "Müller"), ("Strauß", "Strauss"), ("Ångström", "Aangstroem"), ("Isaac", "Isåc"), ("José", "Jose\u{301}"), ("Søren", "Sören")] {
            assert!(same(k, t), "{k} ~ {t}");
            assert_eq!(loose(k), loose(t), "{k} ~ {t}");
        }
    }

    #[test]
    fn fold_normalizes() {
        assert_eq!(fold("Müller"), "mueller");
        assert_eq!(fold("MUELLER"), "mueller");
        assert_eq!(fold("Mu\u{308}ller"), "mueller");
        assert_eq!(fold("Strauß"), "strauss");
        assert_eq!(fold("José García"), "jose garcia");
        assert_eq!(fold("Søren Łukasz"), "soeren lukasz");
    }
}
