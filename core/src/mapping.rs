//! Aufbau der Ersetzungsliste: Echtname-Varianten → Pseudonym (und zurück).
//!
//! Pro Person werden alle Schreibweisen (Vollname, "Nachname, Vorname",
//! Vorname, Nachname, E-Mail, lokaler E-Mail-Teil, Matrikelnummer) auf ein
//! Pseudonym abgebildet. Das Pseudonym ist pro Projekt konsistent.

use crate::project::ProjectVault;
use std::collections::HashMap;
use uuid::Uuid;

fn clean_name(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub fn pseudo_email_local(first: &str, last: &str) -> String {
    format!("{}.{}", clean_name(first), clean_name(last))
}

pub fn pseudo_email(first: &str, last: &str) -> String {
    format!("{}@anonym.invalid", pseudo_email_local(first, last))
}

pub fn pseudo_matriculation(person_id: Uuid) -> String {
    let b = person_id.as_bytes();
    let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    format!("PN-{n:08X}")
}

/// Vorwärts-Mapping (Anonymisierung): (echter Bezeichner, Pseudonym).
/// Kollidierende Bezeichner (z.B. gleicher Vorname bei zwei Personen) werden
/// übersprungen — sie landen später in der Fuzzy-/Review-Liste (M4).
pub fn anonymize_mapping(vault: &ProjectVault) -> Vec<(String, String)> {
    // key: lowercased für Kollisionserkennung; value: (Original, Ersetzung)
    let mut map: HashMap<String, (String, String)> = HashMap::new();
    let mut ambiguous: Vec<String> = Vec::new();

    for p in &vault.persons {
        let Some(a) = vault.assignment_for(p.id) else { continue };
        let p_email = pseudo_email(&a.first_name, &a.last_name);
        let p_local = pseudo_email_local(&a.first_name, &a.last_name);
        let p_matr = pseudo_matriculation(p.id);

        for id in p.identifiers() {
            let repl = if id.contains('@') {
                p_email.clone()
            } else if p.matriculation.as_deref() == Some(id.as_str()) {
                p_matr.clone()
            } else if id == p.first_name {
                a.first_name.clone()
            } else if id == p.last_name {
                a.last_name.clone()
            } else if id == p.full_name() {
                a.full()
            } else if id == p.last_first() {
                a.last_first()
            } else if p.emails.iter().any(|e| {
                e.split('@').next().map(|l| l == id).unwrap_or(false)
            }) {
                p_local.clone()
            } else {
                a.full()
            };
            let key = id.to_lowercase();
            match map.get(&key) {
                Some((_, existing)) if *existing != repl => ambiguous.push(key),
                Some(_) => {}
                None => {
                    map.insert(key, (id, repl));
                }
            }
        }
    }

    let mut out: Vec<(String, String)> = map
        .into_values()
        .filter(|(orig, _)| !ambiguous.contains(&orig.to_lowercase()))
        .collect();
    out.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    out
}

/// Rückwärts-Mapping (De-Anonymisierung): (Pseudonym, Echtname).
/// Pseudonyme, die mehreren Personen zugeordnet wären, werden ausgelassen.
pub fn deanonymize_mapping(vault: &ProjectVault) -> Vec<(String, String)> {
    let forward = anonymize_mapping(vault);
    // Kollisionserkennung über kleingeschriebene Pseudonyme, aber Originalschreibweise
    // des Klarnamens als Zielwert behalten
    let mut seen: HashMap<String, String> = HashMap::new(); // pseudo(lower) -> real(lower)
    let mut ambiguous: Vec<String> = Vec::new();
    for (real, pseudo) in &forward {
        let key = pseudo.to_lowercase();
        match seen.get(&key) {
            Some(existing) if *existing != real.to_lowercase() => ambiguous.push(key),
            Some(_) => {}
            None => {
                seen.insert(key, real.to_lowercase());
            }
        }
    }
    let mut out: Vec<(String, String)> = forward
        .into_iter()
        .filter(|(_, pseudo)| !ambiguous.contains(&pseudo.to_lowercase()))
        .map(|(real, pseudo)| (pseudo, real))
        .collect();
    out.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::person::Person;

    fn vault_with_person() -> ProjectVault {
        let mut v = ProjectVault::new("Test");
        let mut p = Person::new("Anna", "Müller");
        p.emails.push("anna.mueller@schule.ch".into());
        p.matriculation = Some("M-2024-017".into());
        v.persons.push(p);
        v.ensure_pseudonyms();
        v
    }

    #[test]
    fn forward_mapping_covers_all_variants() {
        let v = vault_with_person();
        let m = anonymize_mapping(&v);
        let a = v.assignment_for(v.persons[0].id).unwrap();
        let get = |needle: &str| m.iter().find(|(k, _)| k == needle).map(|(_, v)| v.clone());
        let full = a.full();
        let last_first = a.last_first();
        let email = pseudo_email(&a.first_name, &a.last_name);
        let local = pseudo_email_local(&a.first_name, &a.last_name);
        let matr = pseudo_matriculation(v.persons[0].id);
        assert_eq!(get("Anna Müller").as_deref(), Some(full.as_str()));
        assert_eq!(get("Müller, Anna").as_deref(), Some(last_first.as_str()));
        assert_eq!(get("Anna").as_deref(), Some(a.first_name.as_str()));
        assert_eq!(get("Müller").as_deref(), Some(a.last_name.as_str()));
        assert_eq!(get("anna.mueller@schule.ch").as_deref(), Some(email.as_str()));
        assert_eq!(get("anna.mueller").as_deref(), Some(local.as_str()));
        assert_eq!(get("M-2024-017").as_deref(), Some(matr.as_str()));
    }

    #[test]
    fn reverse_mapping_inverts() {
        let v = vault_with_person();
        let fwd = anonymize_mapping(&v);
        let rev = deanonymize_mapping(&v);
        assert_eq!(fwd.len(), rev.len());
        for (real, pseudo) in &fwd {
            assert!(rev.iter().any(|(p, r)| p == pseudo && r == real));
        }
    }

    #[test]
    fn duplicate_first_name_is_ambiguous_and_skipped() {
        let mut v = ProjectVault::new("Test");
        v.persons.push(Person::new("Anna", "Müller"));
        v.persons.push(Person::new("Anna", "Schulz"));
        v.ensure_pseudonyms();
        let m = anonymize_mapping(&v);
        // "anna" allein darf nicht eindeutig gemappt werden
        assert!(!m.iter().any(|(k, _)| k == "Anna"));
        // volle Namen schon
        assert!(m.iter().any(|(k, _)| k == "Anna Müller"));
        assert!(m.iter().any(|(k, _)| k == "Anna Schulz"));
    }
}
