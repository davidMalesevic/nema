//! Ersetzen in decodiertem Text mit Rückabbildung auf den Rohtext.
//!
//! Viele Formate speichern Namen maskiert: «M&uuml;ller» (HTML), «Müller»
//! (JSON), «M\"uller» (LaTeX). Gesucht wird deshalb im decodierten Text; für
//! jede Byte-Position darin ist bekannt, wo sie im Rohtext steht. Ersetzt wird
//! nur der Rohbereich des Treffers, alles andere bleibt Byte für Byte erhalten.
//!
//! Ein Abschnitt (`Seg`) ist ein Rohbereich samt Decodierung. Abschnitte einer
//! Gruppe werden zusammenhängend durchsucht (z.B. die Text-Runs eines Absatzes,
//! über die ein Name verteilt sein kann). Jede Rohstelle wird höchstens einmal
//! ersetzt; ein Pseudonym, das zufällig auch ein echter Vorname ist, wird also
//! nie ein zweites Mal übersetzt.

use crate::matcher::Matcher;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decode {
    /// unverändert
    Raw,
    /// XML-Entitäten (&amp; &lt; &gt; &quot; &apos; &#..;)
    Xml,
    /// XML plus benannte HTML-Entitäten (&uuml; &nbsp; …)
    Html,
    /// Inhalt eines JSON-Strings (\" \\ \n ü …)
    Json,
    /// LaTeX-Akzente (\"u, \"{u}, {\"u}, \ss{} …)
    Tex,
    /// Leerer Rohbereich, der als ein Leerzeichen zählt (z.B. <text:s/>, <a:br/>):
    /// trennt Wörter, ohne dass im Rohtext etwas steht.
    Space,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Seg {
    pub start: usize,
    pub end: usize,
    pub decode: Decode,
}

impl Seg {
    pub fn new(start: usize, end: usize, decode: Decode) -> Self {
        Self { start, end, decode }
    }
}

/// Decodierter Text eines Abschnitts. `raw_at[i]` ist die Rohposition des
/// decodierten Bytes `i`; `raw_at[text.len()]` ist das Abschnittsende.
pub(crate) struct View {
    pub text: String,
    pub raw_at: Vec<usize>,
}

impl View {
    fn push(&mut self, c: char, raw_pos: usize) {
        for _ in 0..c.len_utf8() {
            self.raw_at.push(raw_pos);
        }
        self.text.push(c);
    }
}

pub(crate) fn view(raw: &str, seg: Seg) -> View {
    let mut v = View { text: String::new(), raw_at: Vec::new() };
    if seg.decode == Decode::Space {
        v.push(' ', seg.start);
        v.raw_at.push(seg.end);
        return v;
    }
    let s = &raw[seg.start..seg.end];
    let mut i = 0;
    while i < s.len() {
        let decoded = match seg.decode {
            Decode::Raw | Decode::Space => None,
            Decode::Xml => decode_entity(&s[i..], false),
            Decode::Html => decode_entity(&s[i..], true),
            Decode::Json => decode_json_escape(&s[i..]),
            Decode::Tex => decode_tex(&s[i..]),
        };
        let (c, len) = decoded.unwrap_or_else(|| {
            let c = s[i..].chars().next().expect("char boundary");
            (c, c.len_utf8())
        });
        v.push(c, seg.start + i);
        i += len;
    }
    v.raw_at.push(seg.end);
    v
}

/// Treffer-Statistik je Datei.
#[derive(Debug, Default, Clone)]
pub(crate) struct Hits {
    pub replacements: usize,
    pub per_identifier: HashMap<String, usize>,
}

impl Hits {
    pub fn add(&mut self, key: &str) {
        self.replacements += 1;
        *self.per_identifier.entry(key.to_string()).or_insert(0) += 1;
    }
}

/// Ersetzt alle Treffer in den Gruppen. `escape` maskiert die Ersetzung fürs
/// Zielformat. None, wenn nichts ersetzt wurde.
pub(crate) fn replace_groups(
    raw: &str,
    groups: &[Vec<Seg>],
    matcher: &Matcher,
    escape: &dyn Fn(&str) -> String,
    hits: &mut Hits,
) -> Option<String> {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for group in groups {
        let views: Vec<View> = group.iter().map(|s| view(raw, *s)).collect();
        let mut full = String::new();
        let mut starts = Vec::with_capacity(views.len());
        for v in &views {
            starts.push(full.len());
            full.push_str(&v.text);
        }
        for f in matcher.find_all(&full) {
            hits.add(&f.key);
            let mut first = true;
            for (k, v) in views.iter().enumerate() {
                let (ds, de) = (starts[k], starts[k] + v.text.len());
                if f.end <= ds || f.start >= de {
                    continue;
                }
                let ls = f.start.max(ds) - ds;
                let le = f.end.min(de) - ds;
                let repl = if first { escape(&f.repl) } else { String::new() };
                first = false;
                edits.push((v.raw_at[ls], v.raw_at[le], repl));
            }
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(raw.len());
    let mut last = 0;
    for (s, e, r) in edits {
        if s < last {
            continue; // kann bei disjunkten Abschnitten nicht vorkommen
        }
        out.push_str(&raw[last..s]);
        out.push_str(&r);
        last = e;
    }
    out.push_str(&raw[last..]);
    Some(out)
}

/// Decodierter Text einer Gruppe samt Rohposition jedes Bytes (für die
/// Tippfehler-Suche: Zeilennummern).
pub(crate) fn group_text(raw: &str, group: &[Seg]) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut raw_at = Vec::new();
    for s in group {
        let mut v = view(raw, *s);
        v.raw_at.pop();
        text.push_str(&v.text);
        raw_at.extend(v.raw_at);
    }
    raw_at.push(group.last().map_or(0, |s| s.end));
    (text, raw_at)
}

// ------------------------------------------------------------ Escapes

pub(crate) fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn no_escape(s: &str) -> String {
    s.to_string()
}

// ------------------------------------------------------------ Decoder

/// Benannte HTML-Entitäten für Buchstaben, die in Namen vorkommen.
/// Latin-1 ab U+00C0 in Codepunkt-Reihenfolge (× und ÷ eingeschlossen).
const LATIN1_LETTERS: [&str; 64] = [
    "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave", "Eacute", "Ecirc", "Euml",
    "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve", "Oacute", "Ocirc", "Otilde", "Ouml", "times",
    "Oslash", "Ugrave", "Uacute", "Ucirc", "Uuml", "Yacute", "THORN", "szlig", "agrave", "aacute", "acirc", "atilde",
    "auml", "aring", "aelig", "ccedil", "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc", "iuml",
    "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash", "ugrave", "uacute", "ucirc",
    "uuml", "yacute", "thorn", "yuml",
];

const HTML_OTHER: [(&str, char); 22] = [
    ("nbsp", '\u{a0}'),
    ("shy", '\u{ad}'),
    ("rsquo", '’'),
    ("lsquo", '‘'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("Scaron", 'Š'),
    ("scaron", 'š'),
    ("Zcaron", 'Ž'),
    ("zcaron", 'ž'),
    ("Ccaron", 'Č'),
    ("ccaron", 'č'),
    ("Cacute", 'Ć'),
    ("cacute", 'ć'),
    ("Dstrok", 'Đ'),
    ("dstrok", 'đ'),
    ("OElig", 'Œ'),
    ("oelig", 'œ'),
    ("Yuml", 'Ÿ'),
    ("Lstrok", 'Ł'),
    ("lstrok", 'ł'),
    ("Rcaron", 'Ř'),
];

/// `&…;` am Anfang von `s` → (Zeichen, Rohlänge).
fn decode_entity(s: &str, html: bool) -> Option<(char, usize)> {
    let rest = s.strip_prefix('&')?;
    let semi = rest.find(';')?;
    if semi == 0 || semi > 10 {
        return None;
    }
    let name = &rest[..semi];
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        n if n.starts_with("#x") || n.starts_with("#X") => char::from_u32(u32::from_str_radix(&n[2..], 16).ok()?)?,
        n if n.starts_with('#') => char::from_u32(n[1..].parse().ok()?)?,
        n if html => {
            if let Some(i) = LATIN1_LETTERS.iter().position(|e| *e == n) {
                char::from_u32(0xC0 + i as u32)?
            } else {
                HTML_OTHER.iter().find(|(e, _)| *e == n)?.1
            }
        }
        _ => return None,
    };
    Some((c, semi + 2))
}

fn decode_json_escape(s: &str) -> Option<(char, usize)> {
    let rest = s.strip_prefix('\\')?;
    let c = match rest.chars().next()? {
        '"' => '"',
        '\\' => '\\',
        '/' => '/',
        'b' => '\u{8}',
        'f' => '\u{c}',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'u' => {
            let hi = u32::from_str_radix(rest.get(1..5)?, 16).ok()?;
            if (0xD800..0xDC00).contains(&hi) {
                // Ersatzpaar 😀
                let lo_part = rest.get(5..11)?;
                let lo = u32::from_str_radix(lo_part.strip_prefix("\\u")?, 16).ok()?;
                if !(0xDC00..0xE000).contains(&lo) {
                    return None;
                }
                let c = char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))?;
                return Some((c, 12));
            }
            return Some((char::from_u32(hi)?, 6));
        }
        _ => return None,
    };
    Some((c, 2))
}

fn tex_accent(accent: char, base: char) -> Option<char> {
    let c = match (accent, base) {
        ('"', 'a') => 'ä',
        ('"', 'o') => 'ö',
        ('"', 'u') => 'ü',
        ('"', 'A') => 'Ä',
        ('"', 'O') => 'Ö',
        ('"', 'U') => 'Ü',
        ('"', 'e') => 'ë',
        ('"', 'i') => 'ï',
        ('\'', 'a') => 'á',
        ('\'', 'e') => 'é',
        ('\'', 'i') => 'í',
        ('\'', 'o') => 'ó',
        ('\'', 'u') => 'ú',
        ('\'', 'c') => 'ć',
        ('\'', 'C') => 'Ć',
        ('\'', 'E') => 'É',
        ('`', 'a') => 'à',
        ('`', 'e') => 'è',
        ('`', 'u') => 'ù',
        ('`', 'E') => 'È',
        ('^', 'a') => 'â',
        ('^', 'e') => 'ê',
        ('^', 'i') => 'î',
        ('^', 'o') => 'ô',
        ('^', 'u') => 'û',
        ('~', 'n') => 'ñ',
        ('~', 'a') => 'ã',
        ('~', 'o') => 'õ',
        ('c', 'c') => 'ç',
        ('c', 'C') => 'Ç',
        ('v', 's') => 'š',
        ('v', 'S') => 'Š',
        ('v', 'c') => 'č',
        ('v', 'C') => 'Č',
        ('v', 'z') => 'ž',
        ('v', 'Z') => 'Ž',
        _ => return None,
    };
    Some(c)
}

/// LaTeX-Akzent am Anfang von `s`: `\"u`, `\"{u}`, `{\"u}`, `\v{s}`, `\ss{}`, `{\ss}`.
fn decode_tex(s: &str) -> Option<(char, usize)> {
    let b = s.as_bytes();
    let (inner, braced) = if b.first() == Some(&b'{') { (&s[1..], true) } else { (s, false) };
    let rest = inner.strip_prefix('\\')?;
    let mut chars = rest.chars();
    let accent = chars.next()?;
    let (c, len) = if rest.starts_with("ss") && !rest[2..].starts_with(|c: char| c.is_alphabetic()) {
        let extra = if !braced && rest[2..].starts_with("{}") { 2 } else { 0 };
        ('ß', 1 + 2 + extra)
    } else if matches!(accent, '"' | '\'' | '`' | '^' | '~' | 'c' | 'v') {
        let after = &rest[1..];
        if let Some(x) = after.strip_prefix('{') {
            let base = x.chars().next()?;
            if !x[base.len_utf8()..].starts_with('}') {
                return None;
            }
            (tex_accent(accent, base)?, 1 + 1 + 1 + base.len_utf8() + 1)
        } else if matches!(accent, 'c' | 'v') {
            return None; // \c und \v nur mit Klammern (sonst wäre «\cite» ein Akzent)
        } else {
            let base = after.chars().next()?;
            (tex_accent(accent, base)?, 1 + 1 + base.len_utf8())
        }
    } else {
        return None;
    };
    if braced {
        if !s[1 + len..].starts_with('}') {
            return None;
        }
        Some((c, len + 2))
    } else {
        Some((c, len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::build_matcher;

    fn decoded(s: &str, d: Decode) -> String {
        view(s, Seg::new(0, s.len(), d)).text
    }

    #[test]
    fn decoders() {
        assert_eq!(decoded("M&uuml;ller &amp; M&#252;ller &#xFC; &bogus; &", Decode::Html), "Müller & Müller ü &bogus; &");
        assert_eq!(decoded("M&uuml;ller", Decode::Xml), "M&uuml;ller");
        assert_eq!(decoded(r#"Müller \"x\" 😀 \q"#, Decode::Json), "Müller \"x\" 😀 \\q");
        assert_eq!(decoded(r#"M\"uller M\"{u}ller M{\"u}ller Stra\ss{}e {\ss} Male\v{s}evi\'c \cite"#, Decode::Tex), "Müller Müller Müller Straße ß Malešević \\cite");
    }

    #[test]
    fn replaces_only_raw_range() {
        let raw = "<p>M&uuml;ller &amp; Co, Anna</p>";
        let m = build_matcher(&[("Müller".into(), "Roth".into()), ("Anna".into(), "Lea".into())]).unwrap();
        let mut hits = Hits::default();
        let out = replace_groups(raw, &[vec![Seg::new(0, raw.len(), Decode::Html)]], &m, &escape_xml_attr, &mut hits).unwrap();
        assert_eq!(out, "<p>Roth &amp; Co, Lea</p>");
        assert_eq!(hits.replacements, 2);
    }

    #[test]
    fn group_spans_segments_and_space() {
        // «Anna» | <br> | «Müller» – zwei Runs mit Zeilenumbruch-Element dazwischen
        let raw = "<r>An</r><r>na</r><br/><r>Müller</r>";
        let segs = vec![
            Seg::new(3, 5, Decode::Xml),
            Seg::new(12, 14, Decode::Xml),
            Seg::new(18, 18, Decode::Space),
            Seg::new(26, 33, Decode::Xml),
        ];
        assert_eq!(&raw[26..33], "Müller");
        let m = build_matcher(&[("Anna Müller".into(), "Lea Roth".into())]).unwrap();
        let mut hits = Hits::default();
        let out = replace_groups(raw, &[segs], &m, &escape_xml_attr, &mut hits).unwrap();
        assert_eq!(out, "<r>Lea Roth</r><r></r><br/><r></r>");
    }

    #[test]
    fn no_chain_replacement() {
        // Pseudonym von Anna ist «Ben», und Ben gibt es wirklich: nur einmal ersetzen
        let raw = "Anna und Ben";
        let m = build_matcher(&[("Anna".into(), "Ben".into()), ("Ben".into(), "Carla".into())]).unwrap();
        let out = replace_groups(raw, &[vec![Seg::new(0, raw.len(), Decode::Raw)]], &m, &no_escape, &mut Hits::default());
        assert_eq!(out.as_deref(), Some("Ben und Carla"));
    }
}
