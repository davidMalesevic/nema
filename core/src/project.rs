//! Projekt (z.B. eine Klasse) mit Personen und Pseudonym-Zuordnung.
//! Wird als `ProjectVault` verschlüsselt auf Platte gespeichert.

use crate::person::Person;
use crate::pseudonym;
use crate::roster::Roster;
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
    /// Alle Personen des Projekts: die Lernenden der verknüpften Klassen
    /// (gleiche ID wie in der Klasse) plus einzeln hinzugefügte.
    #[serde(default)]
    pub persons: Vec<Person>,
    #[serde(default)]
    pub pseudonyms: Vec<PseudonymAssignment>,
    /// Verknüpfte Klassen aus der Lernenden-Verwaltung.
    #[serde(default)]
    pub class_ids: Vec<Uuid>,
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
            class_ids: Vec::new(),
        }
    }

    /// Gleicht das Projekt mit den verknüpften Klassen ab:
    /// neue Lernende kommen dazu, geänderte Angaben werden übernommen.
    /// Wer aus einer Klasse entfernt wurde (oder deren Klasse gelöscht wurde),
    /// bleibt als Einzelperson im Projekt, damit ältere Unterlagen weiterhin
    /// de-anonymisiert werden können. Rückgabe: true, wenn sich etwas geändert hat.
    pub fn sync_with_roster(&mut self, roster: &Roster) -> bool {
        let mut changed = false;
        let before = self.class_ids.len();
        self.class_ids.retain(|id| roster.class(*id).is_some());
        changed |= self.class_ids.len() != before;

        for class_id in &self.class_ids {
            let Some(class) = roster.class(*class_id) else { continue };
            for person in &class.persons {
                match self.persons.iter_mut().find(|p| p.id == person.id) {
                    Some(existing) if existing != person => {
                        *existing = person.clone();
                        changed = true;
                    }
                    Some(_) => {}
                    None => {
                        self.persons.push(person.clone());
                        changed = true;
                    }
                }
            }
        }
        changed |= self.ensure_pseudonyms();
        changed
    }

    /// Verknüpft eine Klasse und übernimmt ihre Lernenden.
    pub fn link_class(&mut self, class_id: Uuid, roster: &Roster) {
        if roster.class(class_id).is_some() && !self.class_ids.contains(&class_id) {
            self.class_ids.push(class_id);
        }
        self.sync_with_roster(roster);
    }

    /// Löst die Verknüpfung und entfernt die Lernenden dieser Klasse aus dem
    /// Projekt (ausser sie gehören zu einer anderen verknüpften Klasse).
    /// Die Pseudonym-Zuordnungen bleiben erhalten: wird die Klasse später
    /// wieder verknüpft, gelten dieselben Pseudonyme.
    pub fn unlink_class(&mut self, class_id: Uuid, roster: &Roster) {
        self.class_ids.retain(|id| *id != class_id);
        if let Some(class) = roster.class(class_id) {
            let keep: HashSet<Uuid> = self
                .class_ids
                .iter()
                .filter_map(|id| roster.class(*id))
                .flat_map(|c| c.persons.iter().map(|p| p.id))
                .collect();
            let gone: HashSet<Uuid> = class.persons.iter().map(|p| p.id).filter(|id| !keep.contains(id)).collect();
            self.persons.retain(|p| !gone.contains(&p.id));
        }
    }

    /// Name der verknüpften Klasse, zu der die Person gehört (sonst None = Einzelperson).
    pub fn class_of<'a>(&self, person_id: Uuid, roster: &'a Roster) -> Option<&'a str> {
        self.class_ids
            .iter()
            .filter_map(|id| roster.class(*id))
            .find(|c| c.persons.iter().any(|p| p.id == person_id))
            .map(|c| c.name.as_str())
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

    #[test]
    fn classes_sync_into_project() {
        let mut roster = Roster::default();
        let a = roster.add_class("INF 24a");
        roster.class_mut(a).unwrap().persons.push(Person::new("Anna", "Müller"));
        roster.class_mut(a).unwrap().persons.push(Person::new("Ben", "Schulz"));

        let mut v = ProjectVault::new("Modul 319");
        v.persons.push(Person::new("Gast", "Einzeln"));
        v.link_class(a, &roster);
        assert_eq!(v.persons.len(), 3);
        assert_eq!(v.pseudonyms.len(), 3);
        assert!(!v.sync_with_roster(&roster), "ohne Änderung kein erneuter Abgleich");
        let anna = roster.class(a).unwrap().persons[0].id;
        let pseudo = v.assignment_for(anna).unwrap().clone();
        assert_eq!(v.class_of(anna, &roster), Some("INF 24a"));

        // Neue Lernende und Korrekturen kommen im Projekt an, das Pseudonym bleibt
        roster.class_mut(a).unwrap().persons[0].last_name = "Mueller".into();
        roster.class_mut(a).unwrap().persons.push(Person::new("Cem", "Yilmaz"));
        assert!(v.sync_with_roster(&roster));
        assert_eq!(v.persons.len(), 4);
        assert_eq!(v.person(anna).unwrap().last_name, "Mueller");
        assert_eq!(v.assignment_for(anna), Some(&pseudo));

        // Aus der Klasse entfernt: bleibt als Einzelperson im Projekt
        roster.class_mut(a).unwrap().persons.retain(|p| p.id != anna);
        v.sync_with_roster(&roster);
        assert!(v.person(anna).is_some());
        assert_eq!(v.class_of(anna, &roster), None);

        // Verknüpfung lösen entfernt die Klassen-Lernenden, nicht die Einzelpersonen
        v.unlink_class(a, &roster);
        assert_eq!(v.persons.len(), 2);
        assert!(v.person(anna).is_some());

        // Wieder verknüpfen: gleiche Pseudonyme wie zuvor
        let ben = roster.class(a).unwrap().persons[0].id;
        let ben_pseudo = v.assignment_for(ben).unwrap().clone();
        v.link_class(a, &roster);
        assert_eq!(v.assignment_for(ben), Some(&ben_pseudo));

        // Klasse gelöscht: Lernende bleiben, Verknüpfung verschwindet
        roster.remove_class(a);
        assert!(v.sync_with_roster(&roster));
        assert!(v.class_ids.is_empty());
        assert_eq!(v.persons.len(), 4);
    }

    #[test]
    fn old_project_files_load_without_classes() {
        let mut v = ProjectVault::new("Alt");
        v.persons.push(Person::new("Anna", "Müller"));
        let mut json = serde_json::to_value(&v).unwrap();
        json.as_object_mut().unwrap().remove("class_ids");
        let loaded: ProjectVault = serde_json::from_value(json).unwrap();
        assert!(loaded.class_ids.is_empty());
        assert_eq!(loaded.persons.len(), 1);
    }
}
