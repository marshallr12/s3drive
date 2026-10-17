//! Versioned admission rules; display names are never normalized or rewritten.
use unicode_normalization::UnicodeNormalization;
pub const EXACT: &str = "exact-utf8-v1";
pub const PORTABLE: &str = "portable-collision-v1";
const _: () = {
    let r = std::char::UNICODE_VERSION;
    let n = unicode_normalization::UNICODE_VERSION;
    assert!(r.0 == 17 && r.1 == 0 && r.2 == 0 && n.0 == 17 && n.1 == 0 && n.2 == 0);
};
pub fn supported(policy: &str) -> bool {
    matches!(policy, EXACT | PORTABLE)
}
pub fn valid_component(name: &str) -> bool {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.ends_with([' ', '.'])
        || name.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*'])
        || name.chars().any(char::is_control)
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return false;
    }
    !["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}

/// A project-defined compatibility/case closure, not OS lookup equivalence.
/// This key admits at most one member of an alias class in a portable directory.
pub fn collision_key(name: &str) -> Option<String> {
    if !valid_component(name) {
        return None;
    }
    let mut key = name.to_owned();
    for _ in 0..8 {
        let next: String = key
            .nfkc()
            .flat_map(char::to_lowercase)
            .flat_map(char::to_uppercase)
            .nfkc()
            .collect();
        if next.len() > 4096 {
            return None;
        }
        if next == key {
            return Some(key);
        }
        key = next;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_collisions_retain_unicode_but_reject_alias_pairs() {
        for (a, b) in [
            ("café", "cafe\u{301}"),
            ("Case", "case"),
            ("ı", "I"),
            ("ß", "ẞ"),
            ("ß", "ss"),
            ("Σ", "ς"),
            ("Ｋ", "k"),
        ] {
            assert_eq!(collision_key(a), collision_key(b), "{a} / {b}");
        }
        for name in ["資料 🦀 %2F + café.bin", ".hidden", "COM10"] {
            let key = collision_key(name).unwrap();
            assert_eq!(collision_key(&key).as_ref(), Some(&key));
        }
        for name in [
            "CON.txt",
            "con .bin",
            "COM¹.txt",
            "LPT²",
            "bad:name",
            "trail.",
            "trail ",
            "",
            "..",
        ] {
            assert!(collision_key(name).is_none(), "{name}");
        }
    }
}
