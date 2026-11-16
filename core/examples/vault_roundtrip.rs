//! Headless-Beweis, dass der Tresor funktioniert:
//! Tresor anlegen (Passwort-Modus) → Projekt mit Personen anlegen → speichern →
//! neu laden → Pseudonyme prüfen → falsches Passwort wird abgelehnt →
//! Klarnamen stehen nicht im Klartext auf Platte.
//!
//! Aufruf: `cargo run -p nema-core --example vault_roundtrip`

use nema_core::person::Person;
use nema_core::store::VaultStore;
use nema_core::uuid::Uuid;

fn main() {
    let dir = std::env::temp_dir().join(format!("nema-roundtrip-{}", Uuid::new_v4()));
    println!("Tresor-Verzeichnis: {}", dir.display());

    let store = VaultStore::init(&dir, false, Some("test-passwort-123")).expect("init");
    let mut vault = store.create_project("Mathe 10a").expect("projekt anlegen");
    vault.persons.push(Person::new("Anna", "Müller"));
    vault.persons.push(Person::new("Ben", "Schulz"));
    vault.ensure_pseudonyms();
    store.save_project(&vault).expect("speichern");
    let id = vault.id;
    println!("Projekt angelegt: {} ({})", vault.name, id);
    drop(store);

    let store2 = VaultStore::unlock_with_password(&dir, "test-passwort-123").expect("unlock");
    let projects = store2.list_projects().expect("liste");
    assert_eq!(projects.len(), 1);
    let loaded = store2.load_project(id).expect("laden");
    assert_eq!(loaded.persons.len(), 2);
    assert_eq!(loaded.pseudonyms.len(), 2);
    for a in &loaded.pseudonyms {
        let p = loaded.person(a.person_id).unwrap();
        println!("  {} {} -> {} {}", p.first_name, p.last_name, a.first_name, a.last_name);
    }

    assert!(VaultStore::unlock_with_password(&dir, "falsches-passwort").is_err());
    println!("Falsches Passwort wird abgelehnt.");

    let raw = std::fs::read(dir.join("projects").join(format!("{id}.nema"))).unwrap();
    let text = String::from_utf8_lossy(&raw);
    assert!(!text.contains("Müller"), "Klarname darf nicht im Klartext auf Platte liegen");
    println!("Verschlüsselung auf Platte geprüft: kein Klarname im Klartext.");

    std::fs::remove_dir_all(&dir).ok();
    println!("OK — Roundtrip erfolgreich.");
}
