#![forbid(unsafe_code)]

// Reject projections that the Windows shell cannot address unambiguously.
pub use s3drive_core::names::valid_component;

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
