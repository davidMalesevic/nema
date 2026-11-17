//! PDF-Anonymisierung und De-Anonymisierung via PDFium (Google).
//!
//! Strategie pro Seite: Der Seitentext wird Zeichen für Zeichen samt Position
//! gelesen und mit dem Mapping (ohne Gross-/Kleinschreibung, längste Variante
//! zuerst, ganze Wörter) durchsucht.
//!
//! - Seite ohne Treffer: bleibt unverändert.
//! - Seite mit Treffer: wird neu aufgebaut. Die Seite wird als Bild gerendert,
//!   die Fundstellen werden **im Bild** weiss übermalt, alle ursprünglichen
//!   Seitenobjekte entfernt. Darauf kommen das Bild, die Ersetzungen als
//!   sichtbarer Text und der restliche Seitentext als unsichtbare Textebene
//!   (durchsuchbar, kopierbar, für die KI lesbar).
//!
//! Text-Objekte direkt zu ändern (`set_text`) und den Content-Stream neu zu
//! erzeugen, zerstört bei eingebetteten Teil-Schriften (z.B. PDFs aus Word)
//! den ganzen Seitentext — darum der Neuaufbau.
//!
//! Nach dem Speichern entfernt lopdf Metadaten (Info-Dict, XMP) und nicht mehr
//! referenzierte Objekte (u.a. die alten Content-Streams mit dem Klarnamen).
//!
//! Die PDFium-Bibliothek wird gesucht: Env `NEMA_PDFIUM_LIB`, dann neben der
//! exe (`libpdfium.so` / `pdfium.dll` / `libpdfium.dylib`), dann System.

use crate::matcher::build_matcher;
use pdfium_render::prelude::*;
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
    #[error("Bild: {0}")]
    Image(String),
    #[error("PDFium-Bibliothek nicht gefunden: {0} — bitte libpdfium neben die App legen oder NEMA_PDFIUM_LIB setzen")]
    LibraryNotFound(String),
}

#[derive(Debug, Default, Clone)]
pub struct PdfStats {
    pub replacements: usize,
    pub per_identifier: HashMap<String, usize>,
    pub pages_scanned: usize,
    /// Treffer, die nach der Ersetzung noch im Seitentext der Ausgabe stehen
    /// (Kontrolle; sollte leer sein) → manuelle Review nötig.
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

/// Auflösung, mit der Seiten mit Treffern neu gerendert werden.
const RENDER_DPI: f32 = 200.0;
const JPEG_QUALITY: u8 = 88;

/// Verarbeitet ein PDF mit dem Mapping (suchtext → ersatz).
/// Anonymisierung: real→pseudo. De-Anonymisierung: pseudo→real.
pub fn process_pdf(
    input: &Path,
    output: &Path,
    mapping: &[(String, String)],
) -> Result<PdfStats, PdfError> {
    let mut stats = PdfStats::default();
    let pdfium = pdfium_instance()?;
    let mut doc = pdfium.load_pdf_from_file(input, None)?;

    let matcher = if mapping.is_empty() { None } else { Some(build_matcher(mapping)?) };
    if let Some(matcher) = &matcher {
        let font = doc.fonts_mut().helvetica();
        let page_count = doc.pages().len();
        for i in 0..page_count {
            let mut page = doc.pages().get(i)?;
            stats.pages_scanned += 1;
            let chars = page_chars(&page)?;
            let text: String = chars.iter().map(|c| c.ch).collect();
            let hits = matcher.find_all(&text);
            if hits.is_empty() {
                continue;
            }
            for f in &hits {
                stats.replacements += 1;
                *stats.per_identifier.entry(f.key.clone()).or_insert(0) += 1;
            }
            rebuild_page(&doc, &mut page, &chars, &text, &hits, font)?;
        }
    }

    doc.save_to_file(output)?;
    drop(doc);
    clean_saved(output)?;

    // Kontrolle am Ergebnis: steht noch irgendwo ein Suchtext im Seitentext?
    if let Some(matcher) = &matcher {
        let doc = pdfium.load_pdf_from_file(output, None)?;
        for page in doc.pages().iter() {
            let chars = page_chars(&page)?;
            let text: String = chars.iter().map(|c| c.ch).collect();
            for f in matcher.find_all(&text) {
                stats.residual_matches.push(text[f.start..f.end].to_string());
            }
        }
    }
    Ok(stats)
}

/// Ein Zeichen des Seitentexts. `rect`/`origin` fehlen bei von PDFium
/// erzeugten Zeichen (Leerzeichen, Zeilenumbrüche).
struct PageChar {
    ch: char,
    rect: Option<PdfRect>,
    origin: Option<(f32, f32)>,
    size: f32,
}

fn page_chars(page: &PdfPage<'_>) -> Result<Vec<PageChar>, PdfError> {
    let text = page.text()?;
    let mut out = Vec::new();
    for c in text.chars().iter() {
        let ch = c.unicode_char().unwrap_or('\u{FFFD}');
        let generated = c.is_generated().unwrap_or(false);
        let (rect, origin) = if generated {
            (None, None)
        } else {
            (c.tight_bounds().ok(), c.origin().ok().map(|(x, y)| (x.value, y.value)))
        };
        out.push(PageChar { ch, rect, origin, size: c.scaled_font_size().value });
    }
    Ok(out)
}

/// Ein Stück der neuen Textebene.
struct Segment {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    /// rechter Rand des Originaltexts (für die Breitenanpassung)
    right: f32,
    visible: bool,
}

/// Baut die Seite als Bild + Textebene neu auf (siehe Modul-Doku).
fn rebuild_page<'a>(
    doc: &PdfDocument<'a>,
    page: &mut PdfPage<'a>,
    chars: &[PageChar],
    text: &str,
    hits: &[crate::matcher::Found],
    font: PdfFontToken,
) -> Result<(), PdfError> {
    // Byte-Offset im Text → Zeichenindex
    let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let idx = |byte: usize| starts.partition_point(|&b| b < byte);
    let spans: Vec<(usize, usize, &str)> = hits.iter().map(|f| (idx(f.start), idx(f.end), f.repl.as_str())).collect();

    // Seitendrehung für den Neuaufbau aufheben: Zeichenpositionen, Bild und neue
    // Objekte liegen dann im selben (ungedrehten) Koordinatensystem.
    let rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
    page.set_rotation(PdfPageRenderRotation::None);

    let bbox = page.boundaries().bounding()?.bounds;
    let config = PdfRenderConfig::new()
        .scale_page_by_factor(RENDER_DPI / 72.0)
        .set_clear_color(PdfColor::WHITE)
        .render_annotations(false)
        .render_form_data(false);
    let mut img = page.render_with_config(&config)?.as_image()?.to_rgb8();
    let sx = img.width() as f32 / bbox.width().value;
    let sy = img.height() as f32 / bbox.height().value;

    // Fundstellen im Bild selbst übermalen (nicht nur überdecken)
    for &(a, b, _) in &spans {
        for r in line_boxes(&chars[a..b]) {
            let pad = 1.0;
            let x0 = (((r.0 - pad) - bbox.left().value) * sx).floor().max(0.0) as u32;
            let x1 = (((r.2 + pad) - bbox.left().value) * sx).ceil().min(img.width() as f32) as u32;
            let y0 = ((bbox.top().value - (r.3 + pad)) * sy).floor().max(0.0) as u32;
            let y1 = ((bbox.top().value - (r.1 - pad)) * sy).ceil().min(img.height() as f32) as u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    img.put_pixel(x, y, image::Rgb([255, 255, 255]));
                }
            }
        }
    }

    // Alle ursprünglichen Objekte entfernen (samt Text mit den Klarnamen)
    while page.objects().len() > 0 {
        page.objects_mut().remove_object_at_index(0)?;
    }

    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode_image(&img)
        .map_err(|e| PdfError::Image(e.to_string()))?;
    let mut image_obj = PdfPageImageObject::new_from_jpeg_reader(doc, std::io::Cursor::new(jpeg))?;
    image_obj.scale(bbox.width().value, bbox.height().value)?;
    image_obj.translate(bbox.left(), bbox.bottom())?;
    page.objects_mut().add_image_object(image_obj)?;

    for seg in segments(chars, &spans) {
        let t = winansi(&seg.text);
        if t.trim().is_empty() {
            continue;
        }
        // Am Ursprung anlegen, Breite an das Original anpassen, dann verschieben
        // (Skalieren nach dem Verschieben würde auch die Position skalieren).
        let mut obj = page.objects_mut().create_text_object(
            PdfPoints::ZERO,
            PdfPoints::ZERO,
            &t,
            font,
            PdfPoints::new(seg.size.max(1.0)),
        )?;
        let width = obj.width().map(|w| w.value).unwrap_or(0.0);
        let target = seg.right - seg.x;
        let ratio = if width > 0.0 && target > 0.0 {
            if seg.visible {
                // Ersetzung: höchstens etwas breiter als die Fundstelle, sonst stauchen
                (target * 1.15 / width).min(1.0)
            } else {
                // unsichtbarer Text: genau über dem Original, nie über den Seitenrand
                target / width
            }
        } else {
            1.0
        };
        if let Some(txt) = obj.as_text_object_mut() {
            if seg.visible {
                txt.set_fill_color(PdfColor::BLACK)?;
            } else {
                txt.set_render_mode(PdfPageTextRenderMode::Invisible)?;
            }
        }
        if (ratio - 1.0).abs() > 0.01 {
            obj.scale(ratio, 1.0)?;
        }
        obj.translate(PdfPoints::new(seg.x), PdfPoints::new(seg.y))?;
    }

    page.regenerate_content()?;
    page.set_rotation(rotation);
    Ok(())
}

/// Umschliessende Rechtecke (links, unten, rechts, oben) je Textzeile.
fn line_boxes(chars: &[PageChar]) -> Vec<(f32, f32, f32, f32)> {
    let mut out: Vec<(f32, f32, f32, f32)> = Vec::new();
    for r in chars.iter().filter_map(|c| c.rect) {
        let (l, b, rr, t) = (r.left().value, r.bottom().value, r.right().value, r.top().value);
        let mid = (b + t) / 2.0;
        match out.last_mut() {
            Some(cur) if mid > cur.1 && mid < cur.3 && l >= cur.0 - 1.0 => {
                cur.0 = cur.0.min(l);
                cur.1 = cur.1.min(b);
                cur.2 = cur.2.max(rr);
                cur.3 = cur.3.max(t);
            }
            _ => out.push((l, b, rr, t)),
        }
    }
    out
}

/// Zerlegt den Seitentext in Zeilenstücke: unsichtbarer Originaltext und
/// sichtbare Ersetzungen an der Stelle der Fundstelle.
fn segments(chars: &[PageChar], spans: &[(usize, usize, &str)]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cur: Option<Segment> = None;
    let mut i = 0;
    let mut span_iter = spans.iter().peekable();
    while i < chars.len() {
        if let Some(&&(a, b, repl)) = span_iter.peek() {
            if i == a {
                out.extend(cur.take());
                if let Some((x, y, size)) = first_pos(&chars[a..b]) {
                    let right = chars[a..b].iter().filter_map(|c| c.rect).map(|r| r.right().value).fold(x, f32::max);
                    out.push(Segment { text: repl.to_string(), x, y, size, right, visible: true });
                }
                i = b;
                span_iter.next();
                continue;
            }
        }
        let c = &chars[i];
        if c.ch == '\n' || c.ch == '\r' {
            out.extend(cur.take());
        } else if let Some((x, y)) = c.origin {
            // Neue Zeile ohne Umbruchzeichen (z.B. bei Silbentrennung zusammengefügt)
            // oder Sprung nach links: neues Stück beginnen.
            let new_line = cur.as_ref().is_some_and(|seg| (y - seg.y).abs() > seg.size.max(c.size) * 0.5 || x < seg.right - seg.size);
            if new_line {
                out.extend(cur.take());
            }
            let right = c.rect.map(|r| r.right().value).unwrap_or(x);
            match &mut cur {
                Some(seg) => {
                    seg.text.push(c.ch);
                    seg.right = seg.right.max(right);
                }
                None => cur = Some(Segment { text: c.ch.to_string(), x, y, size: c.size, right, visible: false }),
            }
        } else if let Some(seg) = &mut cur {
            // erzeugtes Leerzeichen innerhalb der Zeile
            seg.text.push(c.ch);
        }
        i += 1;
    }
    out.extend(cur);
    out
}

fn first_pos(chars: &[PageChar]) -> Option<(f32, f32, f32)> {
    chars.iter().find_map(|c| c.origin.map(|(x, y)| (x, y, c.size)))
}

/// Helvetica (Standardschrift) kann nur WinAnsi: verbreitete Zeichen werden
/// umgesetzt, übrige zu «?».
fn winansi(s: &str) -> String {
    const EXTRA: &str = "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ";
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' '..='~' | '\u{A0}'..='\u{FF}' => out.push(c),
            c if EXTRA.contains(c) => out.push(c),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2212}' | '\u{2}' | '\u{FFFE}' => out.push('-'),
            '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\t' => out.push(' '),
            '\u{200B}'..='\u{200D}' | '\u{FEFF}' => {}
            '→' | '⇒' | '⟶' => out.push_str("->"),
            '←' | '⇐' | '⟵' => out.push_str("<-"),
            '↔' | '⇔' => out.push_str("<->"),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '≠' => out.push_str("!="),
            '\u{F0B7}' | '\u{F0A7}' | '▪' | '■' | '●' | '◦' | '○' => out.push('•'),
            '✓' | '✔' => out.push('v'),
            _ => out.push('?'),
        }
    }
    out
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

/// Entfernt personenbezogene Metadaten (Info-Dict, XMP-Metadaten) und nicht
/// mehr referenzierte Objekte (z.B. ersetzte Content-Streams mit Klarnamen).
fn clean_saved(path: &Path) -> Result<(), PdfError> {
    let mut doc = lopdf::Document::load(path)?;
    doc.trailer.remove(b"Info");
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
    doc.prune_objects();
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
    fn only_pages_with_hits_are_rebuilt_and_name_is_gone_from_raw_data() {
        let Some(pdfium) = try_pdfium() else { return };
        let inp = tmp("in.pdf");
        let out = tmp("out.pdf");
        {
            let mut doc = pdfium.create_new_pdf().unwrap();
            for line in ["Name: Anna Müller", "Seite ohne Namen"] {
                let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
                let font = doc.fonts_mut().times_roman();
                page.objects_mut()
                    .create_text_object(PdfPoints::new(50.0), PdfPoints::new(700.0), line, font, PdfPoints::new(14.0))
                    .unwrap();
            }
            doc.save_to_file(&inp).unwrap();
        }
        let fwd = vec![("Anna Müller".to_string(), "Leo Roth".to_string()), ("Müller".to_string(), "Roth".to_string())];
        let stats = process_pdf(&inp, &out, &fwd).unwrap();
        assert_eq!(stats.replacements, 1);
        assert!(stats.residual_matches.is_empty(), "{:?}", stats.residual_matches);

        let doc = pdfium.load_pdf_from_file(&out, None).unwrap();
        let p1 = doc.pages().get(0).unwrap();
        let t1 = p1.text().unwrap().all();
        assert!(t1.contains("Name:") && t1.contains("Leo Roth"), "{t1}");
        assert!(!t1.contains("Müller"), "{t1}");
        // Seite 1: Bild + neue Textebene; Seite 2 unverändert (nur das Textobjekt)
        assert!(p1.objects().iter().any(|o| o.as_image_object().is_some()));
        let p2 = doc.pages().get(1).unwrap();
        assert_eq!(p2.text().unwrap().all(), "Seite ohne Namen");
        assert_eq!(p2.objects().len(), 1);
        drop(doc);

        // Auch in den (entpackten) Rohdaten steht der Name nirgends
        let raw = lopdf::Document::load(&out).unwrap();
        for obj in raw.objects.values() {
            if let lopdf::Object::Stream(st) = obj {
                let c = st.decompressed_content().unwrap_or_else(|_| st.content.clone());
                assert!(!String::from_utf8_lossy(&c).contains("Müller"));
            }
        }
        let _ = std::fs::remove_file(&inp);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn segments_split_lines_and_place_replacement() {
        let ch = |c: char, x: f32, y: f32| PageChar {
            ch: c,
            rect: Some(PdfRect::new_from_values(y, x, y + 10.0, x + 5.0)),
            origin: Some((x, y)),
            size: 10.0,
        };
        let gen = |c: char| PageChar { ch: c, rect: None, origin: None, size: 0.0 };
        // «Hi Bob» auf Zeile 1, «ok» auf Zeile 2 ohne Umbruchzeichen
        let chars = vec![
            ch('H', 0.0, 100.0), ch('i', 5.0, 100.0), gen(' '),
            ch('B', 15.0, 100.0), ch('o', 20.0, 100.0), ch('b', 25.0, 100.0),
            ch('o', 0.0, 80.0), ch('k', 5.0, 80.0),
        ];
        let segs = segments(&chars, &[(3, 6, "Leo")]);
        let v: Vec<(&str, bool, f32)> = segs.iter().map(|s| (s.text.as_str(), s.visible, s.y)).collect();
        assert_eq!(v, vec![("Hi ", false, 100.0), ("Leo", true, 100.0), ("ok", false, 80.0)]);
        assert_eq!(segs[1].x, 15.0);
    }

    #[test]
    fn winansi_maps_common_symbols() {
        assert_eq!(winansi("E\u{2011}Mail → Ende ≥ 5 ✓ Grüße"), "E-Mail -> Ende >= 5 v Grüße");
        assert_eq!(winansi("日本"), "??");
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
