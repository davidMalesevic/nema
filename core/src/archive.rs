//! ZIP-Archive, z.B. Sammel-Downloads aus Moodle oder Teams
//! («Anna Müller_12345_assignsubmission_file_/Abgabe.docx»).
//!
//! Jede unterstützte Datei im Archiv wird wie eine einzelne Datei verarbeitet
//! (auch verschachtelte ZIPs), Ordner- und Dateinamen werden übersetzt. Nicht
//! unterstützte Dateien kommen NICHT ins Ergebnis: sie könnten Namen enthalten,
//! die Nema nicht prüfen kann. Die Warnung nennt nur Anzahl und Endungen.

use crate::formats::{self, FileStats, FormatError};
use crate::fuzzy::FuzzyHit;
use crate::mapping;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Temporärer Arbeitsordner (nur für den eigenen Benutzer lesbar), wird beim
/// Verlassen gelöscht — die entpackten Dateien enthalten Klarnamen.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let p = std::env::temp_dir().join(format!("nema-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self(p))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Systemdateien, die wegfallen (macOS-Ressourcen enthalten Kopien der Dateinamen).
fn is_junk(name: &str) -> bool {
    name.split('/').any(|c| c == "__MACOSX" || c == ".DS_Store" || c == "Thumbs.db" || c == "desktop.ini" || c.starts_with("._"))
}

/// Übersetzter Pfad im Archiv.
fn output_entry_name(name: &str, mapping: &[(String, String)], used: &mut HashSet<String>) -> String {
    let path = Path::new(name);
    let mut dir = PathBuf::new();
    if let Some(parent) = path.parent() {
        for c in parent.components() {
            if let std::path::Component::Normal(s) = c {
                dir.push(mapping::output_stem(&s.to_string_lossy(), mapping));
            }
        }
    }
    formats::output_name_in(&dir, path, mapping, "", used).to_string_lossy().replace('\\', "/")
}

fn skipped_warning(skipped: &[PathBuf], unreadable: usize) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(s) = formats::summarize_skipped(skipped) {
        out.push(format!("Im ZIP nicht übernommen: {s}"));
    }
    if unreadable > 0 {
        out.push(format!("Im ZIP nicht lesbar und weggelassen: {unreadable} Datei(en)"));
    }
    out
}

pub fn process_zip(input: &Path, output: &Path, mapping: &[(String, String)]) -> Result<FileStats, FormatError> {
    let tmp = TempDir::new()?;
    let mut archive = ZipArchive::new(std::fs::File::open(input)?)?;
    let mut writer = ZipWriter::new(std::fs::File::create(output)?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut stats = FileStats::default();
    let mut skipped: Vec<PathBuf> = Vec::new();
    let mut unreadable = 0usize;
    let mut used = HashSet::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if entry.is_dir() || is_junk(&name) {
            continue;
        }
        let path = Path::new(&name);
        let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|_| formats::is_supported(path)) else {
            skipped.push(PathBuf::from(&name));
            continue;
        };
        let tmp_in = tmp.0.join(format!("in-{i}.{ext}"));
        let tmp_out = tmp.0.join(format!("out-{i}.{ext}"));
        let mut buf = Vec::new();
        if entry.read_to_end(&mut buf).is_err() {
            unreadable += 1;
            continue;
        }
        drop(entry);
        std::fs::write(&tmp_in, &buf)?;
        match formats::process_file(&tmp_in, &tmp_out, mapping) {
            Ok(s) => {
                stats.merge(s);
                writer.start_file(output_entry_name(&name, mapping, &mut used), options)?;
                writer.write_all(&std::fs::read(&tmp_out)?)?;
            }
            Err(_) => unreadable += 1,
        }
        let _ = std::fs::remove_file(&tmp_in);
        let _ = std::fs::remove_file(&tmp_out);
    }
    writer.finish()?;
    stats.warnings.extend(skipped_warning(&skipped, unreadable));
    Ok(stats)
}

pub fn scan_fuzzy_zip(input: &Path, mapping: &[(String, String)]) -> Result<Vec<FuzzyHit>, FormatError> {
    let tmp = TempDir::new()?;
    let mut archive = ZipArchive::new(std::fs::File::open(input)?)?;
    let mut hits = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let path = Path::new(&name);
        if entry.is_dir() || is_junk(&name) || !formats::is_supported(path) {
            continue;
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut buf = Vec::new();
        if entry.read_to_end(&mut buf).is_err() {
            continue;
        }
        drop(entry);
        let tmp_in = tmp.0.join(format!("in-{i}.{ext}"));
        std::fs::write(&tmp_in, &buf)?;
        if let Ok(found) = formats::scan_fuzzy(&tmp_in, mapping) {
            for mut h in found {
                h.location = format!("{name} › {}", h.location);
                hits.push(h);
            }
        }
        let _ = std::fs::remove_file(&tmp_in);
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::office::tests::{make_zip, read_part};

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("nema-archive-test-{}-{name}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn moodle_download() {
        let inner = tmp("inner.pptx");
        make_zip(&inner, &[("ppt/slides/slide1.xml", "<p:sld><a:p><a:r><a:t>Anna Müller</a:t></a:r></a:p></p:sld>")]);
        let inner_bytes = std::fs::read(&inner).unwrap();

        let inp = tmp("in.zip");
        {
            let mut w = ZipWriter::new(std::fs::File::create(&inp).unwrap());
            let o = SimpleFileOptions::default();
            for (name, data) in [
                ("Anna Müller_123_assignsubmission_file_/Abgabe.txt", b"Von Anna M\xc3\xbcller".as_slice()),
                ("Anna Müller_123_assignsubmission_file_/Folien.pptx", &inner_bytes),
                ("Anna Müller_123_assignsubmission_file_/Foto Anna.png", b"PNG"),
                ("Anna Müller_123_assignsubmission_file_/alt.doc", b"DOC"),
                ("__MACOSX/Anna Müller_123_assignsubmission_file_/._Abgabe.txt", b"x"),
            ] {
                w.start_file(name, o).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        let out = tmp("out.zip");
        let mapping = vec![("Anna Müller".to_string(), "Lea Roth".to_string()), ("Anna".to_string(), "Lea".to_string())];
        let stats = process_zip(&inp, &out, &mapping).unwrap();
        assert_eq!(stats.replacements, 2);
        let mut a = ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = a.file_names().map(str::to_string).collect();
        assert_eq!(names, ["Lea Roth_123_assignsubmission_file_/Abgabe.txt", "Lea Roth_123_assignsubmission_file_/Folien.pptx"]);
        let mut txt = String::new();
        a.by_name(&names[0]).unwrap().read_to_string(&mut txt).unwrap();
        assert_eq!(txt, "Von Lea Roth");
        let inner_out = tmp("inner-out.pptx");
        let mut f = Vec::new();
        a.by_name(&names[1]).unwrap().read_to_end(&mut f).unwrap();
        std::fs::write(&inner_out, f).unwrap();
        assert!(read_part(&inner_out, "ppt/slides/slide1.xml").unwrap().contains("Lea Roth"));
        let w = stats.warnings.join(" ");
        assert!(w.contains("1 × .doc") && w.contains("1 × .png"), "{w}");
        assert!(!w.contains("Anna") && !w.contains("Müller"), "Warnung ohne Namen: {w}");

        let hits = scan_fuzzy_zip(&inp, &[("Anna Müler".to_string(), "Lea Roth".to_string())]).unwrap();
        assert!(hits.iter().any(|h| h.location.ends_with("Abgabe.txt › Zeile 1")), "{hits:?}");
        for f in [&inner, &inp, &out, &inner_out] {
            let _ = std::fs::remove_file(f);
        }
    }
}
