//! Excel (.xlsx), PowerPoint (.pptx) und OpenDocument (.odt/.ods/.odp).
//!
//! Alle drei sind ZIP-Archive aus XML-Teilen. Jeder XML-Teil wird zerlegt in
//! - Absätze (`<a:p>`, `<si>`, `<text:p>` …), deren Text-Runs zusammen
//!   durchsucht werden, weil ein Name über mehrere Runs verteilt sein kann,
//! - alle übrigen Textknoten und
//! - alle Attributwerte: Blattnamen, Alt-Texte von Bildern, Kommentar-Autoren,
//!   Hyperlinks (mailto:…), Pivot-Caches.
//!
//! Metadaten (Autor, zuletzt geändert von …) werden geleert, Vorschaubilder
//! (zeigen die erste Seite mit Namen) entfernt.
//!
//! Excel: Zahlenzellen bleiben unangetastet (sonst wäre die Datei kaputt).
//! Steht in einer Zahlenzelle genau eine Matrikelnummer, wird die Zelle zu
//! einer Textzelle mit dem Pseudonym.

use crate::fuzzy::{find_fuzzy, FuzzyHit, DEFAULT_THRESHOLD};
use crate::matcher::{build_matcher, Matcher};
use crate::textview::{self, Decode, Hits, Seg};
use regex::Regex;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeKind {
    Xlsx,
    Pptx,
    /// OpenDocument: Text, Tabelle, Präsentation
    Odf,
}

/// Wie ein XML-Teil zerlegt wird.
pub(crate) struct XmlCfg {
    /// Absatz-Elemente: Text darin wird zusammenhängend durchsucht.
    pub para: &'static [&'static str],
    /// Text-Elemente innerhalb eines Absatzes (leer: jeder Textknoten).
    pub text: &'static [&'static str],
    /// Textknoten durchsuchen (sonst nur Attribute)
    pub text_nodes: bool,
    /// Attribute durchsuchen
    pub attrs: bool,
    /// Nur diese Attribute (leer: alle ausser `skip_attrs` und xmlns)
    pub only_attrs: &'static [&'static str],
    pub skip_attrs: &'static [&'static str],
    /// Excel-Tabellenblatt: `<v>` nur in Formel-Textzellen (t="str") durchsuchen
    pub xlsx_cells: bool,
}

const PPTX: XmlCfg = XmlCfg {
    para: &["a:p"],
    text: &["a:t"],
    text_nodes: true,
    attrs: true,
    only_attrs: &[],
    skip_attrs: &[],
    xlsx_cells: false,
};

const XLSX: XmlCfg = XmlCfg {
    para: &["si", "is", "text", "a:p"],
    text: &["t", "a:t"],
    text_nodes: true,
    attrs: true,
    only_attrs: &[],
    skip_attrs: &[],
    xlsx_cells: false,
};

const ODF: XmlCfg = XmlCfg {
    para: &["text:p", "text:h"],
    text: &[],
    text_nodes: true,
    attrs: true,
    only_attrs: &[],
    // Zahlen-/Datumswerte von Zellen: ein Pseudonym dort machte die Datei kaputt
    skip_attrs: &["office:value", "office:date-value", "office:time-value", "office:boolean-value"],
    xlsx_cells: false,
};

/// Elemente, die innerhalb eines Absatzes als Wortgrenze zählen.
const SPACE_ELEMENTS: [&str; 5] = ["s", "tab", "br", "line-break", "cr"];

/// Reine Zahl («12», «-3.5», «1.71E-2»)? In Attributen und Strukturwerten sind
/// das IDs, Indizes, Koordinaten oder Zähler: eine gleichlautende
/// Matrikelnummer dort zu ersetzen, machte die Datei kaputt. Excel schreibt
/// Diagramm-Layouts in Exponentialschreibweise.
fn is_number(s: &str) -> bool {
    fn unsigned(s: &str, dot: bool) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || (dot && c == '.')) && s.chars().any(|c| c.is_ascii_digit())
    }
    let s = s.trim();
    let s = s.strip_prefix(['-', '+']).unwrap_or(s);
    match s.split_once(['e', 'E']) {
        Some((mant, exp)) => unsigned(mant, true) && unsigned(exp.strip_prefix(['-', '+']).unwrap_or(exp), false),
        None => unsigned(s, true),
    }
}

/// Attribute, deren Wert sichtbarer Text ist (Blattname, Alt-Text) — dort
/// wird auch eine reine Zahl ersetzt.
const TEXT_ATTRS: [&str; 4] = ["name", "descr", "title", "displayName"];

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Passt `name` auf einen Eintrag? Einträge ohne Präfix vergleichen den lokalen
/// Namen (Excel-Dateien aus manchen Programmen schreiben `<x:si>`).
fn name_in(name: &str, list: &[&str]) -> bool {
    list.iter().any(|n| *n == name || (!n.contains(':') && local(name) == *n))
}

/// Zerlegt einen XML-Teil in Suchgruppen (siehe Moduldoku).
pub(crate) fn xml_groups(xml: &str, cfg: &XmlCfg) -> Vec<Vec<Seg>> {
    let b = xml.as_bytes();
    let len = b.len();
    let mut out: Vec<Vec<Seg>> = Vec::new();
    let mut stack: Vec<&str> = Vec::new();
    // offene Absätze: (Stapeltiefe, Abschnitte)
    let mut paras: Vec<(usize, Vec<Seg>)> = Vec::new();
    let mut cell_is_str = false;
    let find = |from: usize, pat: &str| xml[from..].find(pat).map_or(len, |e| from + e);

    let mut i = 0;
    while i < len {
        if b[i] != b'<' {
            // Textknoten
            let end = find(i, "<");
            let parent = stack.last().copied().unwrap_or("");
            let text = &xml[i..end];
            let skip_value = cfg.xlsx_cells && local(parent) == "v" && !cell_is_str;
            let visible = in_para_text(parent, text, cfg, &paras);
            // Zahlen ausserhalb von Fliesstext sind Strukturwerte (Seitenzahl, Diagramm-Cache …)
            let structural_number = !visible && is_number(text);
            if cfg.text_nodes && !skip_value && !structural_number && (!text.trim().is_empty() || visible) {
                push_text(&mut out, &mut paras, cfg, parent, Seg::new(i, end, Decode::Xml), text);
            }
            i = end;
            continue;
        }
        let rest = &xml[i..];
        if rest.starts_with("<!--") {
            let end = find(i + 4, "-->");
            if cfg.text_nodes && !xml[i + 4..end].trim().is_empty() {
                out.push(vec![Seg::new(i + 4, end, Decode::Raw)]);
            }
            i = (end + 3).min(len);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            let end = find(i + 9, "]]>");
            if cfg.text_nodes {
                let parent = stack.last().copied().unwrap_or("");
                push_text(&mut out, &mut paras, cfg, parent, Seg::new(i + 9, end, Decode::Raw), &xml[i + 9..end]);
            }
            i = (end + 3).min(len);
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            i = (find(i, ">") + 1).min(len);
            continue;
        }
        if rest.starts_with("</") {
            let gt = find(i, ">");
            let name = xml[i + 2..gt].trim();
            if let Some(pos) = stack.iter().rposition(|n| *n == name) {
                stack.truncate(pos);
            }
            while paras.last().is_some_and(|(d, _)| *d > stack.len()) {
                let (_, g) = paras.pop().expect("nicht leer");
                if !g.is_empty() {
                    out.push(g);
                }
            }
            i = (gt + 1).min(len);
            continue;
        }
        // Start-Tag
        let mut j = i + 1;
        while j < len && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
            j += 1;
        }
        let name = &xml[i + 1..j];
        let mut self_closing = false;
        let mut t_attr: Option<&str> = None;
        loop {
            while j < len && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= len {
                break;
            }
            if b[j] == b'>' {
                j += 1;
                break;
            }
            if xml[j..].starts_with("/>") {
                self_closing = true;
                j += 2;
                break;
            }
            let an_start = j;
            while j < len && b[j] != b'=' && !b[j].is_ascii_whitespace() && b[j] != b'>' && b[j] != b'/' {
                j += 1;
            }
            let attr = &xml[an_start..j];
            while j < len && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= len || b[j] != b'=' {
                if j < len && b[j] == b'/' && !xml[j..].starts_with("/>") {
                    j += 1;
                }
                if attr.is_empty() && j < len && b[j] != b'>' && b[j] != b'/' {
                    j += 1; // unbekanntes Zeichen überspringen
                }
                continue;
            }
            j += 1;
            while j < len && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= len || (b[j] != b'"' && b[j] != b'\'') {
                continue;
            }
            let quote = b[j] as char;
            let vs = j + 1;
            let ve = xml[vs..].find(quote).map_or(len, |e| vs + e);
            if attr == "t" || local(attr) == "t" {
                t_attr = Some(&xml[vs..ve]);
            }
            let wanted = if cfg.only_attrs.is_empty() {
                cfg.attrs && !attr.starts_with("xmlns") && !cfg.skip_attrs.contains(&attr)
            } else {
                name_in(attr, cfg.only_attrs)
            };
            if wanted && vs < ve && !(is_number(&xml[vs..ve]) && !TEXT_ATTRS.contains(&local(attr))) {
                out.push(vec![Seg::new(vs, ve, Decode::Xml)]);
            }
            j = (ve + 1).min(len);
        }
        if cfg.xlsx_cells && local(name) == "c" {
            cell_is_str = t_attr == Some("str");
        }
        if let Some((_, group)) = paras.last_mut() {
            if cfg.text_nodes && SPACE_ELEMENTS.contains(&local(name)) {
                group.push(Seg::new(j, j, Decode::Space));
            }
        }
        if !self_closing {
            stack.push(name);
            if cfg.text_nodes && name_in(name, cfg.para) {
                paras.push((stack.len(), Vec::new()));
            }
        }
        i = j;
    }
    while let Some((_, g)) = paras.pop() {
        if !g.is_empty() {
            out.push(g);
        }
    }
    out
}

/// Gehört ein Textknoten zum Text des offenen Absatzes? Auch reine Leerzeichen
/// zählen (ein Run mit « » trennt Vor- und Nachname), Einrückungen nicht.
fn in_para_text(parent: &str, text: &str, cfg: &XmlCfg, paras: &[(usize, Vec<Seg>)]) -> bool {
    if paras.is_empty() || text.is_empty() {
        return false;
    }
    if cfg.text.is_empty() {
        !(text.trim().is_empty() && text.contains('\n'))
    } else {
        name_in(parent, cfg.text)
    }
}

fn push_text(out: &mut Vec<Vec<Seg>>, paras: &mut [(usize, Vec<Seg>)], cfg: &XmlCfg, parent: &str, seg: Seg, text: &str) {
    let joined = in_para_text(parent, text, cfg, paras);
    match paras.last_mut() {
        Some((_, group)) if joined => group.push(seg),
        _ if !text.trim().is_empty() => out.push(vec![seg]),
        _ => {}
    }
}

fn cfg_for(kind: OfficeKind) -> &'static XmlCfg {
    match kind {
        OfficeKind::Xlsx => &XLSX,
        OfficeKind::Pptx => &PPTX,
        OfficeKind::Odf => &ODF,
    }
}

fn is_xml_part(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".xml") || n.ends_with(".rels") || n.ends_with(".vml")
}

/// Teile, die ganz wegfallen: Vorschaubilder und eigene Dokumenteigenschaften.
fn is_dropped(name: &str) -> bool {
    name.starts_with("docProps/thumbnail.") || name.starts_with("Thumbnails/") || name == "docProps/custom.xml"
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("gültiger Regex"))
}

/// Inhalte der Elemente leeren (Metadaten).
fn blank_elements(xml: &str, tags: &[&str]) -> String {
    let mut out = xml.to_string();
    for tag in tags {
        let re = Regex::new(&format!("(?s)<{tag}(\\s[^>]*)?>.*?</{tag}>")).expect("gültiger Regex");
        out = re.replace_all(&out, format!("<{tag}$1></{tag}>").as_str()).into_owned();
    }
    out
}

/// Verweise auf entfernte Teile (Vorschaubild, eigene Eigenschaften) aus `_rels/.rels` streichen.
pub(crate) fn drop_removed_rels(xml: &str) -> String {
    static REL: OnceLock<Regex> = OnceLock::new();
    re(&REL, r#"<Relationship\b[^>]*Target="/?docProps/(thumbnail\.[a-z]+|custom\.xml)"[^>]*/>"#).replace_all(xml, "").into_owned()
}

/// Metadaten und Verweise auf entfernte Teile bereinigen, bevor gesucht wird.
fn sanitize_part(name: &str, xml: &str) -> String {
    match name {
        "docProps/core.xml" => blank_elements(xml, &["dc:creator", "cp:lastModifiedBy", "dc:title", "dc:subject", "dc:description", "cp:keywords"]),
        "docProps/app.xml" => blank_elements(xml, &["Company", "Manager"]),
        "meta.xml" => {
            static USER_DEFINED: OnceLock<Regex> = OnceLock::new();
            let out = blank_elements(xml, &["meta:initial-creator", "dc:creator", "meta:printed-by", "dc:title", "dc:subject", "dc:description", "meta:keyword"]);
            re(&USER_DEFINED, r"(?s)<meta:user-defined\b[^>]*?(/>|>.*?</meta:user-defined>)").replace_all(&out, "").into_owned()
        }
        "_rels/.rels" => drop_removed_rels(xml),
        "META-INF/manifest.xml" => {
            static THUMB: OnceLock<Regex> = OnceLock::new();
            re(&THUMB, r#"<manifest:file-entry\b[^>]*manifest:full-path="Thumbnails/[^"]*"[^>]*/>"#).replace_all(xml, "").into_owned()
        }
        _ => xml.to_string(),
    }
}

/// Excel: Zahlenzelle, die genau eine Matrikelnummer enthält → Textzelle mit Pseudonym.
fn convert_numeric_ids(xml: &str, matcher: &Matcher, hits: &mut Hits) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    let cell = re(
        &CELL,
        r"<((?:[A-Za-z_][\w.-]*:)?)c(\s[^>]*?)?>\s*<(?:[A-Za-z_][\w.-]*:)?v>([^<]*)</(?:[A-Za-z_][\w.-]*:)?v>\s*</(?:[A-Za-z_][\w.-]*:)?c>",
    );
    static T_ATTR: OnceLock<Regex> = OnceLock::new();
    let t_attr = re(&T_ATTR, r#"\st="([^"]*)""#);
    cell.replace_all(xml, |c: &regex::Captures| {
        let whole = c[0].to_string();
        let prefix = &c[1];
        let attrs = c.get(2).map_or("", |m| m.as_str());
        if t_attr.captures(attrs).is_some_and(|t| &t[1] != "n") {
            return whole;
        }
        let value = c[3].trim();
        let found = matcher.find_all(value);
        match found.as_slice() {
            [f] if f.start == 0 && f.end == value.len() => {
                hits.add(&f.key);
                let attrs = t_attr.replace_all(attrs, "");
                let t = textview::escape_xml_attr(&f.repl);
                format!("<{prefix}c{attrs} t=\"inlineStr\"><{prefix}is><{prefix}t>{t}</{prefix}t></{prefix}is></{prefix}c>")
            }
            _ => whole,
        }
    })
    .into_owned()
}

/// OpenDocument-Tabelle: Zahlenzelle, deren Wert genau eine Matrikelnummer
/// ist → Textzelle (der Wert stünde sonst weiter im Attribut `office:value`;
/// den Text im Absatz ersetzt danach der normale Durchlauf).
fn convert_odf_numeric_ids(xml: &str, matcher: &Matcher) -> String {
    static CELL: OnceLock<Regex> = OnceLock::new();
    static VALUE: OnceLock<Regex> = OnceLock::new();
    static TYPE: OnceLock<Regex> = OnceLock::new();
    let cell = re(&CELL, r"<table:table-cell\b[^>]*>");
    let value = re(&VALUE, r#"\soffice:value="([^"]*)""#);
    let vtype = re(&TYPE, r#"((?:office|calcext):value-type)="float""#);
    cell.replace_all(xml, |c: &regex::Captures| {
        let tag = &c[0];
        let Some(v) = value.captures(tag) else { return tag.to_string() };
        let found = matcher.find_all(&v[1]);
        match found.as_slice() {
            [f] if f.start == 0 && f.end == v[1].len() && vtype.is_match(tag) => {
                let tag = value.replace(tag, "");
                vtype.replace_all(&tag, "$1=\"string\"").into_owned()
            }
            _ => tag.to_string(),
        }
    })
    .into_owned()
}

/// Ersetzt in einem XML-Teil. None, wenn sich nichts geändert hat.
fn transform_part(name: &str, xml: &str, kind: OfficeKind, matcher: &Matcher, hits: &mut Hits) -> Option<String> {
    let mut cur = sanitize_part(name, xml);
    let sheet = kind == OfficeKind::Xlsx && name.starts_with("xl/worksheets/");
    if sheet {
        cur = convert_numeric_ids(&cur, matcher, hits);
    }
    if kind == OfficeKind::Odf && name == "content.xml" {
        cur = convert_odf_numeric_ids(&cur, matcher);
    }
    let base = cfg_for(kind);
    let cfg = XmlCfg { xlsx_cells: sheet, ..*base };
    if let Some(new) = textview::replace_groups(&cur, &xml_groups(&cur, &cfg), matcher, &textview::escape_xml_attr, hits) {
        cur = new;
    }
    (cur != xml).then_some(cur)
}

/// Schreibt ein ZIP neu: `f(name, bytes)` liefert den neuen Inhalt, None = Teil weglassen.
/// Die Kompression je Eintrag bleibt (ODF verlangt `mimetype` unkomprimiert an erster Stelle).
pub(crate) fn rewrite_zip(
    input: &Path,
    output: &Path,
    mut f: impl FnMut(&str, Vec<u8>) -> Result<Option<Vec<u8>>, crate::formats::FormatError>,
) -> Result<(), crate::formats::FormatError> {
    let mut archive = ZipArchive::new(std::fs::File::open(input)?)?;
    let mut writer = ZipWriter::new(std::fs::File::create(output)?);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let method = if entry.compression() == CompressionMethod::Stored { CompressionMethod::Stored } else { CompressionMethod::Deflated };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        drop(entry);
        if let Some(new) = f(&name, buf)? {
            writer.start_file(name, SimpleFileOptions::default().compression_method(method))?;
            writer.write_all(&new)?;
        }
    }
    writer.finish()?;
    Ok(())
}

pub(crate) fn process_office(input: &Path, output: &Path, kind: OfficeKind, mapping: &[(String, String)]) -> Result<Hits, crate::formats::FormatError> {
    let matcher = build_matcher(mapping)?;
    let mut hits = Hits::default();
    rewrite_zip(input, output, |name, buf| {
        if is_dropped(name) {
            return Ok(None);
        }
        if !is_xml_part(name) {
            return Ok(Some(buf));
        }
        let Ok(xml) = String::from_utf8(buf.clone()) else { return Ok(Some(buf)) };
        Ok(Some(match transform_part(name, &xml, kind, &matcher, &mut hits) {
            Some(new) => new.into_bytes(),
            None => buf,
        }))
    })?;
    Ok(hits)
}

/// Teile mit sichtbarem Inhalt (für die Tippfehler-Suche; Designs, Schriften
/// und Metadaten würden nur Fehlalarme wie «Arial» ≈ «Ariel» liefern).
fn is_content_part(name: &str, kind: OfficeKind) -> bool {
    match kind {
        OfficeKind::Pptx => ["ppt/slides/", "ppt/notesSlides/", "ppt/comments/"].iter().any(|p| name.starts_with(p)),
        OfficeKind::Xlsx => ["xl/sharedStrings.xml", "xl/worksheets/", "xl/comments", "xl/threadedComments/", "xl/drawings/"]
            .iter()
            .any(|p| name.starts_with(p)),
        OfficeKind::Odf => name == "content.xml" || name == "styles.xml",
    }
}

/// Lesbare Fundstelle: «Folie 3», «Blatt 2», «Inhalt» …
fn friendly_part(name: &str) -> String {
    let num = |prefix: &str| {
        name.strip_prefix(prefix)
            .and_then(|r| r.strip_suffix(".xml"))
            .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
            .map(str::to_string)
    };
    if let Some(n) = num("ppt/slides/slide") {
        format!("Folie {n}")
    } else if let Some(n) = num("ppt/notesSlides/notesSlide") {
        format!("Notizen {n}")
    } else if let Some(n) = num("xl/worksheets/sheet") {
        format!("Blatt {n}")
    } else if name == "xl/sharedStrings.xml" {
        "Zelltexte".into()
    } else if name.contains("omments") {
        "Kommentare".into()
    } else if name == "content.xml" {
        "Inhalt".into()
    } else if name == "styles.xml" {
        "Kopf-/Fusszeilen".into()
    } else {
        name.to_string()
    }
}

pub(crate) fn scan_fuzzy_office(input: &Path, kind: OfficeKind, mapping: &[(String, String)]) -> Result<Vec<FuzzyHit>, crate::formats::FormatError> {
    let mut archive = ZipArchive::new(std::fs::File::open(input)?)?;
    let cfg = XmlCfg { attrs: false, ..*cfg_for(kind) };
    let mut hits = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !is_content_part(&name, kind) || !is_xml_part(&name) {
            continue;
        }
        let mut xml = String::new();
        if entry.read_to_string(&mut xml).is_err() {
            continue;
        }
        let sheet = kind == OfficeKind::Xlsx && name.starts_with("xl/worksheets/");
        let cfg = XmlCfg { xlsx_cells: sheet, ..cfg };
        for group in xml_groups(&xml, &cfg) {
            let (text, _) = textview::group_text(&xml, &group);
            for mut h in find_fuzzy(&text, mapping, DEFAULT_THRESHOLD) {
                h.location = friendly_part(&name);
                hits.push(h);
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    pub(crate) fn make_zip(path: &Path, parts: &[(&str, &str)]) {
        let mut w = ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, content) in parts {
            let method = if *name == "mimetype" { CompressionMethod::Stored } else { CompressionMethod::Deflated };
            w.start_file(*name, SimpleFileOptions::default().compression_method(method)).unwrap();
            w.write_all(content.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    pub(crate) fn read_part(path: &Path, part: &str) -> Option<String> {
        let mut a = ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut e = a.by_name(part).ok()?;
        let mut s = String::new();
        e.read_to_string(&mut s).unwrap();
        Some(s)
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("nema-office-test-{}-{name}", uuid::Uuid::new_v4()))
    }

    fn map() -> Vec<(String, String)> {
        vec![
            ("Anna Müller".into(), "Lea Roth".into()),
            ("Müller Anna".into(), "Roth Lea".into()),
            ("Müller".into(), "Roth".into()),
            ("anna.mueller@schule.ch".into(), "lea.roth@anonym.invalid".into()),
            ("123456".into(), "PN-0000ABCD".into()),
        ]
    }

    fn groups_text(xml: &str, cfg: &XmlCfg) -> Vec<String> {
        xml_groups(xml, cfg).iter().map(|g| textview::group_text(xml, g).0).collect()
    }

    #[test]
    fn tokenizer_groups() {
        let xml = r#"<?xml version="1.0"?><p:sld xmlns:a="x"><a:p><a:r><a:t>Anna</a:t></a:r><a:r><a:t> </a:t></a:r><a:r><a:t>M&#252;ller</a:t></a:r><a:br/><a:r><a:t>x</a:t></a:r></a:p>
  <p:cNvPr id="4" descr="Foto von Anna"/><!-- Kommentar --></p:sld>"#;
        let g = groups_text(xml, &PPTX);
        assert!(g.contains(&"Anna Müller x".to_string()), "{g:?}");
        assert!(g.contains(&"Foto von Anna".to_string()), "{g:?}");
        assert!(g.contains(&" Kommentar ".to_string()), "{g:?}");
        assert!(!g.contains(&"x".to_string()), "xmlns wird nicht durchsucht: {g:?}");
    }

    #[test]
    fn exponent_numbers_are_structural() {
        for n in ["1.7141916169281145E-2", "1E5", "-2.5e+3", "+1", "12", "-3.5"] {
            assert!(is_number(n), "{n}");
        }
        for n in ["", "E", "1E", "e5", "Anna", "1.2.3a", "1-2", "Nan"] {
            assert!(!is_number(n), "{n}");
        }
        let m = build_matcher(&[("1".into(), "4711".into())]).unwrap();
        let xml = r#"<c:chart><c:y val="1.7141916169281145E-2"/><c:v>1E-3</c:v><a:p><a:r><a:t>1</a:t></a:r></a:p></c:chart>"#;
        let out = textview::replace_groups(xml, &xml_groups(xml, &XLSX), &m, &textview::escape_xml_attr, &mut Hits::default()).unwrap();
        assert_eq!(out, r#"<c:chart><c:y val="1.7141916169281145E-2"/><c:v>1E-3</c:v><a:p><a:r><a:t>4711</a:t></a:r></a:p></c:chart>"#);
    }

    #[test]
    fn numbers_only_in_visible_text() {
        let m = build_matcher(&[("123456".into(), "PN-0000ABCD".into())]).unwrap();
        let xml = r#"<p:sld><a:off x="123456" y="5"/><p:cNvPr id="123456" name="123456"/><a:p><a:r><a:t>Matrikel 123456</a:t></a:r></a:p><c:v>123456</c:v><Pages>123456</Pages></p:sld>"#;
        let out = textview::replace_groups(xml, &xml_groups(xml, &PPTX), &m, &textview::escape_xml_attr, &mut Hits::default()).unwrap();
        assert_eq!(
            out,
            r#"<p:sld><a:off x="123456" y="5"/><p:cNvPr id="123456" name="PN-0000ABCD"/><a:p><a:r><a:t>Matrikel PN-0000ABCD</a:t></a:r></a:p><c:v>123456</c:v><Pages>123456</Pages></p:sld>"#
        );
    }

    #[test]
    fn pptx_slides_notes_alt_text_and_meta() {
        let inp = tmp("in.pptx");
        let out = tmp("out.pptx");
        make_zip(
            &inp,
            &[
                ("[Content_Types].xml", "<Types/>"),
                ("_rels/.rels", r#"<Relationships><Relationship Id="rId1" Target="ppt/presentation.xml"/><Relationship Id="rId2" Target="docProps/thumbnail.jpeg"/></Relationships>"#),
                ("docProps/thumbnail.jpeg", "JPEG"),
                ("docProps/core.xml", "<cp:coreProperties><dc:creator>Anna Müller</dc:creator><dc:date>2026</dc:date></cp:coreProperties>"),
                ("ppt/slides/slide1.xml", r#"<p:sld><a:p><a:r><a:t>Anna </a:t></a:r><a:r><a:rPr b="1"/><a:t>Müller</a:t></a:r></a:p><p:cNvPr id="2" descr="Anna Müller am Pult"/></p:sld>"#),
                ("ppt/notesSlides/notesSlide1.xml", "<p:notes><a:p><a:r><a:t>Mail an anna.mueller@schule.ch</a:t></a:r></a:p></p:notes>"),
                ("ppt/slides/_rels/slide1.xml.rels", r#"<Relationships><Relationship Id="rId9" TargetMode="External" Target="mailto:anna.mueller@schule.ch"/></Relationships>"#),
            ],
        );
        let hits = process_office(&inp, &out, OfficeKind::Pptx, &map()).unwrap();
        let slide = read_part(&out, "ppt/slides/slide1.xml").unwrap();
        assert_eq!(slide, r#"<p:sld><a:p><a:r><a:t>Lea Roth</a:t></a:r><a:r><a:rPr b="1"/><a:t></a:t></a:r></a:p><p:cNvPr id="2" descr="Lea Roth am Pult"/></p:sld>"#);
        assert!(read_part(&out, "ppt/notesSlides/notesSlide1.xml").unwrap().contains("lea.roth@anonym.invalid"));
        assert!(read_part(&out, "ppt/slides/_rels/slide1.xml.rels").unwrap().contains("mailto:lea.roth@anonym.invalid"));
        assert!(read_part(&out, "docProps/thumbnail.jpeg").is_none(), "Vorschaubild entfernt");
        assert!(!read_part(&out, "_rels/.rels").unwrap().contains("thumbnail"));
        let core = read_part(&out, "docProps/core.xml").unwrap();
        assert!(!core.contains("Müller") && core.contains("2026"), "{core}");
        assert_eq!(hits.replacements, 4);
        for f in [&inp, &out] {
            let _ = std::fs::remove_file(f);
        }
    }

    #[test]
    fn xlsx_strings_sheet_names_and_numbers() {
        let inp = tmp("in.xlsx");
        let out = tmp("out.xlsx");
        make_zip(
            &inp,
            &[
                ("xl/workbook.xml", r#"<workbook><sheets><sheet name="Müller Anna" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="x">'Müller Anna'!$A$1</definedName></definedNames></workbook>"#),
                ("xl/sharedStrings.xml", r#"<sst count="2"><si><t>Anna Müller</t></si><si><r><t>Anna</t></r><r><rPr><b/></rPr><t xml:space="preserve"> Müller</t></r></si></sst>"#),
                (
                    "xl/worksheets/sheet1.xml",
                    r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>123456</v></c><c r="C1" s="2"><v>1234567</v></c><c r="D1" t="str"><f>A1</f><v>Anna Müller</v></c><c r="E1" t="inlineStr"><is><t>Müller</t></is></c></row></sheetData></worksheet>"#,
                ),
            ],
        );
        process_office(&inp, &out, OfficeKind::Xlsx, &map()).unwrap();
        let wb = read_part(&out, "xl/workbook.xml").unwrap();
        assert!(wb.contains(r#"name="Roth Lea""#) && wb.contains("'Roth Lea'!$A$1"), "{wb}");
        let ss = read_part(&out, "xl/sharedStrings.xml").unwrap();
        assert_eq!(ss.matches("Lea Roth").count(), 2, "{ss}");
        assert!(!ss.contains("Müller"), "{ss}");
        let sheet = read_part(&out, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<c r="A1" t="s"><v>0</v></c>"#), "Index bleibt: {sheet}");
        assert!(sheet.contains(r#"<c r="B1" t="inlineStr"><is><t>PN-0000ABCD</t></is></c>"#), "Matrikel: {sheet}");
        assert!(sheet.contains("<v>1234567</v>"), "andere Zahl bleibt: {sheet}");
        assert!(sheet.contains("<v>Lea Roth</v>") && sheet.contains("<t>Roth</t>"), "{sheet}");
        for f in [&inp, &out] {
            let _ = std::fs::remove_file(f);
        }
    }

    #[test]
    fn odf_text_meta_and_mimetype() {
        let inp = tmp("in.odt");
        let out = tmp("out.odt");
        make_zip(
            &inp,
            &[
                ("mimetype", "application/vnd.oasis.opendocument.text"),
                ("content.xml", r#"<office:document-content><office:body><text:p text:style-name="P1">Von <text:span text:style-name="T1">Anna</text:span><text:s/>Müller</text:p><table:table-cell office:value-type="float" office:value="123456"><text:p>123456</text:p></table:table-cell></office:body></office:document-content>"#),
                ("meta.xml", "<office:meta><meta:initial-creator>Anna Müller</meta:initial-creator><meta:user-defined meta:name=\"Schüler\">Anna</meta:user-defined><meta:generator>LibreOffice</meta:generator></office:meta>"),
                ("Thumbnails/thumbnail.png", "PNG"),
                ("META-INF/manifest.xml", r#"<manifest:manifest><manifest:file-entry manifest:full-path="/" manifest:media-type="x"/><manifest:file-entry manifest:full-path="Thumbnails/thumbnail.png" manifest:media-type="image/png"/></manifest:manifest>"#),
            ],
        );
        process_office(&inp, &out, OfficeKind::Odf, &map()).unwrap();
        let content = read_part(&out, "content.xml").unwrap();
        assert!(content.contains(r#"Von <text:span text:style-name="T1">Lea</text:span><text:s/>Roth</text:p>"#) || content.contains("Lea Roth"), "{content}");
        assert!(content.contains(r#"<table:table-cell office:value-type="string"><text:p>PN-0000ABCD</text:p>"#), "Matrikel-Zelle wird Textzelle: {content}");
        assert!(!content.contains("123456"), "{content}");
        let meta = read_part(&out, "meta.xml").unwrap();
        assert!(!meta.contains("Anna") && !meta.contains("Müller") && meta.contains("LibreOffice"), "{meta}");
        assert!(read_part(&out, "Thumbnails/thumbnail.png").is_none());
        assert!(!read_part(&out, "META-INF/manifest.xml").unwrap().contains("Thumbnails"));
        // mimetype bleibt erster, unkomprimierter Eintrag
        let mut a = ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let first = a.by_index(0).unwrap();
        assert_eq!((first.name(), first.compression()), ("mimetype", CompressionMethod::Stored));
        for f in [&inp, &out] {
            let _ = std::fs::remove_file(f);
        }
    }

    #[test]
    fn fuzzy_only_in_content() {
        let inp = tmp("in.pptx");
        make_zip(
            &inp,
            &[
                ("ppt/theme/theme1.xml", r#"<a:theme><a:latin typeface="Müler"/></a:theme>"#),
                ("ppt/slides/slide3.xml", "<p:sld><a:p><a:r><a:t>Hier steht Müler.</a:t></a:r></a:p></p:sld>"),
            ],
        );
        let hits = scan_fuzzy_office(&inp, OfficeKind::Pptx, &map()).unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].location, "Folie 3");
        let _ = std::fs::remove_file(&inp);
    }
}
