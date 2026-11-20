//! Reine Textformate: Text, Markdown, CSV, JSON, HTML/XML, LaTeX und Quellcode.
//!
//! - Die Zeichenkodierung bleibt erhalten: UTF-8 (mit oder ohne BOM), UTF-16
//!   mit BOM und Windows-1252 (CSV aus Excel unter Windows) werden erkannt und
//!   genauso zurückgeschrieben.
//! - Maskierte Namen werden gefunden: «M&uuml;ller» in HTML/XML/Markdown,
//!   «Müller» in JSON, «M\"uller» in LaTeX.
//! - JSON wird nicht neu formatiert: ersetzt wird nur in String-Literalen
//!   (Werte und Schlüssel), lange Base64-Daten (Bilder in Notebooks) bleiben unberührt.

use crate::fuzzy::{find_fuzzy, FuzzyHit, DEFAULT_THRESHOLD};
use crate::matcher::build_matcher;
use crate::textview::{self, Decode, Hits, Seg};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    /// Text, Quellcode: unverändert durchsuchen
    Plain,
    /// CSV/TSV: wie Plain; neue Umlaute in einer reinen ASCII-Datei → UTF-8 mit BOM,
    /// damit Excel sie richtig anzeigt
    Csv,
    /// Markdown: auch HTML-Entitäten, Ersetzung unmaskiert
    Markdown,
    /// HTML: HTML-Entitäten, Ersetzung XML-maskiert
    Html,
    /// XML/SVG: XML-Entitäten
    Xml,
    /// JSON/Notebook: nur String-Literale
    Json,
    /// LaTeX/BibTeX: Akzent-Makros
    Tex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

/// Zeichen 0x80–0x9F in Windows-1252 (None: nicht belegt).
const CP1252_HIGH: [Option<char>; 32] = [
    Some('€'), None, Some('‚'), Some('ƒ'), Some('„'), Some('…'), Some('†'), Some('‡'),
    Some('ˆ'), Some('‰'), Some('Š'), Some('‹'), Some('Œ'), None, Some('Ž'), None,
    None, Some('‘'), Some('’'), Some('“'), Some('”'), Some('•'), Some('–'), Some('—'),
    Some('˜'), Some('™'), Some('š'), Some('›'), Some('œ'), None, Some('ž'), Some('Ÿ'),
];

pub(crate) fn decode(bytes: &[u8]) -> (String, Encoding) {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return (String::from_utf8_lossy(rest).into_owned(), Encoding::Utf8Bom);
    }
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| if le { u16::from_le_bytes(*c) } else { u16::from_be_bytes(*c) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        return (utf16(rest, true), Encoding::Utf16Le);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        return (utf16(rest, false), Encoding::Utf16Be);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), Encoding::Utf8),
        Err(_) => {
            let s = bytes
                .iter()
                .map(|&b| match b {
                    0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize].unwrap_or(char::from(b)),
                    _ => char::from(b),
                })
                .collect();
            (s, Encoding::Windows1252)
        }
    }
}

pub(crate) fn encode(s: &str, enc: Encoding) -> Vec<u8> {
    match enc {
        Encoding::Utf8 => s.as_bytes().to_vec(),
        Encoding::Utf8Bom => [b"\xEF\xBB\xBF".as_slice(), s.as_bytes()].concat(),
        Encoding::Utf16Le => [0xFF, 0xFE].into_iter().chain(s.encode_utf16().flat_map(u16::to_le_bytes)).collect(),
        Encoding::Utf16Be => [0xFE, 0xFF].into_iter().chain(s.encode_utf16().flat_map(u16::to_be_bytes)).collect(),
        Encoding::Windows1252 => s
            .chars()
            .map(|c| match c as u32 {
                n @ (0..=0x7F | 0xA0..=0xFF) => n as u8,
                // 0x80–0x9F: unbelegte Bytes wurden 1:1 gelesen
                n @ 0x80..=0x9F => n as u8,
                _ => CP1252_HIGH
                    .iter()
                    .position(|h| *h == Some(c))
                    .map_or(b'?', |i| 0x80 + i as u8),
            })
            .collect(),
    }
}

/// Abschnitte, in denen gesucht wird.
fn groups(text: &str, kind: TextKind) -> Vec<Vec<Seg>> {
    let whole = |d| vec![vec![Seg::new(0, text.len(), d)]];
    match kind {
        TextKind::Plain | TextKind::Csv => whole(Decode::Raw),
        TextKind::Markdown | TextKind::Html => whole(Decode::Html),
        TextKind::Xml => whole(Decode::Xml),
        TextKind::Tex => whole(Decode::Tex),
        TextKind::Json => json_strings(text).into_iter().map(|s| vec![s]).collect(),
    }
}

/// Inhalte aller JSON-String-Literale (ohne Anführungszeichen). Lange
/// Base64-Blöcke (eingebettete Bilder) werden ausgelassen: dort wäre ein
/// «Treffer» Zufall und würde das Bild zerstören.
fn json_strings(text: &str) -> Vec<Seg> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < b.len() && b[j] != b'"' {
            j += if b[j] == b'\\' { 2 } else { 1 };
        }
        let end = j.min(b.len());
        let content = &text[start..end];
        let base64_like = content.len() > 200
            && content.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=' | b'\\' | b'n'));
        if !base64_like {
            out.push(Seg::new(start, end, Decode::Json));
        }
        i = end + 1;
    }
    out
}

fn escape_for(kind: TextKind) -> fn(&str) -> String {
    match kind {
        TextKind::Html | TextKind::Xml => textview::escape_xml_attr,
        TextKind::Json => textview::escape_json,
        TextKind::Plain | TextKind::Csv | TextKind::Markdown | TextKind::Tex => textview::no_escape,
    }
}

/// Ersetzt im Text; None, wenn nichts ersetzt wurde.
pub(crate) fn replace_text(text: &str, kind: TextKind, mapping: &[(String, String)], hits: &mut Hits) -> Result<Option<String>, regex::Error> {
    if mapping.is_empty() {
        return Ok(None);
    }
    let matcher = build_matcher(mapping)?;
    Ok(textview::replace_groups(text, &groups(text, kind), &matcher, &escape_for(kind), hits))
}

pub(crate) fn process_bytes(bytes: &[u8], kind: TextKind, mapping: &[(String, String)], hits: &mut Hits) -> Result<Vec<u8>, regex::Error> {
    let (text, enc) = decode(bytes);
    Ok(match replace_text(&text, kind, mapping, hits)? {
        Some(new) => {
            // Excel liest UTF-8-CSV ohne BOM als Windows-1252 («MÃ¼ller»)
            let enc = if kind == TextKind::Csv && enc == Encoding::Utf8 && text.is_ascii() && !new.is_ascii() { Encoding::Utf8Bom } else { enc };
            encode(&new, enc)
        }
        None => bytes.to_vec(),
    })
}

pub(crate) fn process_text_file(input: &Path, output: &Path, kind: TextKind, mapping: &[(String, String)]) -> Result<(usize, std::collections::HashMap<String, usize>), crate::formats::FormatError> {
    let bytes = std::fs::read(input)?;
    let mut hits = Hits::default();
    let out = process_bytes(&bytes, kind, mapping, &mut hits)?;
    std::fs::write(output, out)?;
    Ok((hits.replacements, hits.per_identifier))
}

/// Tippfehler-Suche zeilenweise; Fundstelle «Zeile N».
pub(crate) fn scan_fuzzy_text(input: &Path, kind: TextKind, mapping: &[(String, String)]) -> Result<Vec<FuzzyHit>, std::io::Error> {
    let (text, _) = decode(&std::fs::read(input)?);
    Ok(scan_text(&text, kind, mapping))
}

pub(crate) fn scan_text(text: &str, kind: TextKind, mapping: &[(String, String)]) -> Vec<FuzzyHit> {
    let mut hits = Vec::new();
    let markup = matches!(kind, TextKind::Html | TextKind::Xml);
    for group in groups(text, kind) {
        let (decoded, raw_at) = textview::group_text(text, &group);
        let scan = if markup { blank_tags(&decoded) } else { decoded.clone() };
        let mut line_start = 0;
        for line in scan.split('\n') {
            for mut h in find_fuzzy(line, mapping, DEFAULT_THRESHOLD) {
                let raw_pos = raw_at[line_start];
                h.location = format!("Zeile {}", text[..raw_pos].matches('\n').count() + 1);
                hits.push(h);
            }
            line_start += line.len() + 1;
        }
    }
    hits
}

/// Ersetzt `<…>` durch Leerzeichen (gleiche Bytelänge), damit «<td>Müler</td>»
/// als eigenes Wort gefunden wird.
fn blank_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        }
        if in_tag {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        } else {
            out.push(c);
        }
        if c == '>' {
            in_tag = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Vec<(String, String)> {
        vec![
            ("Anna Müller".into(), "Lea Roth".into()),
            ("Müller, Anna".into(), "Roth, Lea".into()),
            ("Anna".into(), "Lea".into()),
            ("Müller".into(), "Roth".into()),
            ("anna.mueller@schule.ch".into(), "lea.roth@anonym.invalid".into()),
        ]
    }

    fn run(text: &str, kind: TextKind) -> String {
        replace_text(text, kind, &map(), &mut Hits::default()).unwrap().unwrap_or_else(|| text.to_string())
    }

    #[test]
    fn plain_csv_and_code() {
        assert_eq!(run("Name;Note\nMüller, Anna;5.5\n", TextKind::Plain), "Name;Note\nRoth, Lea;5.5\n");
        assert_eq!(run("# Autor: Anna Müller <anna.mueller@schule.ch>\nprint('Hallo')", TextKind::Plain), "# Autor: Lea Roth <lea.roth@anonym.invalid>\nprint('Hallo')");
    }

    #[test]
    fn json_keys_values_and_escapes() {
        let src = "{\n  \"Anna M\\u00fcller\": {\"note\": 5.5, \"text\": \"von \\\"Anna\\\"\"},\n  \"x\": 1\n}";
        let out = run(src, TextKind::Json);
        assert_eq!(out, "{\n  \"Lea Roth\": {\"note\": 5.5, \"text\": \"von \\\"Lea\\\"\"},\n  \"x\": 1\n}");
        assert!(serde_json::from_str::<serde_json::Value>(&out).is_ok());
        // Base64 bleibt unberührt, auch wenn zufällig «Anna» darin steht
        let b64 = format!("{{\"png\": \"{}/Anna/{}\"}}", "A".repeat(150), "B".repeat(150));
        assert_eq!(run(&b64, TextKind::Json), b64);
    }

    #[test]
    fn html_entities_and_markdown() {
        assert_eq!(run("<td title=\"Anna\">M&uuml;ller</td>", TextKind::Html), "<td title=\"Lea\">Roth</td>");
        assert_eq!(run("**Autorin:** Anna M&uuml;ller", TextKind::Markdown), "**Autorin:** Lea Roth");
        assert_eq!(run("\\author{Anna M\\\"{u}ller}", TextKind::Tex), "\\author{Lea Roth}");
    }

    #[test]
    fn encodings_roundtrip() {
        // Windows-1252: ü = 0xFC
        let cp = b"M\xFCller, Anna;6\r\n";
        let mut hits = Hits::default();
        let out = process_bytes(cp, TextKind::Plain, &map(), &mut hits).unwrap();
        assert_eq!(out, b"Roth, Lea;6\r\n");
        let back = process_bytes(b"Roth \x80", TextKind::Plain, &[("Roth".into(), "Müller".into())], &mut hits).unwrap();
        assert_eq!(back, b"M\xFCller \x80", "Kodierung bleibt Windows-1252");

        // ASCII-CSV bekommt Umlaute zurück: mit BOM, damit Excel sie lesen kann
        let back = process_bytes(b"Roth, Lea;6\r\n", TextKind::Csv, &[("Roth, Lea".into(), "Müller, Anna".into())], &mut hits).unwrap();
        assert_eq!(back, "\u{feff}Müller, Anna;6\r\n".as_bytes());
        let plain = process_bytes(b"Roth", TextKind::Plain, &[("Roth".into(), "Müller".into())], &mut hits).unwrap();
        assert_eq!(plain, "Müller".as_bytes(), "andere Textdateien ohne BOM");

        for enc in [Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be] {
            let bytes = encode("Anna Müller\n", enc);
            let out = process_bytes(&bytes, TextKind::Plain, &map(), &mut hits).unwrap();
            let (text, enc2) = decode(&out);
            assert_eq!((text.as_str(), enc2), ("Lea Roth\n", enc));
        }
    }

    #[test]
    fn fuzzy_with_line_numbers() {
        let text = "Titel\n<p>Abgabe von <b>Müler</b></p>\n";
        let hits = scan_text(text, TextKind::Html, &map());
        assert!(hits.iter().any(|h| h.matched == "Müler" && h.location == "Zeile 2"), "{hits:?}");
    }
}
