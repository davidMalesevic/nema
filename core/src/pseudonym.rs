//! Deterministische Pseudonyme pro Projekt: derselbe Lernende bekommt in
//! einem Projekt immer dasselbe Pseudonym (seed-basiert aus dem Projekt-Seed
//! und der Personen-ID). Dadurch kann die KI konsistent über alle Prüfungen
//! eines Projekts hinweg bewerten.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashSet;
use uuid::Uuid;

const FIRST_NAMES: &[&str] = &[
    "Anna", "Ben", "Carla", "David", "Elena", "Felix", "Greta", "Henry", "Ida", "Jonas",
    "Katharina", "Liam", "Marie", "Noah", "Olivia", "Paul", "Quinn", "Rosa", "Samuel", "Tara",
    "Ulrich", "Vera", "Wendelin", "Xenia", "Yusuf", "Zora", "Leo", "Mia", "Emil", "Lina",
    "Theo", "Hannah", "Leon", "Emma", "Finn", "Sophia", "Louis", "Clara", "Oscar", "Mathilda",
    "Jakob", "Frieda", "Anton", "Lotta", "Max", "Jule", "Karl", "Mila", "Oskar", "Rieke",
];

const LAST_NAMES: &[&str] = &[
    "Adler", "Berger", "Brandt", "Chen", "Donner", "Engel", "Fuchs", "Graf", "Hahn", "Hofmann",
    "Keller", "Kraft", "Krüger", "Lang", "Lorenz", "Meyer", "Neumann", "Otto", "Pfeiffer", "Quartz",
    "Richter", "Roth", "Schulz", "Seidel", "Steiner", "Turner", "Ullmann", "Vogel", "Wagner", "Wolf",
    "Yilmaz", "Zimmermann", "Amsel", "Bach", "Dorn", "Falk", "Gross", "Haas", "Igel", "Jung",
    "Koch", "Lindner", "März", "Nix", "Pohl", "Reuter", "Stein", "Vogt", "Ziegler", "Holzer",
];

fn seed_bytes(seed_hex: &str) -> [u8; 32] {
    let bytes = hex::decode(seed_hex).expect("Seed ist gültiges Hex");
    bytes.try_into().expect("Seed ist 32 Byte")
}

fn rng_for(seed: &[u8; 32], person_id: Uuid, attempt: u32) -> ChaCha8Rng {
    let mut s = *seed;
    for (i, b) in person_id.as_bytes().iter().enumerate() {
        s[i % 32] ^= b;
    }
    s[30] ^= (attempt & 0xff) as u8;
    s[31] ^= ((attempt >> 8) & 0xff) as u8;
    ChaCha8Rng::from_seed(s)
}

/// Generiert ein eindeutiges Pseudonym (Vorname, Nachname) für eine Person.
pub fn generate(
    seed_hex: &str,
    person_id: Uuid,
    taken: &HashSet<(String, String)>,
) -> (String, String) {
    let seed = seed_bytes(seed_hex);
    let mut attempt = 0u32;
    loop {
        let mut rng = rng_for(&seed, person_id, attempt);
        let first = FIRST_NAMES[rng.gen_range(0..FIRST_NAMES.len())].to_string();
        let last = LAST_NAMES[rng.gen_range(0..LAST_NAMES.len())].to_string();
        if !taken.contains(&(first.clone(), last.clone())) {
            return (first, last);
        }
        attempt += 1;
        assert!(attempt < 100_000, "Pseudonym-Pool erschöpft");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_per_person() {
        let seed = hex::encode([7u8; 32]);
        let id = Uuid::new_v4();
        let a = generate(&seed, id, &HashSet::new());
        let b = generate(&seed, id, &HashSet::new());
        assert_eq!(a, b);
    }

    #[test]
    fn different_persons_different_seeds() {
        let seed = hex::encode([7u8; 32]);
        let a = generate(&seed, Uuid::new_v4(), &HashSet::new());
        let b = generate(&seed, Uuid::new_v4(), &HashSet::new());
        // Kollisionen sind möglich, aber bei 50x50 Namen und 2 Personen unwahrscheinlich
        assert_ne!(a, b);
    }

    #[test]
    fn unique_for_many_persons() {
        let seed = hex::encode([3u8; 32]);
        let mut taken = HashSet::new();
        for _ in 0..100 {
            let id = Uuid::new_v4();
            let p = generate(&seed, id, &taken);
            assert!(taken.insert(p.clone()), "doppeltes Pseudonym: {p:?}");
        }
        assert_eq!(taken.len(), 100);
    }
}
