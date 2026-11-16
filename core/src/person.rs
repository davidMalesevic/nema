//! Ein Lernender in einem Projekt.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Person {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    #[serde(default)]
    pub emails: Vec<String>,
    #[serde(default)]
    pub matriculation: Option<String>,
    #[serde(default)]
    pub note: String,
}

impl Person {
    pub fn new(first_name: impl Into<String>, last_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            first_name: first_name.into(),
            last_name: last_name.into(),
            emails: Vec::new(),
            matriculation: None,
            note: String::new(),
        }
    }

    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name).trim().to_string()
    }

    pub fn last_first(&self) -> String {
        if self.first_name.is_empty() {
            self.last_name.clone()
        } else {
            format!("{}, {}", self.last_name, self.first_name)
        }
    }

    /// Alle Schreibweisen, unter denen diese Person in einem Dokument vorkommen
    /// kann. Der Matcher (M2) baut daraus die Suchmuster.
    pub fn identifiers(&self) -> Vec<String> {
        let mut v = vec![self.full_name(), self.last_first(), self.last_name.clone()];
        if !self.first_name.trim().is_empty() {
            v.push(self.first_name.clone());
        }
        for e in &self.emails {
            v.push(e.clone());
            if let Some(local) = e.split('@').next() {
                if !local.is_empty() {
                    v.push(local.to_string());
                }
            }
        }
        if let Some(m) = &self.matriculation {
            if !m.trim().is_empty() {
                v.push(m.clone());
            }
        }
        v.retain(|s| !s.trim().is_empty());
        v.sort();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_cover_variants() {
        let mut p = Person::new("Anna", "Müller");
        p.emails.push("anna.mueller@schule.ch".into());
        p.matriculation = Some("M-2024-017".into());
        let ids = p.identifiers();
        assert!(ids.contains(&"Anna Müller".to_string()));
        assert!(ids.contains(&"Müller, Anna".to_string()));
        assert!(ids.contains(&"Müller".to_string()));
        assert!(ids.contains(&"Anna".to_string()));
        assert!(ids.contains(&"anna.mueller@schule.ch".to_string()));
        assert!(ids.contains(&"anna.mueller".to_string()));
        assert!(ids.contains(&"M-2024-017".to_string()));
    }
}
