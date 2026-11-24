//! DOCX-Anonymisierung und De-Anonymisierung.
//!
//! .docx ist ein ZIP aus XML-Teilen. Strategie:
//! - Text steht in `<w:t>`-Elementen; Namen können über mehrere Runs (w:r)
//!   verteilt sein. Deshalb wird pro Absatz (w:p) der Gesamttext rekonstruiert,
//!   dort gesucht und das Pseudonym in den ersten betroffenen Run geschrieben.
//! - Tracked Changes werden übernommen: w:ins bleibt (und wird anonymisiert),
//!   w:del fliegt komplett raus.
//! - Metadaten (Autor, Firma, eigene Properties) werden neutralisiert.
//! - De-Anonymisierung läuft denselben Code mit umgedrehtem Mapping.

use regex::Regex;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Error)]
pub enum DocxError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Regex: {0}")]
    Regex(#[from] regex::Error),
}

#[derive(Debug, Default, Clone)]
pub struct DocxStats {
    pub replacements: usize,
    pub per_identifier: HashMap<String, usize>,
    pub parts_changed: usize,
}

/// Verarbeitet ein DOCX mit dem Mapping (suchtext → ersatz).
/// Anonymisierung: Mapping real→pseudo. De-Anonymisierung: pseudo→real.
pub fn process_docx(
    input: &Path,
    output: &Path,
    mapping: &[(String, String)],
) -> Result<DocxStats, DocxError> {
    let mut stats = DocxStats::default();
    let matcher = if mapping.is_empty() {
        None
    } else {
        Some(build_matcher_docx(mapping)?)
    };

    let file = std::fs::File::open(input)?;
    let mut archive = ZipArchive::new(file)?;
    let out = std::fs::File::create(output)?;
    let mut writer = ZipWriter::new(out);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if entry.is_dir() {
            continue;
        }
        // Eigene Dokumenteigenschaften können Namen enthalten, das Vorschaubild
        // zeigt die erste Seite → ganz verwerfen.
        if name == "docProps/custom.xml" || name.starts_with("docProps/thumbnail.") {
            continue;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;

        let new_buf = if is_text_part(&name) {
            match &matcher {
                Some(matcher) => {
                    let xml = String::from_utf8(buf)?;
                    let (new_xml, st) = transform_text_part(&xml, matcher);
                    merge_stats(&mut stats, st);
                    // Alt-Texte von Bildern (descr/title); getrennt von w:t, also nie doppelt ersetzt
                    replace_attrs(new_xml, &ALT_TEXT, matcher, &mut stats).into_bytes()
                }
                None => buf,
            }
        } else if name.starts_with("word/_rels/") && name.ends_with(".rels") {
            // Hyperlinks: mailto:anna.mueller@…, Pfade mit Benutzernamen
            match &matcher {
                Some(matcher) => replace_attrs(String::from_utf8(buf)?, &LINK_TARGETS, matcher, &mut stats).into_bytes(),
                None => buf,
            }
        } else if name == "_rels/.rels" {
            crate::office::drop_removed_rels(&String::from_utf8(buf)?).into_bytes()
        } else if name == "docProps/core.xml" || name == "docProps/app.xml" {
            let xml = String::from_utf8(buf)?;
            sanitize_props(&name, &xml).into_bytes()
        } else {
            buf
        };

        writer.start_file(name, options)?;
        writer.write_all(&new_buf)?;
    }
    writer.finish()?;
    Ok(stats)
}

fn merge_stats(stats: &mut DocxStats, st: DocxStats) {
    stats.replacements += st.replacements;
    stats.parts_changed += st.parts_changed;
    for (k, v) in st.per_identifier {
        *stats.per_identifier.entry(k).or_insert(0) += v;
    }
}

fn is_text_part(name: &str) -> bool {
    name == "word/document.xml"
        || name.starts_with("word/header")
        || name.starts_with("word/footer")
        || name == "word/footnotes.xml"
        || name == "word/endnotes.xml"
        || name == "word/comments.xml"
        || name.starts_with("word/glossary/")
}

use crate::matcher::{build_matcher, Matcher};
use crate::office::{xml_groups, XmlCfg};
use crate::textview::{self, Hits};

const ALT_TEXT: XmlCfg = XmlCfg {
    para: &[],
    text: &[],
    text_nodes: false,
    attrs: true,
    only_attrs: &["descr", "title"],
    skip_attrs: &[],
    xlsx_cells: false,
};

const LINK_TARGETS: XmlCfg = XmlCfg { only_attrs: &["Target"], ..ALT_TEXT };

/// Ersetzt nur in den Attributen aus `cfg.only_attrs`.
fn replace_attrs(xml: String, cfg: &XmlCfg, matcher: &Matcher, stats: &mut DocxStats) -> String {
    let mut hits = Hits::default();
    let out = textview::replace_groups(&xml, &xml_groups(&xml, cfg), matcher, &textview::escape_xml_attr, &mut hits);
    if out.is_some() {
        stats.parts_changed += 1;
    }
    stats.replacements += hits.replacements;
    for (k, v) in hits.per_identifier {
        *stats.per_identifier.entry(k).or_insert(0) += v;
    }
    out.unwrap_or(xml)
}

fn build_matcher_docx(mapping: &[(String, String)]) -> Result<std::sync::Arc<Matcher>, DocxError> {
    Ok(build_matcher(mapping)?)
}

/// Ein XML-Textteil (document/header/footer/comments/...): Tracked Changes
/// übernehmen, Autoren-Attribute neutralisieren, dann Text ersetzen.
fn transform_text_part(
    xml: &str,
    matcher: &Matcher,
) -> (String, DocxStats) {
    let mut stats = DocxStats::default();
    let mut out = accept_revisions(xml);
    out = sanitize_author_attrs(&out);
    out = replace_in_paragraphs(&out, matcher, &mut stats);
    out = replace_in_instr_text(&out, matcher, &mut stats);
    if out != xml {
        stats.parts_changed = 1;
    }
    (out, stats)
}

/// Nimmt alle Änderungsverfolgungen an: gelöschte Inhalte (w:del, w:moveFrom)
/// entfernen, eingefügte (w:ins, w:moveTo) und Änderungs-Metadaten auspacken.
fn accept_revisions(xml: &str) -> String {
    let mut out = xml.to_string();
    let remove_blocks = [
        "w:del", "w:moveFrom", "w:rPrChange", "w:pPrChange", "w:sectPrChange",
        "w:tblPrChange", "w:tcPrChange", "w:trPrChange", "w:cellPrChange",
    ];
    for tag in remove_blocks {
        let re = Regex::new(&format!("(?s)<{tag}\\b[^>]*>.*?</{tag}>")).unwrap();
        out = re.replace_all(&out, "").into_owned();
    }
    for tag in ["w:ins", "w:moveTo"] {
        let open = Regex::new(&format!("<{tag}\\b[^>]*>")).unwrap();
        out = open.replace_all(&out, "").into_owned();
        let close = format!("</{tag}>");
        out = out.replace(&close, "");
    }
    out
}

/// Autoren-Attribute in Kommentaren etc. neutralisieren.
fn sanitize_author_attrs(xml: &str) -> String {
    let re = Regex::new("(?s)\\bw:author=\"[^\"]*\"").unwrap();
    re.replace_all(xml, "w:author=\"anonymisiert\"").into_owned()
}

/// Dokument-Metadaten (core.xml / app.xml) von personenbezogenen Inhalten befreien.
fn sanitize_props(name: &str, xml: &str) -> String {
    let tags: &[&str] = if name == "docProps/core.xml" {
        &[
            "dc:creator",
            "cp:lastModifiedBy",
            "dc:title",
            "dc:subject",
            "dc:description",
            "cp:keywords",
        ]
    } else {
        &["Company", "Manager"]
    };
    let mut out = xml.to_string();
    for tag in tags {
        let re = Regex::new(&format!("(?s)<{tag}\\b[^>]*>.*?</{tag}>")).unwrap();
        out = re.replace_all(&out, format!("<{tag}></{tag}>").as_str()).into_owned();
    }
    out
}

/// Sucht in jedem Absatz (w:p) über alle Runs hinweg und ersetzt.
fn replace_in_paragraphs(
    xml: &str,
    matcher: &Matcher,
    stats: &mut DocxStats,
) -> String {
    let paragraphs = scan_elements(xml, "w:p");
    let mut out = xml.to_string();
    // von hinten nach vorne, damit Byte-Offsets gültig bleiben
    for (elem_start, _cs, _ce, elem_end) in paragraphs.into_iter().rev() {
        let para = &out[elem_start..elem_end];
        let (new_para, changed) = replace_in_paragraph(para, matcher, stats);
        if changed {
            out.replace_range(elem_start..elem_end, &new_para);
        }
    }
    out
}

fn replace_in_paragraph(
    para: &str,
    matcher: &Matcher,
    stats: &mut DocxStats,
) -> (String, bool) {
    let elems = scan_elements(para, "w:t");
    if elems.is_empty() {
        return (para.to_string(), false);
    }
    // Gesamttext des Absatzes (unescaped) + Byte-Bereiche der einzelnen w:t-Elemente
    let mut full_u = String::new();
    let mut ranges_u: Vec<(usize, usize)> = Vec::new();
    let mut contents: Vec<String> = Vec::new();
    for (_es, cs, ce, _ee) in &elems {
        let c = unescape(&para[*cs..*ce]);
        ranges_u.push((full_u.len(), full_u.len() + c.len()));
        full_u.push_str(&c);
        contents.push(c);
    }

    let matches: Vec<(usize, usize, String, String)> =
        matcher.find_all(&full_u).into_iter().map(|f| (f.start, f.end, f.key, f.repl)).collect();
    if matches.is_empty() {
        return (para.to_string(), false);
    }

    // Neue Inhalte pro w:t-Element berechnen. Matches von hinten nach vorne,
    // damit die Byte-Offsets der linken Matches gültig bleiben. Die Ersetzung
    // landet nur im ersten betroffenen Element, der Rest des Namens wird entfernt.
    let mut new_contents: Vec<Option<String>> = vec![None; contents.len()];
    for (ms, me, key, repl) in matches.iter().rev() {
        stats.replacements += 1;
        *stats.per_identifier.entry(key.clone()).or_insert(0) += 1;
        let first_idx = ranges_u
            .iter()
            .position(|(es, ee)| !(*me <= *es || *ms >= *ee))
            .expect("Match überlappt mindestens ein Element");
        for (i, c) in contents.iter().enumerate() {
            let (es, ee) = ranges_u[i];
            if *me <= es || *ms >= ee {
                continue; // keine Überlappung
            }
            let local_start = ms.saturating_sub(es);
            let local_end = (*me).min(ee) - es;
            let cur = new_contents[i].clone().unwrap_or_else(|| c.clone());
            let mut next = String::new();
            next.push_str(&cur[..local_start]);
            if i == first_idx {
                next.push_str(repl);
            }
            next.push_str(&cur[local_end..]);
            new_contents[i] = Some(next);
        }
    }

    // Neue Inhalte in den Absatz splicen (von hinten nach vorne)
    let mut out = para.to_string();
    for (i, elem) in elems.iter().enumerate().rev() {
        if let Some(nc) = &new_contents[i] {
            let (_, cs, ce, _) = *elem;
            out.replace_range(cs..ce, &escape_xml(nc));
        }
    }
    (out, true)
}

/// Feldcodes (w:instrText) einzeln durchsuchen — dort können z.B. Dateipfade
/// mit Benutzernamen stehen.
fn replace_in_instr_text(
    xml: &str,
    matcher: &Matcher,
    stats: &mut DocxStats,
) -> String {
    let elems = scan_elements(xml, "w:instrText");
    let mut out = xml.to_string();
    for (_es, cs, ce, _ee) in elems.into_iter().rev() {
        let raw = &out[cs..ce];
        let un = unescape(raw);
        let new_un = matcher.replace_all(&un, |f| {
            stats.replacements += 1;
            *stats.per_identifier.entry(f.key.clone()).or_insert(0) += 1;
        });
        if let Some(new_un) = new_un {
            out.replace_range(cs..ce, &escape_xml(&new_un));
        }
    }
    out
}

/// Findet alle `<tag ...>inhalt</tag>`-Elemente (nicht selbstschließend).
/// Rückgabe: (element_start, content_start, content_end, element_end) als Byte-Offsets.
pub(crate) fn scan_elements(xml: &str, tag: &str) -> Vec<(usize, usize, usize, usize)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(rel) = xml[pos..].find(&open) {
        let start = pos + rel;
        let after = start + open.len();
        let rest = &xml[after..];
        let c = rest.chars().next();
        let handled = match c {
            // selbstschließend: <w:t/>
            Some('/') if rest.starts_with("/>") => {
                pos = after + 2;
                true
            }
            // <w:t> ohne Attribute
            Some('>') => {
                let cs = after + 1;
                if let Some(e) = xml[cs..].find(&close) {
                    let ce = cs + e;
                    out.push((start, cs, ce, ce + close.len()));
                    pos = ce + close.len();
                } else {
                    pos = after + 1;
                }
                true
            }
            // <w:t xml:space="preserve"> mit Attributen
            Some(ch) if ch.is_whitespace() => {
                if let Some(gt_rel) = rest.find('>') {
                    let gt = after + gt_rel;
                    if xml[..gt].ends_with('/') {
                        pos = gt + 1;
                    } else {
                        let cs = gt + 1;
                        if let Some(e) = xml[cs..].find(&close) {
                            let ce = cs + e;
                            out.push((start, cs, ce, ce + close.len()));
                            pos = ce + close.len();
                        } else {
                            pos = gt + 1;
                        }
                    }
                } else {
                    pos = after + 1;
                }
                true
            }
            _ => false,
        };
        if !handled {
            pos = start + open.len();
        }
    }
    out
}

pub(crate) fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        if let Some(semi) = after.find(';') {
            let entity = &after[..semi];
            let decoded = match entity {
                "amp" => Some("&".to_string()),
                "lt" => Some("<".to_string()),
                "gt" => Some(">".to_string()),
                "quot" => Some("\"".to_string()),
                "apos" => Some("'".to_string()),
                num if num.starts_with("#x") || num.starts_with("#X") => u32::from_str_radix(&num[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string()),
                num if num.starts_with('#') => num[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(|c| c.to_string()),
                _ => None,
            };
            if let Some(d) = decoded {
                out.push_str(&d);
                rest = &after[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Scannt ein DOCX nach unscharfen Namen-Treffern (Tippfehler), die das exakte
/// Mapping verpasst. Pro Absatz wird der Text rekonstruiert und per
/// Fuzzy-Matching durchsucht. Rückgabe: Review-Liste mit Fundstellen.
pub fn scan_fuzzy_docx(
    input: &Path,
    mapping: &[(String, String)],
) -> Result<Vec<crate::fuzzy::FuzzyHit>, DocxError> {
    use crate::fuzzy::{find_fuzzy, DEFAULT_THRESHOLD};
    let file = std::fs::File::open(input)?;
    let mut archive = ZipArchive::new(file)?;
    let mut hits = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !is_text_part(&name) {
            continue;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        let xml = String::from_utf8(buf)?;
        for (pi, (es, _cs, _ce, ee)) in scan_elements(&xml, "w:p").iter().enumerate() {
            let para = &xml[*es..*ee];
            let mut text = String::new();
            for (_a, cs, ce, _b) in scan_elements(para, "w:t") {
                text.push_str(&unescape(&para[cs..ce]));
            }
            for mut h in find_fuzzy(&text, mapping, DEFAULT_THRESHOLD) {
                h.location = format!("{name}, Absatz {}", pi + 1);
                hits.push(h);
            }
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Baut ein minimales DOCX (ZIP) mit gegebenem document.xml + core.xml.
    fn make_docx(path: &Path, document_xml: &str, core_xml: &str) {
        let f = std::fs::File::create(path).unwrap();
        let mut w = ZipWriter::new(f);
        let opt = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        w.start_file("[Content_Types].xml", opt).unwrap();
        w.write_all(b"<?xml version=\"1.0\"?><Types/>").unwrap();
        w.start_file("word/document.xml", opt).unwrap();
        w.write_all(document_xml.as_bytes()).unwrap();
        w.start_file("docProps/core.xml", opt).unwrap();
        w.write_all(core_xml.as_bytes()).unwrap();
        w.finish().unwrap();
    }

    fn read_docx_part(path: &Path, part: &str) -> String {
        let f = std::fs::File::open(path).unwrap();
        let mut a = ZipArchive::new(f).unwrap();
        let mut e = a.by_name(part).unwrap();
        let mut s = String::new();
        e.read_to_string(&mut s).unwrap();
        s
    }

    const CORE_XML: &str = r#"<?xml version="1.0"?>
<cp:coreProperties xmlns:cp="x" xmlns:dc="y">
  <dc:creator>Anna Müller</dc:creator>
  <cp:lastModifiedBy>Anna Müller</cp:lastModifiedBy>
  <dcterms:created>2026-01-01</dcterms:created>
</cp:coreProperties>"#;

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("nema-docx-test-{}-{name}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn replaces_name_split_across_runs() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body>
<w:p><w:r><w:t>Das ist </w:t></w:r><w:r><w:t>Anna </w:t></w:r><w:r><w:t>Müller</w:t></w:r><w:r><w:t> und sie rechnet.</w:t></w:r></w:p>
</w:body></w:document>"#;
        let inp = tmp("in.docx");
        let outp = tmp("out.docx");
        make_docx(&inp, doc, CORE_XML);
        let mapping = vec![("Anna Müller".to_string(), "Leo Roth".to_string())];
        let stats = process_docx(&inp, &outp, &mapping).unwrap();
        assert_eq!(stats.replacements, 1);
        let result = read_docx_part(&outp, "word/document.xml");
        assert!(result.contains("Leo Roth"), "Ergebnis: {result}");
        // Pseudonym genau einmal (nicht pro Run wiederholt)
        assert_eq!(result.matches("Leo Roth").count(), 1, "Ergebnis: {result}");
        assert!(!result.contains("Müller"), "Ergebnis: {result}");
        // Struktur erhalten: noch 4 Runs
        assert_eq!(result.matches("<w:r>").count(), 4);
        // Metadaten bereinigt, Datum bleibt
        let core = read_docx_part(&outp, "docProps/core.xml");
        assert!(!core.contains("Anna Müller"));
        assert!(core.contains("2026-01-01"));
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&outp);
    }

    #[test]
    fn case_insensitive_and_roundtrip() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body>
<w:p><w:r><w:t>ANNA MÜLLER hat abgegeben.</w:t></w:r></w:p>
</w:body></w:document>"#;
        let inp = tmp("in.docx");
        let mid = tmp("mid.docx");
        let outp = tmp("out.docx");
        make_docx(&inp, doc, CORE_XML);
        let fwd = vec![("Anna Müller".to_string(), "Leo Roth".to_string())];
        process_docx(&inp, &mid, &fwd).unwrap();
        let anon = read_docx_part(&mid, "word/document.xml");
        assert!(anon.contains("Leo Roth"), "anon: {anon}");
        let rev = vec![("Leo Roth".to_string(), "Anna Müller".to_string())];
        let stats = process_docx(&mid, &outp, &rev).unwrap();
        assert_eq!(stats.replacements, 1);
        let back = read_docx_part(&outp, "word/document.xml");
        assert!(back.contains("ANNA MÜLLER") || back.contains("Anna Müller"), "back: {back}");
        for f in [&inp, &mid, &outp] { let _ = std::fs::remove_file(f); }
    }

    #[test]
    fn tracked_changes_accepted_and_anonymized() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body>
<w:p>
  <w:ins w:id="1" w:author="Anna Müller"><w:r><w:t>Anna Müller</w:t></w:r></w:ins>
  <w:del w:id="2" w:author="Anna Müller"><w:r><w:delText>Anna Müller</w:delText></w:r></w:del>
</w:p>
</w:body></w:document>"#;
        let inp = tmp("in.docx");
        let outp = tmp("out.docx");
        make_docx(&inp, doc, CORE_XML);
        let mapping = vec![("Anna Müller".to_string(), "Leo Roth".to_string())];
        process_docx(&inp, &outp, &mapping).unwrap();
        let result = read_docx_part(&outp, "word/document.xml");
        assert!(result.contains("Leo Roth"), "result: {result}");
        assert!(!result.contains("w:del"), "del entfernt: {result}");
        assert!(!result.contains("w:ins "), "ins ausgepackt: {result}");
        assert!(!result.contains("Anna Müller"));
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&outp);
    }

    #[test]
    fn header_footer_and_comment_author() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>Text</w:t></w:r></w:p></w:body></w:document>"#;
        let header = r#"<?xml version="1.0"?>
<w:hdr xmlns:w="x"><w:p><w:r><w:t>Prüfung von Anna Müller</w:t></w:r></w:p></w:hdr>"#;
        let comments = r#"<?xml version="1.0"?>
<w:comments xmlns:w="x"><w:comment w:id="0" w:author="Anna Müller"><w:p><w:r><w:t>gut</w:t></w:r></w:p></w:comment></w:comments>"#;
        let inp = tmp("in.docx");
        let outp = tmp("out.docx");
        let f = std::fs::File::create(&inp).unwrap();
        let mut w = ZipWriter::new(f);
        let opt = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (n, c) in [("word/document.xml", doc), ("word/header1.xml", header), ("word/comments.xml", comments)] {
            w.start_file(n, opt).unwrap();
            w.write_all(c.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        let mapping = vec![("Anna Müller".to_string(), "Leo Roth".to_string())];
        process_docx(&inp, &outp, &mapping).unwrap();
        let h = read_docx_part(&outp, "word/header1.xml");
        assert!(h.contains("Leo Roth") && !h.contains("Anna Müller"), "header: {h}");
        let c = read_docx_part(&outp, "word/comments.xml");
        assert!(c.contains("w:author=\"anonymisiert\""), "comments: {c}");
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&outp);
    }

    #[test]
    fn alt_text_and_hyperlinks() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body><w:p><w:r><w:drawing><wp:docPr id="1" name="Bild 1" descr="Foto von Anna Müller"/></w:drawing></w:r><w:r><w:t>Anna Müller</w:t></w:r></w:p></w:body></w:document>"#;
        let rels = r#"<Relationships><Relationship Id="rId5" Type="hyperlink" Target="mailto:anna.mueller@schule.ch" TargetMode="External"/></Relationships>"#;
        let inp = tmp("in.docx");
        let outp = tmp("out.docx");
        let f = std::fs::File::create(&inp).unwrap();
        let mut w = ZipWriter::new(f);
        let opt = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (n, c) in [("word/document.xml", doc), ("word/_rels/document.xml.rels", rels), ("docProps/thumbnail.jpeg", "JPEG")] {
            w.start_file(n, opt).unwrap();
            w.write_all(c.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        let mapping = vec![
            ("Anna Müller".to_string(), "Leo Roth".to_string()),
            ("anna.mueller@schule.ch".to_string(), "leo.roth@anonym.invalid".to_string()),
        ];
        let stats = process_docx(&inp, &outp, &mapping).unwrap();
        assert_eq!(stats.replacements, 3);
        let d = read_docx_part(&outp, "word/document.xml");
        assert!(d.contains(r#"descr="Foto von Leo Roth""#) && d.contains("<w:t>Leo Roth</w:t>") && !d.contains("Müller"), "{d}");
        let r = read_docx_part(&outp, "word/_rels/document.xml.rels");
        assert!(r.contains("mailto:leo.roth@anonym.invalid"), "{r}");
        let mut a = ZipArchive::new(std::fs::File::open(&outp).unwrap()).unwrap();
        assert!(a.by_name("docProps/thumbnail.jpeg").is_err());
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&outp);
    }

    #[test]
    fn scan_fuzzy_finds_typo() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body>
<w:p><w:r><w:t>Hier hat Müler unterschrieben.</w:t></w:r></w:p>
<w:p><w:r><w:t>Anna Müller war auch da.</w:t></w:r></w:p>
</w:body></w:document>"#;
        let inp = tmp("in.docx");
        make_docx(&inp, doc, CORE_XML);
        let mapping = vec![("Anna Müller".to_string(), "Leo Roth".to_string()), ("Müller".to_string(), "Roth".to_string())];
        let hits = scan_fuzzy_docx(&inp, &mapping).unwrap();
        assert!(hits.iter().any(|h| h.matched == "Müler" && h.location.contains("Absatz 1")), "hits: {hits:?}");
        assert!(!hits.iter().any(|h| h.matched == "Anna Müller"), "exakter Treffer darf nicht fuzzy sein: {hits:?}");
        let _ = std::fs::remove_file(&inp);
    }

    #[test]
    fn empty_mapping_is_noop() {
        let doc = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>Anna Müller</w:t></w:r></w:p></w:body></w:document>"#;
        let inp = tmp("in.docx");
        let outp = tmp("out.docx");
        make_docx(&inp, doc, CORE_XML);
        let stats = process_docx(&inp, &outp, &[]).unwrap();
        assert_eq!(stats.replacements, 0);
        let result = read_docx_part(&outp, "word/document.xml");
        assert!(result.contains("Anna Müller"));
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&outp);
    }
}
