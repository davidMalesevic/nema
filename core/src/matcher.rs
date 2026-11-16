//! Gemeinsamer Regex-Matcher für DOCX und PDF: matcht alle Suchtexte
//! case-insensitiv, längste zuerst (damit „Anna Müller" vor „Müller“ gewinnt).

use regex::Regex;
use std::cmp::Reverse;
use std::collections::HashMap;

/// lowercased match → (original-Schlüssel fürs Reporting, Ersetzung)
pub type Lookup = HashMap<String, (String, String)>;

pub fn build_matcher(mapping: &[(String, String)]) -> Result<(Regex, Lookup), regex::Error> {
    let mut sorted: Vec<&(String, String)> = mapping.iter().collect();
    sorted.sort_by_key(|(k, _)| Reverse(k.len()));
    let pattern = sorted
        .iter()
        .map(|(k, _)| regex::escape(k))
        .collect::<Vec<_>>()
        .join("|");
    let re = Regex::new(&format!("(?i)(?:{pattern})"))?;
    let lookup = mapping
        .iter()
        .map(|(k, v)| (k.to_lowercase(), (k.clone(), v.clone())))
        .collect();
    Ok((re, lookup))
}
