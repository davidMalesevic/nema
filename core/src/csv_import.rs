//! Import von Namenslisten (CSV/TXT).
//!
//! Unterstützte Formen:
//! - mit Headerzeile (Spalten werden an Hand der Überschriften erkannt,
//!   z.B. `Nachname;Vorname;E-Mail;Matrikel`)
//! - ohne Header, mehrere Spalten: Konvention `Nachname, Vorname, [E-Mail], [Matrikel]`
//! - eine Spalte: `Vorname Nachname` oder `Nachname, Vorname`

use crate::person::Person;

#[derive(Debug, Clone, PartialEq)]
pub struct PersonInput {
    pub first_name: String,
    pub last_name: String,
    pub emails: Vec<String>,
    pub matriculation: Option<String>,
}

impl PersonInput {
    pub fn into_person(self) -> Person {
        let mut p = Person::new(self.first_name, self.last_name);
        p.emails = self.emails;
        p.matriculation = self.matriculation;
        p
    }
}

fn split_line(line: &str, delimiter: char) -> Vec<String> {
    line.split(delimiter)
        .map(|c| {
            let c = c.trim();
            c.strip_prefix('"')
                .and_then(|c| c.strip_suffix('"'))
                .unwrap_or(c)
                .trim()
                .to_string()
        })
        .collect()
}

fn find_col(cols: &[String], exact: &[&str], contains: &[&str]) -> Option<usize> {
    let norm: Vec<String> = cols.iter().map(|c| c.to_lowercase()).collect();
    if let Some(i) = norm.iter().position(|c| exact.iter().any(|e| c == e)) {
        return Some(i);
    }
    norm.iter().position(|c| contains.iter().any(|e| c.contains(e)))
}

fn looks_like_header(line: &str) -> bool {
    let l = line.to_lowercase();
    ["vorname", "nachname", "name", "e-mail", "email", "mail", "matrikel"]
        .iter()
        .any(|k| l.contains(k))
}

fn make_input(last: &str, first: &str, email: Option<&str>, matrikel: Option<&str>) -> Option<PersonInput> {
    let last = last.trim().to_string();
    let first = first.trim().to_string();
    if last.is_empty() && first.is_empty() {
        return None;
    }
    Some(PersonInput {
        first_name: first,
        last_name: last,
        emails: email
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty() && e.contains('@'))
            .into_iter()
            .collect(),
        matriculation: matrikel
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty()),
    })
}

/// Parst eine Namensliste. `delimiter` ist das Spaltentrennzeichen.
pub fn parse(text: &str, delimiter: char) -> Result<Vec<PersonInput>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .peekable();
    let first = lines.next().ok_or("Datei ist leer")?;

    let mut out = Vec::new();

    if looks_like_header(first) {
        let cols = split_line(first, delimiter);
        let vi = find_col(&cols, &["vorname", "firstname", "first name", "given name", "first", "given"], &["vorname", "given", "first"])
            .ok_or("Spalte für Vorname nicht gefunden")?;
        let ni = find_col(&cols, &["nachname", "lastname", "last name", "surname", "family name", "name"], &["nachname", "surname", "lastname", "family"])
            .ok_or("Spalte für Nachname nicht gefunden")?;
        let ei = find_col(&cols, &["e-mail", "email", "mail"], &["mail"]);
        let mi = find_col(&cols, &["matrikel", "matriculation", "nummer"], &["matrikel", "matric"]);
        for (lineno, line) in lines.enumerate() {
            let cols = split_line(line, delimiter);
            let get = |i: Option<usize>| i.and_then(|i| cols.get(i)).map(|s| s.as_str());
            if let Some(input) = make_input(get(Some(ni)).unwrap_or(""), get(Some(vi)).unwrap_or(""), get(ei), get(mi)) {
                out.push(input);
            } else {
                return Err(format!("Zeile {}: kein Name erkannt", lineno + 2));
            }
        }
    } else if first.contains(delimiter) {
        // ohne Header: Nachname, Vorname, [E-Mail], [Matrikel]
        for (lineno, line) in std::iter::once(first).chain(lines).enumerate() {
            let cols = split_line(line, delimiter);
            let last = cols.first().map(|s| s.as_str()).unwrap_or("");
            let first_n = cols.get(1).map(|s| s.as_str()).unwrap_or("");
            let email = cols.get(2).map(|s| s.as_str());
            let matrikel = cols.get(3).map(|s| s.as_str());
            if let Some(input) = make_input(last, first_n, email, matrikel) {
                out.push(input);
            } else {
                return Err(format!("Zeile {}: kein Name erkannt", lineno + 1));
            }
        }
    } else {
        // eine Spalte
        for (lineno, line) in std::iter::once(first).chain(lines).enumerate() {
            let input = if let Some(comma) = line.find(',') {
                make_input(&line[..comma], &line[comma + 1..], None, None)
            } else {
                let mut parts = line.splitn(2, char::is_whitespace);
                let first_n = parts.next().unwrap_or("");
                let last = parts.next().unwrap_or("");
                make_input(last, first_n, None, None)
            };
            if let Some(input) = input {
                out.push(input);
            } else {
                return Err(format!("Zeile {}: kein Name erkannt", lineno + 1));
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_semicolon() {
        let text = "Nachname;Vorname;E-Mail;Matrikel\nMüller;Anna;anna@schule.ch;M-1\nSchulz;Ben;;";
        let v = parse(text, ';').unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].last_name, "Müller");
        assert_eq!(v[0].first_name, "Anna");
        assert_eq!(v[0].emails, vec!["anna@schule.ch".to_string()]);
        assert_eq!(v[0].matriculation.as_deref(), Some("M-1"));
        assert_eq!(v[1].last_name, "Schulz");
        assert!(v[1].emails.is_empty());
        assert_eq!(v[1].matriculation, None);
    }

    #[test]
    fn no_header_convention_last_first() {
        let v = parse("Müller, Anna\nSchulz, Ben", ',').unwrap();
        assert_eq!(v[0].last_name, "Müller");
        assert_eq!(v[0].first_name, "Anna");
        assert_eq!(v[1].first_name, "Ben");
    }

    #[test]
    fn single_column_first_last() {
        // Tab als Delimiter, aber Zeilen enthalten kein Tab → Single-Column-Modus
        let v = parse("Anna Müller\nBen Schulz", '\t').unwrap();
        assert_eq!(v[0].first_name, "Anna");
        assert_eq!(v[0].last_name, "Müller");
        assert_eq!(v[1].first_name, "Ben");
        assert_eq!(v[1].last_name, "Schulz");
    }

    #[test]
    fn single_column_last_comma_first() {
        let v = parse("Müller, Anna", '\t').unwrap();
        assert_eq!(v[0].last_name, "Müller");
        assert_eq!(v[0].first_name, "Anna");
    }

    #[test]
    fn quoted_and_bom() {
        let v = parse("\u{feff}\"Nachname\";\"Vorname\"\n\"Müller\";\"Anna\"", ';').unwrap();
        assert_eq!(v[0].last_name, "Müller");
        assert_eq!(v[0].first_name, "Anna");
    }

    #[test]
    fn empty_file_errors() {
        assert!(parse("   \n  ", ';').is_err());
    }
}
