//! Zentrale Format-Tabelle: welche Dateien Nema verarbeitet, wie, und was
//! bei nicht unterstützten Dateien zu tun ist.
//!
//! App und Läufe rufen nur `process_file` und `scan_fuzzy` auf; ein neues
//! Format braucht nur hier einen Eintrag.

use crate::archive;
use crate::docx;
use crate::fuzzy::FuzzyHit;
use crate::mapping;
use crate::office::{self, OfficeKind};
use crate::pdf;
use crate::textfile::{self, TextKind};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Docx,
    Pdf,
    Text(TextKind),
    Office(OfficeKind),
    Zip,
}

/// Endung → Format. Reihenfolge = Reihenfolge im Datei-Dialog.
const TABLE: &[(&str, Format)] = &[
    ("docx", Format::Docx),
    ("pdf", Format::Pdf),
    ("xlsx", Format::Office(OfficeKind::Xlsx)),
    ("pptx", Format::Office(OfficeKind::Pptx)),
    ("odt", Format::Office(OfficeKind::Odf)),
    ("ods", Format::Office(OfficeKind::Odf)),
    ("odp", Format::Office(OfficeKind::Odf)),
    ("zip", Format::Zip),
    ("txt", Format::Text(TextKind::Plain)),
    ("md", Format::Text(TextKind::Markdown)),
    ("markdown", Format::Text(TextKind::Markdown)),
    ("csv", Format::Text(TextKind::Csv)),
    ("tsv", Format::Text(TextKind::Csv)),
    ("json", Format::Text(TextKind::Json)),
    ("ipynb", Format::Text(TextKind::Json)),
    ("html", Format::Text(TextKind::Html)),
    ("htm", Format::Text(TextKind::Html)),
    ("xhtml", Format::Text(TextKind::Html)),
    ("xml", Format::Text(TextKind::Xml)),
    ("svg", Format::Text(TextKind::Xml)),
    ("bpmn", Format::Text(TextKind::Xml)),
    ("tex", Format::Text(TextKind::Tex)),
    ("bib", Format::Text(TextKind::Tex)),
    ("yaml", Format::Text(TextKind::Plain)),
    ("yml", Format::Text(TextKind::Plain)),
    ("toml", Format::Text(TextKind::Plain)),
    ("ini", Format::Text(TextKind::Plain)),
    ("log", Format::Text(TextKind::Plain)),
    // Quellcode: Namen stehen in Kopfkommentaren, Strings, E-Mail-Adressen
    ("py", Format::Text(TextKind::Plain)),
    ("java", Format::Text(TextKind::Plain)),
    ("js", Format::Text(TextKind::Plain)),
    ("mjs", Format::Text(TextKind::Plain)),
    ("ts", Format::Text(TextKind::Plain)),
    ("jsx", Format::Text(TextKind::Plain)),
    ("tsx", Format::Text(TextKind::Plain)),
    ("cs", Format::Text(TextKind::Plain)),
    ("c", Format::Text(TextKind::Plain)),
    ("h", Format::Text(TextKind::Plain)),
    ("cpp", Format::Text(TextKind::Plain)),
    ("hpp", Format::Text(TextKind::Plain)),
    ("ino", Format::Text(TextKind::Plain)),
    ("sql", Format::Text(TextKind::Plain)),
    ("php", Format::Text(TextKind::Plain)),
    ("css", Format::Text(TextKind::Plain)),
    ("scss", Format::Text(TextKind::Plain)),
    ("sh", Format::Text(TextKind::Plain)),
    ("ps1", Format::Text(TextKind::Plain)),
    ("bat", Format::Text(TextKind::Plain)),
    ("rb", Format::Text(TextKind::Plain)),
    ("go", Format::Text(TextKind::Plain)),
    ("rs", Format::Text(TextKind::Plain)),
    ("kt", Format::Text(TextKind::Plain)),
    ("swift", Format::Text(TextKind::Plain)),
    ("r", Format::Text(TextKind::Plain)),
    ("vb", Format::Text(TextKind::Plain)),
    ("lua", Format::Text(TextKind::Plain)),
    ("puml", Format::Text(TextKind::Plain)),
    ("mmd", Format::Text(TextKind::Plain)),
];

/// Bekannte, aber nicht unterstützte Endungen mit Hinweis, was zu tun ist.
const HINTS: &[(&[&str], &str)] = &[
    (&["doc"], "altes Word-Format: in Word als .docx speichern"),
    (&["xls"], "altes Excel-Format: in Excel als .xlsx speichern"),
    (&["ppt"], "altes PowerPoint-Format: als .pptx speichern"),
    (&["rtf"], "in Word als .docx speichern"),
    (&["pages", "numbers", "key"], "Apple-Format: als .docx, .xlsx, .pptx oder PDF exportieren"),
    (&["png", "jpg", "jpeg", "gif", "heic", "bmp", "tif", "tiff", "webp"], "Bild: Namen in Bildern erkennt Nema nicht, bitte selbst prüfen"),
    (&["eml", "msg"], "E-Mail: Inhalt als PDF speichern"),
    (&["rar", "7z", "tar", "gz"], "Archiv: entpacken oder als .zip packen"),
];

fn ext_of(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn format_of(path: &Path) -> Option<Format> {
    let ext = ext_of(path);
    TABLE.iter().find(|(e, _)| *e == ext).map(|(_, f)| *f)
}

pub fn is_supported(path: &Path) -> bool {
    format_of(path).is_some()
}

/// Alle unterstützten Endungen (für den Datei-Dialog).
pub fn supported_extensions() -> Vec<&'static str> {
    TABLE.iter().map(|(e, _)| *e).collect()
}

/// Was tun mit einer nicht unterstützten Datei?
pub fn unsupported_hint(path: &Path) -> &'static str {
    let ext = ext_of(path);
    HINTS
        .iter()
        .find(|(exts, _)| exts.contains(&ext.as_str()))
        .map_or("wird nicht unterstützt", |(_, h)| h)
}

/// Zusammenfassung übersprungener Dateien, nach Endung gruppiert und ohne
/// Dateinamen (die können Namen enthalten): «2 × .doc (altes Word-Format: …)».
pub fn summarize_skipped(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let mut by_ext: BTreeMap<String, (usize, &'static str)> = BTreeMap::new();
    for p in paths {
        let ext = ext_of(p);
        let label = if ext.is_empty() { "ohne Endung".to_string() } else { format!(".{ext}") };
        by_ext.entry(label).or_insert((0, unsupported_hint(p))).0 += 1;
    }
    Some(by_ext.into_iter().map(|(ext, (n, hint))| format!("{n} × {ext} ({hint})")).collect::<Vec<_>>().join(", "))
}

#[derive(Debug, Error)]
pub enum FormatError {
    #[error(transparent)]
    Docx(#[from] docx::DocxError),
    #[error(transparent)]
    Pdf(#[from] pdf::PdfError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Regex: {0}")]
    Regex(#[from] regex::Error),
    #[error("Format .{0} wird nicht unterstützt")]
    Unsupported(String),
}

/// Ergebnis einer verarbeiteten Datei.
#[derive(Debug, Default, Clone)]
pub struct FileStats {
    pub replacements: usize,
    /// Ersetzungen je Suchtext (Originalschreibweise des Mapping-Schlüssels)
    pub per_identifier: HashMap<String, usize>,
    /// PDF: Treffer, die nach der Ersetzung noch im Text der Ausgabe stehen
    pub residual_matches: Vec<String>,
    /// Hinweise ohne Namen, z.B. «2 Dateien im ZIP übersprungen (.doc)»
    pub warnings: Vec<String>,
}

impl FileStats {
    fn from_counts(replacements: usize, per_identifier: HashMap<String, usize>) -> Self {
        Self { replacements, per_identifier, ..Default::default() }
    }

    pub(crate) fn merge(&mut self, other: FileStats) {
        self.replacements += other.replacements;
        for (k, v) in other.per_identifier {
            *self.per_identifier.entry(k).or_insert(0) += v;
        }
        self.residual_matches.extend(other.residual_matches);
        self.warnings.extend(other.warnings);
    }
}

/// Verarbeitet eine Datei mit dem Mapping (suchtext → ersatz).
pub fn process_file(input: &Path, output: &Path, mapping: &[(String, String)]) -> Result<FileStats, FormatError> {
    match format_of(input).ok_or_else(|| FormatError::Unsupported(ext_of(input)))? {
        Format::Docx => {
            let s = docx::process_docx(input, output, mapping)?;
            Ok(FileStats::from_counts(s.replacements, s.per_identifier))
        }
        Format::Pdf => {
            let s = pdf::process_pdf(input, output, mapping)?;
            Ok(FileStats { residual_matches: s.residual_matches, ..FileStats::from_counts(s.replacements, s.per_identifier) })
        }
        Format::Text(kind) => {
            let (n, per) = textfile::process_text_file(input, output, kind, mapping)?;
            Ok(FileStats::from_counts(n, per))
        }
        Format::Office(kind) => {
            let h = office::process_office(input, output, kind, mapping)?;
            Ok(FileStats::from_counts(h.replacements, h.per_identifier))
        }
        Format::Zip => archive::process_zip(input, output, mapping),
    }
}

/// Sucht Tippfehler-Varianten der Namen (Review-Liste).
pub fn scan_fuzzy(input: &Path, mapping: &[(String, String)]) -> Result<Vec<FuzzyHit>, FormatError> {
    match format_of(input).ok_or_else(|| FormatError::Unsupported(ext_of(input)))? {
        Format::Docx => Ok(docx::scan_fuzzy_docx(input, mapping)?),
        Format::Pdf => Ok(pdf::scan_fuzzy_pdf(input, mapping)?),
        Format::Text(kind) => Ok(textfile::scan_fuzzy_text(input, kind, mapping)?),
        Format::Office(kind) => office::scan_fuzzy_office(input, kind, mapping),
        Format::Zip => archive::scan_fuzzy_zip(input, mapping),
    }
}

/// Gemeinsamer Elternordner aller Dateien.
fn common_dir(files: &[PathBuf]) -> Option<PathBuf> {
    let mut common: Option<PathBuf> = None;
    for f in files {
        let parent = f.parent()?;
        common = Some(match common {
            None => parent.to_path_buf(),
            Some(c) => {
                let mut acc = PathBuf::new();
                for (a, b) in c.components().zip(parent.components()) {
                    if a != b {
                        break;
                    }
                    acc.push(a);
                }
                acc
            }
        });
    }
    common
}

/// Ausgabepfade für einen Lauf. Unterordner werden ab dem gemeinsamen
/// Elternordner nachgebildet (zwei «Abgabe.docx» aus verschiedenen
/// Abgabe-Ordnern überschreiben sich so nicht). Ordner- und Dateinamen werden
/// übersetzt. Liegen die Dateien weit verstreut (gemeinsamer Ordner ist das
/// Wurzel- oder Home-Verzeichnis), bleibt es flach, damit z.B. der eigene
/// Benutzername nicht im Export steht. Doppelte Namen bekommen «_2», «_3» …
pub fn output_paths(files: &[PathBuf], out_dir: &Path, mapping: &[(String, String)], suffix: &str) -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let common = common_dir(files).filter(|c| {
        c.components().any(|x| matches!(x, Component::Normal(_))) && !home.as_ref().is_some_and(|h| h.starts_with(c))
    });
    let mut used: HashSet<String> = HashSet::new();
    files
        .iter()
        .map(|f| {
            let mut dir = out_dir.to_path_buf();
            if let Some(rel) = common.as_ref().and_then(|c| f.parent()?.strip_prefix(c).ok()) {
                for c in rel.components() {
                    if let Component::Normal(s) = c {
                        dir.push(mapping::output_stem(&s.to_string_lossy(), mapping));
                    }
                }
            }
            output_name_in(&dir, f, mapping, suffix, &mut used)
        })
        .collect()
}

/// Freier Ausgabepfad in `dir` für `input` (übersetzter Name + Suffix).
pub(crate) fn output_name_in(dir: &Path, input: &Path, mapping: &[(String, String)], suffix: &str, used: &mut HashSet<String>) -> PathBuf {
    let stem = input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "datei".into());
    let ext = ext_of(input);
    let base = mapping::output_stem(&stem, mapping);
    let dot_ext = if ext.is_empty() { String::new() } else { format!(".{ext}") };
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{base}{suffix}{dot_ext}") } else { format!("{base}_{n}{suffix}{dot_ext}") };
        let p = dir.join(name);
        if used.insert(p.to_string_lossy().to_lowercase()) {
            return p;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_and_hints() {
        assert_eq!(format_of(Path::new("a/B.DOCX")), Some(Format::Docx));
        assert_eq!(format_of(Path::new("x.ipynb")), Some(Format::Text(TextKind::Json)));
        assert!(format_of(Path::new("x.doc")).is_none());
        assert!(unsupported_hint(Path::new("x.doc")).contains(".docx"));
        assert_eq!(unsupported_hint(Path::new("x.xyz")), "wird nicht unterstützt");
        let s = summarize_skipped(&[PathBuf::from("Müller.doc"), PathBuf::from("b.doc"), PathBuf::from("c.PNG")]).unwrap();
        assert!(s.starts_with("2 × .doc (") && s.contains("1 × .png (Bild"), "{s}");
        assert!(!s.contains("Müller"));
        assert!(summarize_skipped(&[]).is_none());
        // jede Endung nur einmal
        let exts = supported_extensions();
        assert_eq!(exts.len(), exts.iter().collect::<HashSet<_>>().len());
    }

    #[test]
    fn output_paths_mirror_subfolders_and_avoid_collisions() {
        let mapping = vec![("Anna Müller".to_string(), "Lea Roth".to_string()), ("Ben Graf".to_string(), "Max Wolf".to_string())];
        let root = PathBuf::from("/srv/abgaben/M349");
        let files = vec![
            root.join("Anna Müller_123_assignsubmission_file_/Abgabe.docx"),
            root.join("Ben Graf_456_assignsubmission_file_/Abgabe.docx"),
            root.join("Ben Graf_456_assignsubmission_file_/abgabe.DOCX"),
        ];
        let out = output_paths(&files, Path::new("/out"), &mapping, "_anonym");
        assert_eq!(
            out,
            [
                PathBuf::from("/out/Lea Roth_123_assignsubmission_file_/Abgabe_anonym.docx"),
                PathBuf::from("/out/Max Wolf_456_assignsubmission_file_/Abgabe_anonym.docx"),
                PathBuf::from("/out/Max Wolf_456_assignsubmission_file_/abgabe_2_anonym.docx"),
            ]
        );
        // Dateien aus einem Ordner: flach
        let flat = output_paths(&[root.join("a.pdf"), root.join("b.pdf")], Path::new("/out"), &mapping, "");
        assert_eq!(flat, [PathBuf::from("/out/a.pdf"), PathBuf::from("/out/b.pdf")]);
        // weit verstreut (gemeinsam nur «/»): flach statt /out/srv/…
        let spread = output_paths(&[root.join("a.pdf"), PathBuf::from("/tmp/a.pdf")], Path::new("/out"), &mapping, "");
        assert_eq!(spread, [PathBuf::from("/out/a.pdf"), PathBuf::from("/out/a_2.pdf")]);
    }
}
