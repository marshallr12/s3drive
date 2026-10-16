//! Shared byte-preservation corpus for the two real OS mount gates.
use super::fixture::Fixture;
use std::{collections::BTreeSet, path::Path};

const NAMES: &[&str] = &[
    "café.txt",        // NFC: precomposed accent.
    "cafe\u{301}.txt", // NFD: distinct S3 key, not an alias of the above.
    "日本語.txt",
    "中文.txt",
    "Привет.txt",
    "مرحبا.txt",
    "नमस्ते.txt",
    "🚀.txt",           // Supplementary code point / UTF-16 surrogate pair.
    "👩\u{200d}💻.txt", // Emoji sequence containing a zero-width joiner.
    "a%2Fb + café.txt", // Encoding must preserve percent, space and plus.
];
const DIRECTORY: &str = "目录-📁";
const CHILD: &str = "résumé-🎉.txt";

pub fn seed(fixture: &Fixture) {
    for name in NAMES {
        fixture.put(
            &format!("/test-bucket/desktop/unicode/{name}"),
            name.as_bytes(),
            "unicode",
        );
    }
    fixture.put(
        &format!("/test-bucket/desktop/unicode/{DIRECTORY}/{CHILD}"),
        CHILD.as_bytes(),
        "unicode-child",
    );
}

fn names(path: &Path) -> BTreeSet<String> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

pub fn verify(root: &Path) {
    assert_eq!(
        names(root),
        NAMES
            .iter()
            .copied()
            .chain([DIRECTORY])
            .map(str::to_owned)
            .collect(),
        "Unicode listing changed, normalized, or lost a filename"
    );
    for name in NAMES {
        assert_eq!(
            std::fs::read(root.join(name)).unwrap(),
            name.as_bytes(),
            "wrong object for Unicode filename {name:?}"
        );
    }
    let directory = root.join(DIRECTORY);
    assert_eq!(names(&directory), BTreeSet::from([CHILD.to_owned()]));
    assert_eq!(
        std::fs::read(directory.join(CHILD)).unwrap(),
        CHILD.as_bytes()
    );
    eprintln!(
        "Verified {} distinct Unicode filenames and a Unicode directory",
        NAMES.len() + 1
    );
}
