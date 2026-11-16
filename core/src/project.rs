//! Projekt (z.B. eine Klasse) mit Personen und Pseudonym-Zuordnung.
//! Wird als `ProjectVault` verschlüsselt auf Platte gespeichert.

use crate::person::Person;
use crate::pseudonym;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PseudonymAssignment {
    pub person_id: Uuid,
    pub first_name: String,
    pub last_name: String,
}

impl PseudonymAssignment {
    pub fn full(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
    pub fn last_first(&self) -> String {
        format!("{}, {}", self.last_name, self.first_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectVault {
    pub version: u32,
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    /// Hex-kodierter 32-Byte-Seed für die deterministische Pseudonym-Vergabe.
    pub seed: String,
    #[serde(default)]
    pub persons: Vec<Person>,
    #[serde(default)]
    pub pseudonyms: Vec<PseudonymAssignment>,
}

impl ProjectVault {
    pub fn new(name: impl Into<String>) -> Self {
        let mut seed = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
        Self {
            version: 1,
            id: Uuid::new_v4(),
            name: name.into(),
            created_at: Utc::now(),
            seed: hex::encode(seed),
            persons: Vec::new(),
            pseudonyms: Vec::new(),
        }
    }

    /// Vergibt für alle Personen ohne Zuordnung ein Pseudonym.
    /// Rückgabe: true, wenn sich etwas geändert hat.
    pub fn ensure_pseudonyms(&mut self) -> bool {
        let mut changed = false;
        let mut taken: HashSet<(String, String)> = self
            .pseudonyms
            .iter()
            .map(|p| (p.first_name.clone(), p.last_name.clone()))
            .collect();
        for person in &self.persons {
            if self.pseudonyms.iter().any(|p| p.person_id == person.id) {
                continue;
            }
            let (first, last) = pseudonym::generate(&self.seed, person.id, &taken);
            taken.insert((first.clone(), last.clone()));
            self.pseudonyms.push(PseudonymAssignment {
                person_id: person.id,
                first_name: first,
                last_name: last,
            });
            changed = true;
        }
        changed
    }

    pub fn assignment_for(&self, person_id: Uuid) -> Option<&PseudonymAssignment> {
        self.pseudonyms.iter().find(|p| p.person_id == person_id)
    }

    pub fn person(&self, id: Uuid) -> Option<&Person> {
        self.persons.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudonyms_stable_and_complete() {
        let mut v = ProjectVault::new("Mathe 10a");
        v.persons.push(Person::new("Anna", "Müller"));
        v.persons.push(Person::new("Ben", "Schulz"));
        assert!(v.ensure_pseudonyms());
        assert!(!v.ensure_pseudonyms(), "beim zweiten Lauf keine Änderung");
        assert_eq!(v.pseudonyms.len(), 2);

        // deterministisch: frische Vault mit gleichem Seed erzeugt gleiche Pseudonyme
        let mut v2 = ProjectVault::new("Kopie");
        v2.seed = v.seed.clone();
        v2.persons = v.persons.clone();
        v2.ensure_pseudonyms();
        assert_eq!(v.pseudonyms, v2.pseudonyms);
    }
}
