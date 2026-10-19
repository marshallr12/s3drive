use super::*;
use s3drive_core::manifest::base::Descriptor;
pub const C: u64 = manifest::CHUNK_BYTES;
pub struct Base {
    pub root: RootDescriptor,
    pub page: Page,
    pub pages: BTreeMap<(i8, u64), Page>,
}
pub fn pattern(index: u64, size: u64) -> Vec<u8> {
    (0..size).map(|n| ((index * 37 + n) % 251) as u8).collect()
}
pub fn base(size: u64, data: BTreeMap<u64, Vec<u8>>) -> Base {
    let mut pages = BTreeMap::<(i8, u64), Page>::new();
    for level in 0..=model::root_level(size) {
        for slot in 0..model::count(size, level) {
            let mut entries = Vec::new();
            for child in slot * 128..((slot + 1) * 128).min(model::count(size, level - 1)) {
                let offset = child * model::span(level - 1);
                let length = (size - offset).min(model::span(level - 1));
                let entry = if level > 0 {
                    let p = &pages[&(level - 1, child)];
                    Entry::Child {
                        length,
                        object: Reference::new(
                            [5; 16],
                            [7; 16],
                            model::ordinal(level - 1, child),
                            p.digest(),
                        )
                        .unwrap(),
                    }
                } else if let Some(bytes) = data.get(&child) {
                    assert_eq!(bytes.len() as u64, length);
                    Entry::Data {
                        length,
                        object: Reference::new([5; 16], [7; 16], child, digest(bytes)).unwrap(),
                    }
                } else {
                    Entry::Hole { length }
                };
                if let (Some(Entry::Hole { length: a }), Entry::Hole { length: b }) =
                    (entries.last_mut(), &entry)
                {
                    *a += *b;
                } else {
                    entries.push(entry);
                }
            }
            pages.insert(
                (level, slot),
                Page::new(
                    [1; 16],
                    [2; 16],
                    slot * model::span(level),
                    level as u8,
                    entries,
                )
                .unwrap(),
            );
        }
    }
    let level = model::root_level(size);
    let page = pages[&(level, 0)].clone();
    let root = RootDescriptor::new(
        [1; 16],
        [2; 16],
        size,
        Reference::new([5; 16], [7; 16], model::ordinal(level, 0), page.digest()).unwrap(),
    )
    .unwrap();
    Base { root, page, pages }
}
pub fn binding(size: u64) -> Binding {
    Binding::new(
        Descriptor::from_hex(
            &"01".repeat(16),
            &"02".repeat(16),
            &"03".repeat(16),
            &"4".repeat(64),
        )
        .unwrap(),
        [5; 16],
        [6; 32],
        "a".repeat(64),
        "managed/".into(),
        size,
    )
    .unwrap()
}
pub fn view(s: &mut State, b: &Base, size: u64) -> View {
    super::super::super::Journal::new(s, super::super::super::Options::default())
        .unwrap()
        .begin([1; 32], binding(b.root.length()), size)
        .unwrap()
}
pub fn request(v: &View, b: &Base, operation: u8) -> Request {
    Request {
        operation: [operation; 32],
        view: v.id(),
        generation: [operation; 16],
        base_root: b.root.encode().to_vec(),
        base_page: b.page.encode(),
    }
}
pub fn proof(b: &Base, position: u64, level: Option<u8>) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-authenticated-chunk-reference-v1\0");
    hash.update("4".repeat(64).as_bytes());
    let binding: Id = hash.finalize().into();
    let offset = if level.is_some() {
        position
    } else {
        position * C
    };
    let mut page = b.page.clone();
    let mut path = Vec::<Vec<u8>>::new();
    loop {
        if level == Some(page.level()) {
            break;
        }
        let found = page.lookup(offset).unwrap();
        let Entry::Child { object, .. } = found.entry else {
            assert!(level.is_none());
            break;
        };
        let child = b
            .pages
            .values()
            .find(|p| &p.digest() == object.sha256())
            .unwrap()
            .clone();
        path.push(child.encode());
        page = child;
    }
    let mut out = if level.is_some() {
        b"S3DPREF1".to_vec()
    } else {
        b"S3DCREF1".to_vec()
    };
    out.extend_from_slice(&binding);
    out.extend_from_slice(&position.to_le_bytes());
    if let Some(level) = level {
        out.push(level);
    }
    out.push(path.len() as u8);
    for page in path {
        out.extend_from_slice(&(page.len() as u16).to_le_bytes());
        out.extend_from_slice(&page);
    }
    out
}
pub fn finish(s: &mut State, mut info: Info) -> Info {
    for _ in 0..1024 {
        if info.root().unwrap().is_some() {
            return info;
        }
        info = Journal::new(s, Options::default())
            .unwrap()
            .step(info.id().unwrap(), info.cursor())
            .unwrap();
    }
    panic!("preparation did not finish within bounded page steps")
}
pub fn fail<T>(r: Result<T>, expected: Failure) {
    let error = match r {
        Ok(_) => panic!("unexpected success"),
        Err(e) => e,
    };
    assert_eq!(error.downcast_ref::<Failure>(), Some(&expected));
}
