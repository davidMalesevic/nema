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

fn sort_longest_first(list: &mut [(String, String)]) {
    list.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
}

/// Vorwärts-Mapping (Anonymisierung): Personen plus Begriffe
/// (Klassennamen, Schulname …) → Pseudonym bzw. Platzhalter.
pub fn anonymize_mapping(vault: &ProjectVault) -> Vec<(String, String)> {
    let mut out = person_forward(vault);
    for t in vault.terms.iter().filter(|t| t.active) {
        // Personen-Bezeichner haben Vorrang, falls ein Begriff gleich lautet
        if !out.iter().any(|(k, _)| k.eq_ignore_ascii_case(&t.text)) {
            out.push((t.text.clone(), t.replacement.clone()));
        }
    }
    sort_longest_first(&mut out);
    out
}

/// Nur die Personen-Bezeichner, für die Suche nach Tippfehlern (Review).
/// Begriffe bleiben aussen vor: «INF 24b» soll nicht als Tippfehler von
/// «INF 24a» vorgeschlagen werden.
pub fn person_mapping(vault: &ProjectVault, anonymize: bool) -> Vec<(String, String)> {
    if anonymize {
        person_forward(vault)
    } else {
        person_reverse(vault)
    }
}

fn apply(text: &str, mapping: &[(String, String)], whole_words: bool) -> String {
    crate::matcher::Matcher::cached(mapping, whole_words)
        .ok()
        .and_then(|m| m.replace_all(text, |_| {}))
        .unwrap_or_else(|| text.to_string())
}

/// Wendet ein Mapping auf einen Text an, mit denselben Regeln wie im Dokument
/// (ohne Gross-/Kleinschreibung, längste zuerst, nur ganze Wörter).
pub fn apply_to_text(text: &str, mapping: &[(String, String)]) -> String {
    apply(text, mapping, true)
}

/// Dateiname (ohne Endung) für die Ausgabe: Namen ersetzt, Pfadtrenner entschärft.
/// Hier wird auch mitten im Wort ersetzt («MüllerAnna_Pruefung»): im Dateinamen
/// wäre ein stehen gebliebener Name schlimmer als ein verändertes Wort.
pub fn output_stem(stem: &str, mapping: &[(String, String)]) -> String {
    let s: String = apply(stem, mapping, false)
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    if s.trim().is_empty() {
        "datei".to_string()
    } else {
        s
    }
}

/// Personen: (echter Bezeichner, Pseudonym).
/// Kollidierende Bezeichner (z.B. gleicher Vorname bei zwei Personen) werden
/// übersprungen — sie landen später in der Fuzzy-/Review-Liste (M4).
fn person_forward(vault: &ProjectVault) -> Vec<(String, String)> {
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

/// Rückwärts-Mapping (De-Anonymisierung): Pseudonyme und Platzhalter zurück.
/// Auch inaktive Begriffe (z.B. von einer inzwischen entfernten Klasse) werden
/// zurückgesetzt, damit ältere Unterlagen vollständig lesbar werden.
pub fn deanonymize_mapping(vault: &ProjectVault) -> Vec<(String, String)> {
    let mut out = person_reverse(vault);
    for t in &vault.terms {
        if !out.iter().any(|(k, _)| k.eq_ignore_ascii_case(&t.replacement)) {
            out.push((t.replacement.clone(), t.text.clone()));
        }
    }
    sort_longest_first(&mut out);
    out
}

/// Personen rückwärts: (Pseudonym, Echtname).
/// Pseudonyme, die mehreren Personen zugeordnet wären, werden ausgelassen.
fn person_reverse(vault: &ProjectVault) -> Vec<(String, String)> {
    let forward = person_forward(vault);
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

    #[test]
    fn terms_are_replaced_and_restored() {
        use crate::roster::Roster;
        let mut roster = Roster::default();
        let class = roster.add_class_with("INF 24a", vec![Person::new("Anna", "Müller")]);
        let short = roster.add_class_with("3b", vec![Person::new("Ben", "Schulz")]);
        roster.add_term("Berufsschule Bern").unwrap();
        assert!(roster.add_term("ab").is_none(), "zu kurz");

        let mut v = ProjectVault::new("Modul 319");
        v.link_class(class, &roster);
        v.link_class(short, &roster);
        let fwd = anonymize_mapping(&v);
        let text = "Berufsschule Bern, Klasse inf 24a, Aufgabe 3b: Anna Müller";
        let anon = apply_to_text(text, &fwd);
        assert!(!anon.contains("Bern") && !anon.to_lowercase().contains("inf 24a") && !anon.contains("Müller"), "{anon}");
        assert!(anon.contains("Aufgabe 3b"), "kurze Klassennamen bleiben stehen: {anon}");
        assert!(anon.contains("KL-") && anon.contains("BG-"));

        let back = apply_to_text(&anon, &deanonymize_mapping(&v));
        assert_eq!(back, "Berufsschule Bern, Klasse INF 24a, Aufgabe 3b: Anna Müller");

        // Tippfehler-Suche kennt nur Personen
        assert!(person_mapping(&v, true).iter().all(|(k, _)| k != "INF 24a"));

        // Klasse entfernt: wird nicht mehr ersetzt, aber alte Platzhalter gehen noch zurück
        v.unlink_class(class, &roster);
        assert!(!anonymize_mapping(&v).iter().any(|(k, _)| k == "INF 24a"));
        assert!(apply_to_text(&anon, &deanonymize_mapping(&v)).contains("INF 24a"));
    }

    #[test]
    fn file_names_are_anonymised() {
        let v = vault_with_person();
        let a = v.assignment_for(v.persons[0].id).unwrap();
        let stem = output_stem("Müller_Anna_Pruefung", &anonymize_mapping(&v));
        assert_eq!(stem, format!("{}_{}_Pruefung", a.last_name, a.first_name));
        let back = output_stem(&stem, &deanonymize_mapping(&v));
        assert_eq!(back, "Müller_Anna_Pruefung");
    }
}
