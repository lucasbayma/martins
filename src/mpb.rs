//! MPB artist name list and workspace name generator.

use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, thiserror::Error)]
pub enum NameError {
    #[error("name is empty")]
    Empty,
    #[error("name is too long (max 40 chars)")]
    TooLong,
    #[error("name contains invalid characters (only a-z0-9- allowed)")]
    InvalidChars,
    #[error("name cannot start with a dash")]
    StartsWithDash,
}

const ARTISTS: &[&str; 50] = &[
    // Bossa nova (5)
    "joao-gilberto",
    "tom-jobim",
    "vinicius",
    "dorival-caymmi",
    "carlos-lyra",
    // Tropicália (10)
    "caetano",
    "gil",
    "gal",
    "bethania",
    "tom-ze",
    "chico",
    "elis",
    "jorge-ben",
    "mutantes",
    "novos-baianos",
    // Clássicos pós-tropicália (10)
    "milton",
    "djavan",
    "marisa",
    "ivan-lins",
    "joao-bosco",
    "belchior",
    "alceu",
    "lo-borges",
    "edu-lobo",
    "marcos-valle",
    // Contemporâneos (15)
    "moreno",
    "bebel",
    "marisa-monte",
    "tulipa",
    "silva",
    "tiago-iorc",
    "liniker",
    "luedji",
    "rubel",
    "rodrigo-amarante",
    "jeneci",
    "ceu",
    "maria-gadu",
    "mallu",
    "arnaldo-antunes",
    // Emergentes (10)
    "tim-bernardes",
    "duda-beat",
    "flora-matos",
    "marina-sena",
    "emicida",
    "bala-desejo",
    "carne-doce",
    "kiko-dinucci",
    "fabiano",
    "ze-manoel",
];

/// Normalize a name: lowercase, strip accents, replace spaces with hyphens,
/// strip non-alphanumeric (except hyphens).
pub fn normalize(name: &str) -> String {
    name.nfd()
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .to_lowercase()
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

/// Validate a workspace name.
pub fn validate(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > 40 {
        return Err(NameError::TooLong);
    }
    if name.starts_with('-') {
        return Err(NameError::StartsWithDash);
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(NameError::InvalidChars);
    }
    Ok(())
}

/// Generate a unique workspace name not in `used`.
/// Picks randomly from ARTISTS. If all 50 are used, appends -2, -3, etc.
pub fn generate_name(used: &HashSet<String>) -> String {
    // Try each artist in random order
    let mut indices: Vec<usize> = (0..ARTISTS.len()).collect();
    // Fisher-Yates shuffle using fastrand
    for i in (1..indices.len()).rev() {
        let j = fastrand::usize(0..=i);
        indices.swap(i, j);
    }
    for i in indices {
        let name = ARTISTS[i].to_string();
        if !used.contains(&name) {
            return name;
        }
    }
    // All 50 used: find an artist and append suffix
    // Pick the first artist and increment suffix
    let base = ARTISTS[fastrand::usize(0..ARTISTS.len())];
    let mut n = 2u32;
    loop {
        let candidate = format!("{}-{}", base, n);
        if !used.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_empty_set() {
        let used = HashSet::new();
        let name = generate_name(&used);
        assert!(
            ARTISTS.contains(&name.as_str()),
            "expected one of 50 artists, got: {name}"
        );
    }

    #[test]
    fn generate_all_used_returns_suffix() {
        let used: HashSet<String> = ARTISTS.iter().map(|s| s.to_string()).collect();
        let name = generate_name(&used);
        // Should be one of the artists with -2 suffix
        let has_suffix = ARTISTS.iter().any(|a| name == format!("{}-2", a));
        assert!(has_suffix, "expected artist with -2 suffix, got: {name}");
    }

    #[test]
    fn collision_handling() {
        // Fill all 50 + all -2 variants
        let mut used: HashSet<String> = ARTISTS.iter().map(|s| s.to_string()).collect();
        for a in ARTISTS {
            used.insert(format!("{}-2", a));
        }
        let name = generate_name(&used);
        let has_suffix = ARTISTS.iter().any(|a| name == format!("{}-3", a));
        assert!(has_suffix, "expected artist with -3 suffix, got: {name}");
    }

    #[test]
    fn normalize_accents() {
        assert_eq!(normalize("João Gilberto"), "joao-gilberto");
        assert_eq!(normalize("Zé Manoel"), "ze-manoel");
        assert_eq!(normalize("Bethânia"), "bethania");
        assert_eq!(normalize("Lô Borges"), "lo-borges");
    }

    #[test]
    fn validate_cases() {
        assert!(validate("").is_err());
        assert!(validate("caetano").is_ok());
        assert!(validate("foo bar").is_err()); // space
        assert!(validate("-bad").is_err());
        assert!(validate("a".repeat(41).as_str()).is_err());
        assert!(validate("valid-name-123").is_ok());
    }

    #[test]
    fn all_artists_valid() {
        for artist in ARTISTS {
            validate(artist).unwrap_or_else(|e| panic!("invalid artist {}: {}", artist, e));
        }
    }

    #[test]
    fn artist_count() {
        assert_eq!(ARTISTS.len(), 50);
    }
}
