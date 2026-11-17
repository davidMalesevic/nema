//! Der verschlüsselte Tresor (`VaultStore`).
//!
//! Ablage im App-Datenverzeichnis (z.B. `~/.local/share/nema`):
//! ```text
//! <dir>/
//!   key.meta            Klartext-Metadaten: Modus, Salt, Verifier
//!   index.nema          verschlüsselte Liste aller Projekte (ProjectMeta)
//!   projects/<id>.nema  verschlüsselte ProjectVault je Projekt
//!   roster.nema         verschlüsselte Lernenden-Verwaltung (Klassen)
//! ```
//!
//! Der Masterkey liegt im OS-Schlüsselbund (Modus `Keychain`) oder wird aus
//! einem Master-Passwort abgeleitet (Modus `Password`, Fallback wenn kein
//! Schlüsselbund verfügbar ist).

use crate::crypto::{self, CryptoError, MasterKey};
use crate::project::{ProjectMeta, ProjectVault};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const KEY_META: &str = "key.meta";
const INDEX: &str = "index.nema";
const PROJECTS_DIR: &str = "projects";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Projekt nicht gefunden: {0}")]
    NotFound(String),
    #[error("Passwort falsch")]
    WrongPassword,
    #[error("ungültiger Zustand: {0}")]
    InvalidState(String),
    #[error("Schlüssel im Schlüsselbund passt nicht zu diesem Tresor")]
    KeychainMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyMode {
    Keychain,
    Password,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyMeta {
    mode: KeyMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    salt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verifier: Option<String>,
    /// Masterkey, mit dem Backup-Passwort (Argon2id-KEK) verschlüsselt —
    /// Fallback, wenn der OS-Schlüsselbund nicht verfügbar ist.
    #[serde(skip_serializing_if = "Option::is_none")]
    wrapped_key: Option<String>,
}

#[derive(Clone)]
pub struct VaultStore {
    dir: PathBuf,
    key: MasterKey,
    mode: KeyMode,
}

impl std::fmt::Debug for VaultStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Masterkey nie ausgeben
        f.debug_struct("VaultStore")
            .field("dir", &self.dir)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl VaultStore {
    pub fn default_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nema")
    }

    pub fn is_initialized(dir: &Path) -> bool {
        dir.join(KEY_META).exists()
    }

    /// Legt einen neuen Tresor an.
    ///
    /// Der Masterkey ist immer zufällig. Er wird im OS-Schlüsselbund abgelegt
    /// (wenn `use_keychain` und der Schlüsselbund einen Roundtrip schafft) UND
    /// zusätzlich mit dem Backup-Passwort verschlüsselt in `key.meta` gewrappt —
    /// damit der Tresor auch nach einem Neustart ohne Schlüsselbund (z.B.
    /// gesperrtes Login-Keyring) per Passwort entsperrt werden kann.
    pub fn init(dir: &Path, use_keychain: bool, password: Option<&str>) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir)?;
        std::fs::create_dir_all(dir.join(PROJECTS_DIR))?;

        let password = password.ok_or_else(|| {
            StoreError::InvalidState("Backup-Passwort erforderlich".into())
        })?;
        if password.len() < 8 {
            return Err(StoreError::InvalidState(
                "Passwort muss mindestens 8 Zeichen haben".into(),
            ));
        }

        let salt = crypto::new_salt();
        let kek = crypto::derive_key_from_password(password, &salt);
        let key = MasterKey::random();
        let verifier = crypto::make_verifier(&key);
        let wrapped_key = hex::encode(crypto::wrap_key(&kek, &key));

        let mode = if use_keychain && crypto::keychain_backend_available() && crypto::keychain_roundtrip_ok(&key) {
            KeyMode::Keychain
        } else {
            // Schlüsselbund nicht verfügbar oder nicht persistierend → Passwort-Modus.
            // Nur aufräumen, wenn wir oben selbst geschrieben haben — sonst würde
            // ein Tresor ohne Schlüsselbund (Tests, Screenshots) den Key des
            // echten Tresors löschen.
            if use_keychain {
                let _ = crypto::keychain_delete_key();
            }
            KeyMode::Password
        };

        let meta = KeyMeta {
            mode,
            salt: Some(crypto::salt_to_b64(&salt)),
            verifier: Some(verifier),
            wrapped_key: Some(wrapped_key),
        };
        std::fs::write(dir.join(KEY_META), serde_json::to_vec_pretty(&meta)?)?;
        let store = Self { dir: dir.to_path_buf(), key, mode };
        store.save_index(&[])?;
        Ok(store)
    }

    /// Entsperrt den Tresor ohne Passwort — nur erfolgreich, wenn der Modus
    /// `Keychain` ist und der Schlüsselbund den Key liefern kann.
    pub fn unlock(dir: &Path) -> Result<Self, StoreError> {
        let meta = Self::read_key_meta(dir)?;
        match meta.mode {
            KeyMode::Keychain => {
                let key = crypto::keychain_load_key()?;
                // Fremder Key im Schlüsselbund (z.B. überschrieben) → nicht
                // damit weiterarbeiten, sondern Passwort verlangen.
                if let Some(v) = &meta.verifier {
                    if !crypto::check_verifier(&key, v) {
                        return Err(StoreError::KeychainMismatch);
                    }
                }
                Ok(Self { dir: dir.to_path_buf(), key, mode: KeyMode::Keychain })
            }
            KeyMode::Password => Err(StoreError::InvalidState(
                "Passwort erforderlich (kein Schlüsselbund aktiv)".into(),
            )),
        }
    }

    /// Entsperrt den Tresor mit dem Backup-Passwort (funktioniert in beiden Modi:
    /// der Masterkey ist mit dem Passwort-KEK gewrappt in key.meta abgelegt).
    pub fn unlock_with_password(dir: &Path, password: &str) -> Result<Self, StoreError> {
        let meta = Self::read_key_meta(dir)?;
        let salt_b64 = meta
            .salt
            .ok_or_else(|| StoreError::InvalidState("kein Salt in key.meta".into()))?;
        let salt = crypto::salt_from_b64(&salt_b64)?;
        let kek = crypto::derive_key_from_password(password, &salt);
        let key = match &meta.wrapped_key {
            Some(w) => {
                let data = hex::decode(w).map_err(|_| StoreError::InvalidState("wrapped_key ungültig".into()))?;
                crypto::unwrap_key(&kek, &data).map_err(|_| StoreError::WrongPassword)?
            }
            None => crypto::derive_key_from_password(password, &salt),
        };
        if let Some(v) = &meta.verifier {
            if !crypto::check_verifier(&key, v) {
                return Err(StoreError::WrongPassword);
            }
        }
        let store = Self { dir: dir.to_path_buf(), key, mode: meta.mode };
        // Migration: Tresor im Keychain-Modus, aber der Key fehlt im
        // Schlüsselbund (z.B. kaputtes Backend) oder ist ein fremder Key →
        // richtigen Key zurückspeichern. Dateien, die zwischenzeitlich mit dem
        // fremden Key verschlüsselt wurden, vorher auf den richtigen umschlüsseln,
        // sonst wären sie nach dem Überschreiben verloren.
        if meta.mode == KeyMode::Keychain {
            let healthy = match crypto::keychain_load_key() {
                Ok(stale) if stale.to_hex() != store.key.to_hex() => store.reseal_from(&stale).is_ok(),
                _ => true,
            };
            if healthy {
                let _ = crypto::keychain_roundtrip_ok(&store.key);
            }
        }
        Ok(store)
    }

    /// Alle Tresor-Dateien, die sich nicht mit dem Masterkey, wohl aber mit
    /// `stale` öffnen lassen, mit dem Masterkey neu verschlüsseln.
    fn reseal_from(&self, stale: &MasterKey) -> Result<usize, StoreError> {
        let mut files = vec![self.index_path(), self.roster_path(), self.runs_path()];
        if let Ok(rd) = std::fs::read_dir(self.dir.join(PROJECTS_DIR)) {
            files.extend(rd.filter_map(|e| e.ok()).map(|e| e.path()));
        }
        let mut n = 0;
        for f in files.iter().filter(|f| f.is_file()) {
            let data = std::fs::read(f)?;
            if crypto::open_json::<serde_json::Value>(&self.key, &data).is_ok() {
                continue;
            }
            if let Ok(v) = crypto::open_json::<serde_json::Value>(stale, &data) {
                std::fs::write(f, crypto::seal_json(&self.key, &v)?)?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn read_key_meta(dir: &Path) -> Result<KeyMeta, StoreError> {
        let raw = std::fs::read(dir.join(KEY_META))?;
        Ok(serde_json::from_slice(&raw)?)
    }

    pub fn mode(&self) -> KeyMode {
        self.mode
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join(INDEX)
    }

    fn project_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(PROJECTS_DIR).join(format!("{id}.nema"))
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectMeta>, StoreError> {
        let data = std::fs::read(self.index_path())?;
        Ok(crypto::open_json(&self.key, &data)?)
    }

    fn save_index(&self, list: &[ProjectMeta]) -> Result<(), StoreError> {
        std::fs::write(self.index_path(), crypto::seal_json(&self.key, list)?)?;
        Ok(())
    }

    pub fn create_project(&self, name: &str) -> Result<ProjectVault, StoreError> {
        // Index zuerst lesen: schlägt das fehl, bleibt keine verwaiste Projektdatei zurück.
        let mut list = self.list_projects()?;
        let vault = ProjectVault::new(name);
        self.save_project(&vault)?;
        list.push(ProjectMeta {
            id: vault.id,
            name: vault.name.clone(),
            created_at: vault.created_at,
        });
        self.save_index(&list)?;
        Ok(vault)
    }

    pub fn save_project(&self, vault: &ProjectVault) -> Result<(), StoreError> {
        std::fs::write(self.project_path(vault.id), crypto::seal_json(&self.key, vault)?)?;
        Ok(())
    }

    pub fn load_project(&self, id: Uuid) -> Result<ProjectVault, StoreError> {
        let data = std::fs::read(self.project_path(id))
            .map_err(|_| StoreError::NotFound(id.to_string()))?;
        Ok(crypto::open_json(&self.key, &data)?)
    }

    pub fn delete_project(&self, id: Uuid) -> Result<(), StoreError> {
        let _ = std::fs::remove_file(self.project_path(id));
        let mut list = self.list_projects()?;
        list.retain(|m| m.id != id);
        self.save_index(&list)?;
        Ok(())
    }

    // --- Lernenden-Verwaltung (Klassen) ---

    fn roster_path(&self) -> PathBuf {
        self.dir.join("roster.nema")
    }

    /// Lädt die Klassen. Fehlt die Datei (älterer Tresor), ist die Verwaltung leer.
    pub fn load_roster(&self) -> Result<crate::roster::Roster, StoreError> {
        let p = self.roster_path();
        if !p.exists() {
            return Ok(crate::roster::Roster::default());
        }
        let data = std::fs::read(p)?;
        Ok(crypto::open_json(&self.key, &data)?)
    }

    pub fn save_roster(&self, roster: &crate::roster::Roster) -> Result<(), StoreError> {
        std::fs::write(self.roster_path(), crypto::seal_json(&self.key, roster)?)?;
        Ok(())
    }

    // --- Audit-Log (Läufe) ---

    fn runs_path(&self) -> PathBuf {
        self.dir.join("runs.nema")
    }

    /// Hängt einen Lauf an das verschlüsselte Audit-Log an.
    pub fn log_run(&self, run: &crate::runs::RunRecord) -> Result<(), StoreError> {
        let mut runs = self.list_runs()?;
        runs.push(run.clone());
        std::fs::write(self.runs_path(), crypto::seal_json(&self.key, &runs)?)?;
        Ok(())
    }

    /// Liest das Audit-Log (entschlüsselt). Enthält nur Pseudonyme, nie Klarnamen.
    pub fn list_runs(&self) -> Result<Vec<crate::runs::RunRecord>, StoreError> {
        let p = self.runs_path();
        if !p.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read(p)?;
        Ok(crypto::open_json(&self.key, &data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::person::Person;

    fn tmpdir() -> PathBuf {
        std::env::temp_dir().join(format!("nema-core-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn create_save_load_roundtrip() {
        let dir = tmpdir();
        let store = VaultStore::init(&dir, false, Some("passwort-123")).unwrap();
        let mut vault = store.create_project("Mathe 10a").unwrap();
        vault.persons.push(Person::new("Anna", "Müller"));
        vault.persons.push(Person::new("Ben", "Schulz"));
        vault.ensure_pseudonyms();
        store.save_project(&vault).unwrap();
        let id = vault.id;
        drop(store);

        let store2 = VaultStore::unlock_with_password(&dir, "passwort-123").unwrap();
        let list = store2.list_projects().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Mathe 10a");

        let loaded = store2.load_project(id).unwrap();
        assert_eq!(loaded.persons.len(), 2);
        assert_eq!(loaded.pseudonyms.len(), 2);

        // Pseudonyme deterministisch: nach Neuvergabe identisch
        let mut again = loaded.clone();
        again.pseudonyms.clear();
        again.ensure_pseudonyms();
        assert_eq!(again.pseudonyms, loaded.pseudonyms);

        // Auf Platte steht kein Klarname
        let raw = std::fs::read(dir.join(PROJECTS_DIR).join(format!("{id}.nema"))).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("Müller"));
        let raw_index = std::fs::read(dir.join(INDEX)).unwrap();
        // Projektname ist im Index verschlüsselt, aber Klartext-Metadaten enthalten ihn nicht zwingend
        assert!(!String::from_utf8_lossy(&raw_index).contains("Müller"));

        // Löschen
        store2.delete_project(id).unwrap();
        assert!(store2.list_projects().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_password_rejected() {
        let dir = tmpdir();
        let _store = VaultStore::init(&dir, false, Some("richtig-123")).unwrap();
        let err = VaultStore::unlock_with_password(&dir, "falsch-999").unwrap_err();
        assert!(matches!(err, StoreError::WrongPassword));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn short_password_rejected() {
        let dir = tmpdir();
        let err = VaultStore::init(&dir, false, Some("kurz")).unwrap_err();
        assert!(matches!(err, StoreError::InvalidState(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn password_required_without_keychain_flag_mismatch() {
        let dir = tmpdir();
        // ohne Passwort → Fehler
        let err = VaultStore::init(&dir, false, None).unwrap_err();
        assert!(matches!(err, StoreError::InvalidState(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unlock_by_password_works_in_both_modes() {
        let dir = tmpdir();
        // use_keychain=true: Modus ist Keychain (gesunder Schlüsselbund) oder
        // Password (Schlüsselbund nicht persistierend) — Passwort-Unlock muss
        // in beiden Fällen funktionieren.
        let store = VaultStore::init(&dir, true, Some("passwort-123")).unwrap();
        let mode = store.mode();
        drop(store);
        let s2 = VaultStore::unlock_with_password(&dir, "passwort-123").unwrap();
        assert_eq!(s2.mode(), mode);
        assert!(s2.list_projects().is_ok());
        assert!(matches!(
            VaultStore::unlock_with_password(&dir, "falsch-999"),
            Err(StoreError::WrongPassword)
        ));

        // Fremder Key im Schlüsselbund (im selben Test, weil sich die Tests
        // den Test-Eintrag im Schlüsselbund teilen).
        if mode == KeyMode::Keychain {
            let stale = MasterKey::random();
            crypto::keychain_save_key(&stale).unwrap();
            assert!(matches!(VaultStore::unlock(&dir), Err(StoreError::KeychainMismatch)));

            // Mit dem fremden Key geschriebene Klassen …
            let wrong = VaultStore { dir: dir.clone(), key: stale, mode };
            let mut roster = crate::roster::Roster::default();
            roster.add_class("INF 24a");
            wrong.save_roster(&roster).unwrap();

            // … werden beim Passwort-Unlock umgeschlüsselt, der Schlüsselbund repariert.
            let s3 = VaultStore::unlock_with_password(&dir, "passwort-123").unwrap();
            assert_eq!(s3.load_roster().unwrap().classes.len(), 1);
            let s4 = VaultStore::unlock(&dir).unwrap();
            assert!(s4.list_projects().is_ok());
            assert_eq!(s4.load_roster().unwrap().classes.len(), 1);
        }
        let _ = crypto::keychain_delete_key();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roster_roundtrip_and_missing_file() {
        let dir = tmpdir();
        let store = VaultStore::init(&dir, false, Some("passwort-123")).unwrap();
        assert!(store.load_roster().unwrap().classes.is_empty());
        let mut roster = store.load_roster().unwrap();
        let id = roster.add_class("INF 24a");
        roster.class_mut(id).unwrap().persons.push(Person::new("Anna", "Müller"));
        store.save_roster(&roster).unwrap();
        let loaded = store.load_roster().unwrap();
        assert_eq!(loaded.classes.len(), 1);
        assert_eq!(loaded.class(id).unwrap().persons[0].last_name, "Müller");
        // Klarnamen liegen nicht im Klartext auf der Platte
        let raw = std::fs::read(dir.join("roster.nema")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("Müller"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
