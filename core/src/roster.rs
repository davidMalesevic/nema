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

/// Zusätzlicher Begriff, der in allen Projekten ersetzt wird
/// (z.B. Schulname, Ort, Name der Lehrperson).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraTerm {
    pub id: Uuid,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roster {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub classes: Vec<SchoolClass>,
    #[serde(default)]
    pub terms: Vec<ExtraTerm>,
}

/// Kürzeste Länge, ab der ein Klassenname automatisch ersetzt wird. Kürzere Namen
/// («3b», «10a») kämen als Teil anderer Wörter vor («Aufgabe 3b») und bleiben stehen.
pub const MIN_CLASS_NAME_LEN: usize = 4;
/// Kürzeste Länge eines zusätzlichen Begriffs.
pub const MIN_TERM_LEN: usize = 3;

fn short_code(id: Uuid) -> String {
    let b = id.as_bytes();
    format!("{:02X}{:02X}{:02X}", b[0], b[1], b[2])
}

/// Platzhalter für einen Klassennamen in anonymisierten Unterlagen.
pub fn class_placeholder(class_id: Uuid) -> String {
    format!("KL-{}", short_code(class_id))
}

/// Platzhalter für einen zusätzlichen Begriff.
pub fn term_placeholder(term_id: Uuid) -> String {
    format!("BG-{}", short_code(term_id))
}

/// Wird dieser Klassenname in Unterlagen ersetzt?
pub fn class_name_is_replaced(name: &str) -> bool {
    name.trim().chars().count() >= MIN_CLASS_NAME_LEN
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

    /// Fügt einen Begriff hinzu. None, wenn er zu kurz ist oder schon existiert.
    pub fn add_term(&mut self, text: &str) -> Option<Uuid> {
        let text = text.trim();
        if text.chars().count() < MIN_TERM_LEN || self.terms.iter().any(|t| t.text.eq_ignore_ascii_case(text)) {
            return None;
        }
        let id = Uuid::new_v4();
        self.terms.push(ExtraTerm { id, text: text.to_string() });
        Some(id)
    }

    pub fn remove_term(&mut self, id: Uuid) {
        self.terms.retain(|t| t.id != id);
    }
}
