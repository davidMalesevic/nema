//! PDF-Anonymisierung und De-Anonymisierung via PDFium (Google).
//!
//! Strategie pro Seite: jedes Text-Objekt wird einzeln gelesen, das Mapping
//! (case-insensitiv, längste Variante zuerst) per Regex auf seinen Text
//! angewendet und per `set_text` zurückgeschrieben. Dadurch landet der echte
//! Name nicht mehr im Content-Stream (kein bloßes Überdecken — Copy-Paste
//! liefert nichts mehr). Namen, die über mehrere Text-Objekte gespannt sind,
//! werden nach dem Lauf erkannt und als `residual_matches` gemeldet (Review).
//!
//! Metadaten (Info-Dict, XMP) werden nach dem Speichern per lopdf entfernt.
//!
//! Die PDFium-Bibliothek wird gesucht: Env `NEMA_PDFIUM_LIB`, dann neben der
//! exe (`libpdfium.so` / `pdfium.dll` / `libpdfium.dylib`), dann System.

use crate::matcher::{build_matcher, Lookup};
use pdfium_render::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("PDFium: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Regex: {0}")]
    Regex(#[from] regex::Error),
    #[error("lopdf: {0}")]
    Lopdf(#[from] lopdf::Error),
    #[error("PDFium-Bibliothek nicht gefunden: {0} — bitte libpdfium neben die App legen oder NEMA_PDFIUM_LIB setzen")]
    LibraryNotFound(String),
}

#[derive(Debug, Default, Clone)]
pub struct PdfStats {
    pub replacements: usize,
    pub per_identifier: HashMap<String, usize>,
    pub pages_scanned: usize,
    /// Treffer, die nach der Ersetzung noch im Seitentext stehen (z.B. Name
    /// über mehrere Text-Objekte verteilt) → manuelle Review nötig.
    pub residual_matches: Vec<String>,
}

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

/// Liefert die prozessweit einmal gebundene PDFium-Instanz.
pub fn pdfium_instance() -> Result<&'static Pdfium, PdfError> {
    PDFIUM
        .get_or_init(|| load_pdfium().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| PdfError::LibraryNotFound(e.clone()))
}

fn load_pdfium() -> Result<Pdfium, PdfError> {
    // 1. explizit per Env-Variable
    if let Ok(p) = std::env::var("NEMA_PDFIUM_LIB") {
        if !p.is_empty() {
            return Ok(Pdfium::new(Pdfium::bind_to_library(&p)?));
        }
    }
    // 2. neben der ausführbaren Datei bzw. in lib/ daneben (Deployment: libpdfium mit ausliefern)
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let candidates = [
            Pdfium::pdfium_platform_library_name_at_path(&dir),
            Pdfium::pdfium_platform_library_name_at_path(&dir.join("lib")),
            Pdfium::pdfium_platform_library_name_at_path(&dir.join("../lib")),
        ];
        for candidate in candidates {
            if candidate.exists() {
                return Ok(Pdfium::new(Pdfium::bind_to_library(&candidate)?));
            }
        }
    }
    // 3. Systempfad
    Ok(Pdfium::new(Pdfium::bind_to_system_library()?))
}

/// Verarbeitet ein PDF mit dem Mapping (suchtext → ersatz).
/// Anonymisierung: real→pseudo. De-Anonymisierung: pseudo→real.
pub fn process_pdf(
    input: &Path,
    output: &Path,
    mapping: &[(String, String)],
) -> Result<PdfStats, PdfError> {
    let mut stats = PdfStats::default();
    let pdfium = pdfium_instance()?;
    let doc = pdfium.load_pdf_from_file(input, None)?;

    if !mapping.is_empty() {
        let (re, lookup) = build_matcher(mapping)?;
        let page_count = doc.pages().len();
        for i in 0..page_count {
            let mut page = doc.pages().get(i)?;
            stats.pages_scanned += 1;
            let changed = replace_in_text_objects(&mut page, &re, &lookup, &mut stats)?;
            if changed {
                // geladene Seiten regenerieren ihren Content-Stream nur manuell
                page.regenerate_content()?;
            }
            collect_residuals(&page, &re, &mut stats);
        }
    }

    doc.save_to_file(output)?;
    strip_metadata(output)?;
    Ok(stats)
}

fn replace_in_text_objects(
    page: &mut PdfPage<'_>,
    re: &Regex,
    lookup: &Lookup,
    stats: &mut PdfStats,
) -> Result<bool, PdfError> {
    let mut page_changed = false;
    let count = page.objects().len();
    for oi in 0..count {
        let mut obj = page.objects_mut().get(oi)?;
        let Some(txt) = obj.as_text_object_mut() else { continue };
        let old = txt.text();
        if old.is_empty() {
            continue;
        }
        let mut changed = false;
        let new = re
            .replace_all(&old, |caps: &regex::Captures| {
                changed = true;
                let (key, repl) = lookup
                    .get(&caps[0].to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| (caps[0].to_string(), caps[0].to_string()));
                stats.replacements += 1;
                *stats.per_identifier.entry(key).or_insert(0) += 1;
                repl
            })
            .into_owned();
        if changed {
            txt.set_text(new)?;
            page_changed = true;
        }
    }
    Ok(page_changed)
}

fn collect_residuals(page: &PdfPage<'_>, re: &Regex, stats: &mut PdfStats) {
    if let Ok(text) = page.text() {
        for m in re.find_iter(&text.all()) {
            stats.residual_matches.push(m.as_str().to_string());
        }
    }
}

/// Scannt ein PDF nach unscharfen Namen-Treffern (Tippfehler) pro Seite.
pub fn scan_fuzzy_pdf(
    input: &Path,
    mapping: &[(String, String)],
) -> Result<Vec<crate::fuzzy::FuzzyHit>, PdfError> {
    use crate::fuzzy::{find_fuzzy, DEFAULT_THRESHOLD};
    let pdfium = pdfium_instance()?;
    let doc = pdfium.load_pdf_from_file(input, None)?;
    let mut hits = Vec::new();
    for i in 0..doc.pages().len() {
        let page = doc.pages().get(i)?;
        let text = match page.text() {
            Ok(t) => t.all(),
            Err(_) => continue,
        };
        for mut h in find_fuzzy(&text, mapping, DEFAULT_THRESHOLD) {
            h.location = format!("Seite {}", i + 1);
            hits.push(h);
        }
    }
    Ok(hits)
}

/// Entfernt personenbezogene Metadaten (Info-Dict, XMP-Metadaten).
fn strip_metadata(path: &Path) -> Result<(), PdfError> {
    let mut doc = lopdf::Document::load(path)?;
    doc.trailer.remove(b"Info");
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
    doc.save(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_pdfium() -> Option<&'static Pdfium> {
        pdfium_instance().ok()
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nema-pdf-test-{}-{name}", uuid::Uuid::new_v4()))
    }

    /// Erzeugt ein PDF mit zwei Text-Objekten (eines enthält den Namen).
    fn make_pdf(path: &Path, pdfium: &Pdfium, line1: &str, line2: &str) {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
        let font = doc.fonts_mut().helvetica();
        page.objects_mut()
            .create_text_object(PdfPoints::new(50.0), PdfPoints::new(700.0), line1, font, PdfPoints::new(12.0))
            .unwrap();
        let font = doc.fonts_mut().helvetica();
        page.objects_mut()
            .create_text_object(PdfPoints::new(50.0), PdfPoints::new(680.0), line2, font, PdfPoints::new(12.0))
            .unwrap();
        doc.save_to_file(path).unwrap();
    }

    fn page_text(path: &Path, pdfium: &Pdfium) -> String {
        let doc = pdfium.load_pdf_from_file(path, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        let text = page.text().unwrap().all();
        text
    }

    #[test]
    fn anonymize_and_deanonymize_roundtrip() {
        let Some(pdfium) = try_pdfium() else {
            eprintln!("SKIP: kein libpdfium (NEMA_PDFIUM_LIB nicht gesetzt)");
            return;
        };
        let inp = tmp("in.pdf");
        let anon = tmp("anon.pdf");
        let back = tmp("back.pdf");
        make_pdf(&inp, pdfium, "Prüfung von Anna Müller", "Klasse 10a, Note: sehr gut");

        let fwd = vec![("Anna Müller".to_string(), "Leo Roth".to_string())];
        let stats = process_pdf(&inp, &anon, &fwd).unwrap();
        assert_eq!(stats.replacements, 1, "stats: {stats:?}");
        assert!(stats.residual_matches.is_empty(), "residuals: {:?}", stats.residual_matches);
        let t = page_text(&anon, pdfium);
        assert!(t.contains("Leo Roth"), "anon text: {t}");
        assert!(!t.contains("Müller"), "anon text: {t}");

        let rev = vec![("Leo Roth".to_string(), "Anna Müller".to_string())];
        let stats2 = process_pdf(&anon, &back, &rev).unwrap();
        assert_eq!(stats2.replacements, 1);
        let t2 = page_text(&back, pdfium);
        assert!(t2.contains("Anna Müller"), "back text: {t2}");

        for f in [&inp, &anon, &back] { let _ = std::fs::remove_file(f); }
    }

    #[test]
    fn metadata_stripped() {
        let Some(pdfium) = try_pdfium() else { return };
        let inp = tmp("in.pdf");
        let out = tmp("out.pdf");
        make_pdf(&inp, pdfium, "Anna Müller", "Text");
        // Info-Metadaten einschreiben
        {
            let mut doc = lopdf::Document::load(&inp).unwrap();
            let info_id = doc.add_object(lopdf::Object::Dictionary(lopdf::Dictionary::from_iter(vec![
                (b"Author".to_vec(), lopdf::Object::string_literal("Anna Müller")),
            ])));
            doc.trailer.set("Info", lopdf::Object::Reference(info_id));
            doc.save(&inp).unwrap();
        }
        let fwd = vec![("Anna Müller".to_string(), "Leo Roth".to_string())];
        process_pdf(&inp, &out, &fwd).unwrap();
        let doc = lopdf::Document::load(&out).unwrap();
        assert!(!doc.trailer.has(b"Info"), "Info-Dict muss entfernt sein");
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn scan_fuzzy_finds_typo() {
        let Some(pdfium) = try_pdfium() else { return };
        let inp = tmp("in.pdf");
        make_pdf(&inp, pdfium, "Unterschrift: Müler", "Anna Müller, 10a");
        let mapping = vec![
            ("Anna Müller".to_string(), "Leo Roth".to_string()),
            ("Müller".to_string(), "Roth".to_string()),
        ];
        let hits = scan_fuzzy_pdf(&inp, &mapping).unwrap();
        assert!(hits.iter().any(|h| h.matched == "Müler" && h.location == "Seite 1"), "hits: {hits:?}");
        let _ = std::fs::remove_file(&inp);
    }

    #[test]
    fn empty_mapping_copies() {
        let Some(pdfium) = try_pdfium() else { return };
        let inp = tmp("in.pdf");
        let out = tmp("out.pdf");
        make_pdf(&inp, pdfium, "Anna Müller", "Text");
        let stats = process_pdf(&inp, &out, &[]).unwrap();
        assert_eq!(stats.replacements, 0);
        let t = page_text(&out, pdfium);
        assert!(t.contains("Anna Müller"));
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&out);
    }
}
