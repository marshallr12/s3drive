#![forbid(unsafe_code)]

// Reject projections that the Windows shell cannot address unambiguously.
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unaddressable_names_without_rewriting_literal_keys() {
        for name in [
            "",
            ".",
            "..",
            "a.",
            "a ",
            "a:b",
            "a/b",
            "a\\b",
            "nul.txt",
            "CON",
            "con .txt",
            "Lpt².doc",
            "COM1",
            "CONIN$",
            "a\0b",
            "a?b",
            "a*b",
            "a|b",
        ] {
            assert!(!valid_component(name), "{name:?}");
        }
        for name in [
            "a%2Fb + café.txt",
            "文書",
            "😀",
            ".hidden",
            "COM10",
            "my.nul.txt",
            "normal",
        ] {
            assert!(valid_component(name), "{name:?}");
        }
        assert!(!valid_component(&"é".repeat(128)));
    }
}
