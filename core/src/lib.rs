//! nema-core — Kernlogik des Anonymisierungstools.
//!
//! Alles hier ist GUI-unabhängig und headless testbar:
//! - `crypto`: Masterkey, OS-Schlüsselbund, AES-256-GCM, Argon2id-Fallback
//! - `store`: verschlüsselter Tresor für Projekte und Namenslisten
//! - `project` / `person`: Datenmodell
//! - `pseudonym`: deterministische Pseudonym-Generierung pro Projekt
//! - `roster`: Klassen mit Lernenden, projektübergreifend
//! - `csv_import`: Import von Namenslisten
//! - `formats`: zentrale Format-Tabelle (DOCX, PDF, Office, Text, ZIP)

pub mod archive;
pub mod crypto;
pub mod csv_import;
pub mod docx;
pub mod formats;
pub mod fuzzy;
pub mod mapping;
pub(crate) mod matcher;
pub mod office;
pub mod pdf;
pub mod person;
pub mod project;
pub mod pseudonym;
pub mod roster;
pub mod runs;
pub mod store;
pub mod textfile;
pub(crate) mod textview;

pub use person::Person;
pub use project::{ProjectMeta, ProjectVault, PseudonymAssignment};
pub use store::{KeyMode, StoreError, VaultStore};
pub use uuid;
