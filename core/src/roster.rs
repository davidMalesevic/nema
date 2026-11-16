//! Lernenden-Verwaltung: Klassen mit Lernenden, unabhängig von Projekten.
//!
//! Die Klassen werden einmal gepflegt (`roster.nema`, verschlüsselt) und pro
//! Projekt verknüpft (`ProjectVault::class_ids`). Das Projekt übernimmt die
//! Lernenden mit derselben Personen-ID; die Pseudonyme bleiben pro Projekt.

use crate::person::Person;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchoolClass {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub persons: Vec<Person>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roster {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub classes: Vec<SchoolClass>,
}

impl Roster {
    pub fn class(&self, id: Uuid) -> Option<&SchoolClass> {
        self.classes.iter().find(|c| c.id == id)
    }

    pub fn class_mut(&mut self, id: Uuid) -> Option<&mut SchoolClass> {
        self.classes.iter_mut().find(|c| c.id == id)
    }

    /// Legt eine leere Klasse an und gibt ihre ID zurück.
    pub fn add_class(&mut self, name: impl Into<String>) -> Uuid {
        self.add_class_with(name, Vec::new())
    }

    /// Legt eine Klasse mit bestehenden Personen an (IDs bleiben erhalten,
    /// damit Projekte ihre Pseudonyme behalten).
    pub fn add_class_with(&mut self, name: impl Into<String>, persons: Vec<Person>) -> Uuid {
        let id = Uuid::new_v4();
        self.classes.push(SchoolClass { id, name: name.into(), created_at: Utc::now(), persons });
        id
    }

    pub fn remove_class(&mut self, id: Uuid) {
        self.classes.retain(|c| c.id != id);
    }
}
