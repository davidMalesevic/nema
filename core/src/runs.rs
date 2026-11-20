//! Anonymisierungs-Läufe (Runs) + Audit-Log.
//!
//! Ein Lauf exportiert ausgewählte Prüfungen anonymisiert in einen Ordner
//! (dieser Ordner ist für die KI bestimmt und enthält nur Pseudonyme).
//! Jeder Lauf wird im verschlüsselten Audit-Log (`runs.nema` im Tresor)
//! protokolliert — ausschließlich mit Pseudonymen, nie mit Klarnamen.
//!
//! De-Anonymisieren läuft als eigener Lauf in Gegenrichtung.

use crate::formats::{self, FormatError};
use crate::mapping;
use crate::store::{StoreError, VaultStore};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Format(#[from] FormatError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Projekt nicht gefunden")]
    ProjectNotFound,
    #[error("kein unterstütztes Dokument in {0}")]
    NoDocuments(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunDirection {
    Anonymize,
    Deanonymize,
}

impl RunDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunDirection::Anonymize => "anonymisiert",
            RunDirection::Deanonymize => "de-anonymisiert",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEntry {
    /// Pfad der Quelle relativ zum Lauf-Ordner, nur mit Pseudonymen
    pub source: String,
    /// Pfad der Ausgabe relativ zum Lauf-Ordner, nur mit Pseudonymen
    pub output: String,
    pub replacements: usize,
    /// nur Pseudonyme (bzw. beim De-Anonymisieren: die ersetzten Klarnamen
    /// werden NICHT geloggt — hier stehen die Such-Treffer der Gegenrichtung)
    pub replaced_tokens: Vec<String>,
    /// PDF: Treffer, die nach der Ersetzung noch im Text der Ausgabe standen
    pub residuals: Vec<String>,
    /// Hinweise ohne Namen (z.B. im ZIP übersprungene Dateien)
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub project_name: String,
    pub label: String,
    pub direction: RunDirection,
    pub created_at: DateTime<Utc>,
    pub entries: Vec<RunEntry>,
}

/// Namensanfang der Export-Ordner (neutral, ohne Bezeichnung).
pub const EXPORT_FOLDER_PREFIX: &str = "Nema-Export";

/// Dateisystem-sicherer Ordnername aus einem Label.
pub fn safe_folder_name(label: &str) -> String {
    let mut s: String = label
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    s.truncate(60);
    if s.is_empty() {
        s = "lauf".to_string();
    }
    s
}

/// Aus den ersetzten Bezeichnern die tatsächlich verwendeten Ziel-Tokens
/// (Pseudonyme) ableiten — ohne Klarnamen zu loggen.
fn used_tokens(per_identifier: &std::collections::HashMap<String, usize>, mapping: &[(String, String)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (k, count) in per_identifier {
        if *count == 0 {
            continue;
        }
        if let Some((_, v)) = mapping.iter().find(|(mk, _)| mk.to_lowercase() == k.to_lowercase()) {
            if !out.contains(v) {
                out.push(v.clone());
            }
        }
    }
    out.sort();
    out
}

/// Exportiert Dateien anonymisiert in einen neuen Lauf-Ordner und loggt den Lauf.
/// Rückgabe: (RunRecord, Ordnerpfad).
pub fn export_anonymized(
    store: &VaultStore,
    project_id: Uuid,
    label: &str,
    files: &[PathBuf],
    out_root: &Path,
) -> Result<(RunRecord, PathBuf), RunError> {
    let vault = store.load_project(project_id).map_err(|_| RunError::ProjectNotFound)?;
    let mapping = mapping::anonymize_mapping(&vault);
    run_files(&RunParams {
        store,
        vault: &vault,
        label,
        files,
        out_root,
        in_root: None,
        mapping: &mapping,
        direction: RunDirection::Anonymize,
        suffix: "_anonym",
    })
}

/// De-anonymisiert alle unterstützten Dateien in einem Lauf-Ordner samt
/// Unterordnern zurück in `out_dir` (eigener Unterordner pro Lauf, gleiche
/// Ordnerstruktur) und loggt den Lauf. Rückgabe: (RunRecord, Ordnerpfad).
pub fn deanonymize_folder(
    store: &VaultStore,
    project_id: Uuid,
    label: &str,
    folder: &Path,
    out_dir: &Path,
) -> Result<(RunRecord, PathBuf), RunError> {
    let vault = store.load_project(project_id).map_err(|_| RunError::ProjectNotFound)?;
    let mapping = mapping::deanonymize_mapping(&vault);
    let files = collect_supported(folder)?;
    if files.is_empty() {
        return Err(RunError::NoDocuments(folder.display().to_string()));
    }
    run_files(&RunParams {
        store,
        vault: &vault,
        label,
        files: &files,
        out_root: out_dir,
        in_root: Some(folder),
        mapping: &mapping,
        direction: RunDirection::Deanonymize,
        suffix: "_deanonym",
    })
}

/// Alle unterstützten Dateien in `dir` samt Unterordnern, sortiert; versteckte
/// Einträge und Office-Sperrdateien (`~$…`) ausgenommen.
fn collect_supported(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with("~$") {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() && formats::is_supported(&e.path()) {
                out.push(e.path());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// «a/b_anonym.docx» → «a/b.docx»
fn without_suffix(path: &str, suffix: &str) -> String {
    match path.rfind(suffix) {
        Some(i) if !suffix.is_empty() => format!("{}{}", &path[..i], &path[i + suffix.len()..]),
        _ => path.to_string(),
    }
}

/// «a/b.docx» → «a/b_deanonym.docx»
fn with_suffix(path: &str, suffix: &str) -> String {
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    match path[name_start..].rfind('.') {
        Some(dot) if dot > 0 => {
            let dot = name_start + dot;
            format!("{}{suffix}{}", &path[..dot], &path[dot..])
        }
        _ => format!("{path}{suffix}"),
    }
}

/// Pfad relativ zu `base` mit `/` als Trenner (fürs Protokoll).
fn rel_string(path: &Path, base: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

struct RunParams<'a> {
    store: &'a VaultStore,
    vault: &'a crate::project::ProjectVault,
    label: &'a str,
    files: &'a [PathBuf],
    out_root: &'a Path,
    /// Eingangsordner (De-Anonymisieren), für relative Pfade im Protokoll
    in_root: Option<&'a Path>,
    mapping: &'a [(String, String)],
    direction: RunDirection,
    suffix: &'a str,
}

fn run_files(p: &RunParams<'_>) -> Result<(RunRecord, PathBuf), RunError> {
    let RunParams { store, vault, label, files, out_root, in_root, mapping, direction, suffix } = *p;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    // Der Export-Ordner geht an die KI: neutraler Name, die Bezeichnung steht nur im Protokoll.
    // Zurück (lokal, mit Klarnamen) darf der Ordner die Bezeichnung tragen.
    let folder_name = match direction {
        RunDirection::Anonymize => format!("{EXPORT_FOLDER_PREFIX}_{stamp}"),
        RunDirection::Deanonymize => format!("{}_{}", safe_folder_name(label), stamp),
    };
    let folder = out_root.join(folder_name);
    std::fs::create_dir_all(&folder)?;

    let outputs = formats::output_paths(files, &folder, mapping, suffix);
    let mut entries = Vec::new();
    for (f, out) in files.iter().zip(&outputs) {
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let stats = formats::process_file(f, out, mapping)?;
        let tokens = used_tokens(&stats.per_identifier, mapping);
        let out_rel = rel_string(out, &folder);
        // Im Protokoll stehen nur pseudonyme Pfade, nie die mit Klarnamen.
        let (logged_source, logged_output) = match direction {
            RunDirection::Anonymize => (without_suffix(&out_rel, suffix), out_rel),
            RunDirection::Deanonymize => {
                let src = rel_string(f, in_root.unwrap_or(Path::new("")));
                let with_suffix = with_suffix(&src, suffix);
                (src, with_suffix)
            }
        };
        entries.push(RunEntry {
            source: logged_source,
            output: logged_output,
            replacements: stats.replacements,
            replaced_tokens: tokens,
            residuals: stats.residual_matches,
            warnings: stats.warnings,
        });
    }

    let record = RunRecord {
        id: Uuid::new_v4(),
        project_id: vault.id,
        project_name: vault.name.clone(),
        label: label.to_string(),
        direction,
        created_at: Utc::now(),
        entries,
    };
    store.log_run(&record)?;
    Ok((record, folder))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::person::Person;
    use crate::store::VaultStore;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nema-runs-test-{}-{name}", Uuid::new_v4()))
    }

    fn make_docx(path: &Path, text: &str) {
        let doc = format!(
            r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
        );
        let f = std::fs::File::create(path).unwrap();
        let mut w = ZipWriter::new(f);
        let opt = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        w.start_file("word/document.xml", opt).unwrap();
        w.write_all(doc.as_bytes()).unwrap();
        w.finish().unwrap();
    }

    fn read_docx_text(path: &Path) -> String {
        let f = std::fs::File::open(path).unwrap();
        let mut a = zip::ZipArchive::new(f).unwrap();
        let mut e = a.by_name("word/document.xml").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut e, &mut s).unwrap();
        s
    }

    fn setup() -> (PathBuf, VaultStore, Uuid, String) {
        let dir = tmp("vault");
        let store = VaultStore::init(&dir, false, Some("passwort-123")).unwrap();
        let mut vault = store.create_project("Mathe 10a").unwrap();
        vault.persons.push(Person::new("Anna", "Müller"));
        vault.ensure_pseudonyms();
        let pseudo = vault.assignment_for(vault.persons[0].id).unwrap().full();
        store.save_project(&vault).unwrap();
        let id = vault.id;
        (dir, store, id, pseudo)
    }

    #[test]
    fn export_then_deanonymize_roundtrip_and_log() {
        let (dir, store, pid, pseudo) = setup();
        let work = tmp("work");
        std::fs::create_dir_all(&work).unwrap();
        let f1 = work.join("pruefung1.docx");
        make_docx(&f1, "Prüfung von Anna Müller");

        let out_root = tmp("export");
        std::fs::create_dir_all(&out_root).unwrap();
        let (record, folder) = export_anonymized(&store, pid, "Klassenarbeit 1", std::slice::from_ref(&f1), &out_root).unwrap();
        assert_eq!(record.direction, RunDirection::Anonymize);
        assert_eq!(record.entries.len(), 1);
        assert_eq!(record.entries[0].replacements, 1);
        // nur Pseudonym im Log, kein Klarname
        assert!(record.entries[0].replaced_tokens.iter().all(|t| !t.contains("Müller")));
        let anon_file = folder.join("pruefung1_anonym.docx");
        let txt = read_docx_text(&anon_file);
        assert!(!txt.contains("Müller"), "exportierter Text: {txt}");
        assert!(txt.contains(&pseudo), "exportierter Text: {txt}, erwartet Pseudonym: {pseudo}");

        // Audit-Log: verschlüsselt, ohne Klarnamen
        let raw = std::fs::read(dir.join("runs.nema")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("Müller"));
        let runs = store.list_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].label, "Klassenarbeit 1");

        // De-Anonymisieren des Lauf-Ordners
        let back_dir = tmp("back");
        std::fs::create_dir_all(&back_dir).unwrap();
        let (rec2, back_folder) = deanonymize_folder(&store, pid, "Klassenarbeit 1 zurück", &folder, &back_dir).unwrap();
        assert_eq!(rec2.direction, RunDirection::Deanonymize);
        let back_file = back_folder.join("pruefung1_anonym_deanonym.docx");
        let txt2 = read_docx_text(&back_file);
        assert!(txt2.contains("Anna Müller"), "zurück: {txt2}");

        let runs = store.list_runs().unwrap();
        assert_eq!(runs.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&work);
        let _ = std::fs::remove_dir_all(&out_root);
        let _ = std::fs::remove_dir_all(&back_dir);
    }

    #[test]
    fn export_hides_names_in_file_and_folder_names() {
        let (dir, store, pid, _) = setup();
        let vault = store.load_project(pid).unwrap();
        let a = vault.assignment_for(vault.persons[0].id).unwrap().clone();
        let work = tmp("work");
        std::fs::create_dir_all(&work).unwrap();
        let f1 = work.join("Müller_Anna.docx");
        make_docx(&f1, "Prüfung von Anna Müller");
        let out_root = tmp("export");
        std::fs::create_dir_all(&out_root).unwrap();

        let (record, folder) = export_anonymized(&store, pid, "INF 24a Müller", std::slice::from_ref(&f1), &out_root).unwrap();
        let folder_name = folder.file_name().unwrap().to_string_lossy().to_string();
        assert!(folder_name.starts_with(EXPORT_FOLDER_PREFIX), "{folder_name}");
        assert!(!folder_name.contains("INF") && !folder_name.contains("Müller"), "{folder_name}");
        let names: Vec<String> = std::fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        assert_eq!(names, vec![format!("{}_{}_anonym.docx", a.last_name, a.first_name)]);
        assert!(!record.entries[0].source.contains("Müller") && !record.entries[0].output.contains("Müller"));

        // zurück: Dateiname und Inhalt tragen wieder den echten Namen, das Protokoll nicht
        let back_dir = tmp("back");
        std::fs::create_dir_all(&back_dir).unwrap();
        let (rec2, back_folder) = deanonymize_folder(&store, pid, "zurück", &folder, &back_dir).unwrap();
        let back_file = back_folder.join("Müller_Anna_anonym_deanonym.docx");
        assert!(back_file.exists(), "{:?}", std::fs::read_dir(&back_folder).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>());
        assert!(read_docx_text(&back_file).contains("Anna Müller"));
        assert!(!rec2.entries[0].source.contains("Müller") && !rec2.entries[0].output.contains("Müller"));
        let raw = std::fs::read(dir.join("runs.nema")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("Müller"));

        for d in [&dir, &work, &out_root, &back_dir] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn subfolders_text_formats_and_roundtrip() {
        let (dir, store, pid, pseudo) = setup();
        let work = tmp("work");
        // Moodle-Download: zwei «Abgabe»-Dateien in Ordnern mit Namen
        for sub in ["Anna Müller_1_assignsubmission_file_", "Gast_2_assignsubmission_file_"] {
            std::fs::create_dir_all(work.join(sub)).unwrap();
        }
        let f1 = work.join("Anna Müller_1_assignsubmission_file_/Abgabe.md");
        let f2 = work.join("Gast_2_assignsubmission_file_/Abgabe.md");
        let f3 = work.join("Gast_2_assignsubmission_file_/bewertung.csv");
        std::fs::write(&f1, "# Abgabe von Anna M&uuml;ller\n").unwrap();
        std::fs::write(&f2, "# Abgabe\n").unwrap();
        std::fs::write(&f3, "Name;Note\nMüller, Anna;5.5\n").unwrap();
        let out_root = tmp("export");
        let files = vec![f1, f2, f3];
        let (rec, folder) = export_anonymized(&store, pid, "Moodle", &files, &out_root).unwrap();
        assert_eq!(rec.entries.len(), 3);
        let vault = store.load_project(pid).unwrap();
        let a = vault.assignment_for(vault.persons[0].id).unwrap().clone();
        let anon_dir = folder.join(format!("{}_1_assignsubmission_file_", pseudo));
        let md = std::fs::read_to_string(anon_dir.join("Abgabe_anonym.md")).unwrap();
        assert_eq!(md, format!("# Abgabe von {pseudo}\n"));
        assert!(folder.join("Gast_2_assignsubmission_file_/Abgabe_anonym.md").exists(), "zweite Abgabe überschreibt die erste nicht");
        let csv = std::fs::read_to_string(folder.join("Gast_2_assignsubmission_file_/bewertung_anonym.csv")).unwrap();
        assert!(csv.contains(&a.last_first()) && !csv.contains("Müller"), "{csv}");
        // Protokoll: relative Pfade, nur Pseudonyme; Tokens erkannt (Gross-/Kleinschreibung egal)
        assert_eq!(rec.entries[0].output, format!("{pseudo}_1_assignsubmission_file_/Abgabe_anonym.md"));
        assert_eq!(rec.entries[0].source, format!("{pseudo}_1_assignsubmission_file_/Abgabe.md"));
        assert_eq!(rec.entries[0].replaced_tokens, vec![pseudo.clone()]);
        assert!(rec.entries.iter().all(|e| !e.source.contains("Müller") && !e.output.contains("Müller")));

        // Rückweg: auch Unterordner, Struktur bleibt
        let back_dir = tmp("back");
        let (rec2, back) = deanonymize_folder(&store, pid, "zurück", &folder, &back_dir).unwrap();
        assert_eq!(rec2.entries.len(), 3);
        let md_back = std::fs::read_to_string(back.join("Anna Müller_1_assignsubmission_file_/Abgabe_anonym_deanonym.md")).unwrap();
        assert_eq!(md_back, "# Abgabe von Anna Müller\n");
        assert!(rec2.entries.iter().all(|e| !e.source.contains("Müller") && !e.output.contains("Müller")), "{:?}", rec2.entries);
        assert!(rec2.entries.iter().any(|e| e.output == format!("{pseudo}_1_assignsubmission_file_/Abgabe_anonym_deanonym.md")));
        for d in [&dir, &work, &out_root, &back_dir] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn suffix_helpers() {
        assert_eq!(without_suffix("a_anonym/b_anonym.docx", "_anonym"), "a_anonym/b.docx");
        assert_eq!(with_suffix("a.b/c.docx", "_x"), "a.b/c_x.docx");
        assert_eq!(with_suffix("a.b/c", "_x"), "a.b/c_x");
    }

    #[test]
    fn safe_folder_name_sanitizes() {
        assert_eq!(safe_folder_name("Klasse 10a / Prüfung 1"), "Klasse_10a___Prüfung_1");
        assert_eq!(safe_folder_name(""), "lauf");
    }
}
