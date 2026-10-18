use super::*;

const EMPTY: &[u8] = include_bytes!("fixtures/empty.bin");
const LEAF: &[u8] = include_bytes!("fixtures/leaf.bin");
const ROOT: &[u8] = include_bytes!("fixtures/root.bin");
const DATA: &[u8] = include_bytes!("fixtures/data.bin");
fn hash(hex: &str) -> [u8; 32] {
    let mut out = [0; 32];
    assert_eq!(hex.trim().len(), 64);
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}
fn leaf_hash() -> [u8; 32] {
    hash(include_str!("fixtures/leaf.sha256"))
}
fn root_hash() -> [u8; 32] {
    hash(include_str!("fixtures/root.sha256"))
}
fn reference(digest: [u8; 32]) -> Reference {
    Reference::new([3; 16], [4; 16], 7, digest).unwrap()
}
fn leaf() -> Page {
    Page::decode(LEAF).unwrap()
}
fn verified_leaf() -> VerifiedPage {
    Page::verify_root(
        LEAF,
        &leaf_hash(),
        &[1; 16],
        &[2; 16],
        CHUNK_BYTES + DATA.len() as u64,
    )
    .unwrap()
}
fn parent(child: &Page, length: u64) -> Page {
    Page::new(
        [1; 16],
        [2; 16],
        0,
        1,
        vec![Entry::Child {
            length,
            object: reference(child.digest()),
        }],
    )
    .unwrap()
}
fn verified(page: &Page) -> VerifiedPage {
    Page::verify_root(
        &page.encode(),
        &page.digest(),
        page.volume(),
        page.node(),
        page.length(),
    )
    .unwrap()
}

#[test]
fn independent_python_vectors_fix_encoding_hash_domains_and_identity_fields() {
    let empty = Page::new([1; 16], [2; 16], 0, 0, vec![]).unwrap();
    assert_eq!(empty.encode(), EMPTY);
    assert_eq!(empty.digest(), hash(include_str!("fixtures/empty.sha256")));
    let object = Reference::new(
        [3; 16],
        [4; 16],
        7,
        hash(include_str!("fixtures/data.sha256")),
    )
    .unwrap();
    let p = Page::new(
        [1; 16],
        [2; 16],
        0,
        0,
        vec![
            Entry::Hole {
                length: CHUNK_BYTES,
            },
            Entry::Data {
                length: DATA.len() as u64,
                object: object.clone(),
            },
        ],
    )
    .unwrap();
    assert_eq!(p.encode(), LEAF);
    assert_eq!(p.digest(), leaf_hash());
    assert_eq!(object.store(), &[3; 16]);
    assert_eq!(object.generation(), &[4; 16]);
    assert_eq!(object.ordinal(), 7);
    assert_eq!(*object.sha256(), <[u8; 32]>::from(Sha256::digest(DATA)));
    let root = Page::new(
        [1; 16],
        [2; 16],
        0,
        1,
        vec![Entry::Child {
            length: p.length(),
            object: Reference::new([3; 16], [5; 16], 9, p.digest()).unwrap(),
        }],
    )
    .unwrap();
    assert_eq!(root.encode(), ROOT);
    assert_eq!(root.digest(), root_hash());
    assert_ne!(root.digest(), <[u8; 32]>::from(Sha256::digest(ROOT)));
    for raw in [EMPTY, LEAF, ROOT] {
        assert_eq!(Page::decode(raw).unwrap().encode(), raw);
    }
}

#[test]
fn authenticated_root_child_and_complete_data_verify_before_range_use() {
    let root = Page::verify_root(ROOT, &root_hash(), &[1; 16], &[2; 16], leaf().length()).unwrap();
    let p = root.verify_child(0, LEAF).unwrap();
    assert_eq!(p.page(), &leaf());
    p.verify_data(1, DATA).unwrap();
    assert_eq!(p.verify_data(0, &[]), Err(Error::Shape));
    assert_eq!(p.verify_data(2, DATA), Err(Error::Shape));
    assert_eq!(root.verify_data(0, DATA), Err(Error::Shape));
    assert_eq!(p.verify_child(1, LEAF), Err(Error::Shape));
    assert_eq!(root.verify_child(1, LEAF), Err(Error::Shape));
    assert_eq!(
        p.verify_data(1, &DATA[..DATA.len() - 1]),
        Err(Error::Integrity)
    );
    let mut corrupt = DATA.to_vec();
    corrupt[0] ^= 1;
    assert_eq!(p.verify_data(1, &corrupt), Err(Error::Integrity));
    let oversized = vec![0; CHUNK_BYTES as usize + 1];
    assert_eq!(p.verify_data(1, &oversized), Err(Error::Integrity));
}

#[test]
fn every_byte_tamper_and_every_truncated_prefix_and_trailing_byte_are_refused() {
    for (bytes, digest) in [
        (EMPTY, hash(include_str!("fixtures/empty.sha256"))),
        (LEAF, leaf_hash()),
        (ROOT, root_hash()),
    ] {
        let size = Page::decode(bytes).unwrap().length();
        for i in 0..bytes.len() {
            assert!(Page::decode(&bytes[..i]).is_err(), "accepted prefix {i}");
            let mut changed = bytes.to_vec();
            changed[i] ^= 0x80;
            assert!(
                Page::verify_root(&changed, &digest, &[1; 16], &[2; 16], size).is_err(),
                "accepted tamper {i}"
            );
        }
        let mut extra = bytes.to_vec();
        extra.push(0);
        assert!(Page::decode(&extra).is_err());
    }
}

#[test]
fn root_identity_length_start_and_legacy_whole_sha_cannot_be_substituted() {
    assert_eq!(
        Page::verify_root(LEAF, &leaf_hash(), &[8; 16], &[2; 16], leaf().length()),
        Err(Error::Binding)
    );
    assert_eq!(
        Page::verify_root(LEAF, &leaf_hash(), &[1; 16], &[8; 16], leaf().length()),
        Err(Error::Binding)
    );
    assert_eq!(
        Page::verify_root(LEAF, &leaf_hash(), &[1; 16], &[2; 16], leaf().length() + 1),
        Err(Error::Binding)
    );
    let nonroot = Page::new([1; 16], [2; 16], CHUNK_BYTES, 0, leaf().entries().to_vec()).unwrap();
    assert_eq!(
        Page::verify_root(
            &nonroot.encode(),
            &nonroot.digest(),
            &[1; 16],
            &[2; 16],
            nonroot.length()
        ),
        Err(Error::Binding)
    );
    assert_eq!(
        Page::verify_root(
            LEAF,
            &Sha256::digest(LEAF).into(),
            &[1; 16],
            &[2; 16],
            leaf().length()
        ),
        Err(Error::Integrity)
    );
}

#[test]
fn even_validly_hashed_children_must_match_parent_volume_node_span_and_level() {
    for child in [
        Page::new([8; 16], [2; 16], 0, 0, leaf().entries().to_vec()).unwrap(),
        Page::new([1; 16], [8; 16], 0, 0, leaf().entries().to_vec()).unwrap(),
        Page::new([1; 16], [2; 16], CHUNK_BYTES, 0, leaf().entries().to_vec()).unwrap(),
        parent(&leaf(), leaf().length()),
    ] {
        let p = parent(&child, child.length());
        assert_eq!(
            verified(&p).verify_child(0, &child.encode()),
            Err(Error::Binding)
        );
    }
    let p = parent(&leaf(), CHUNK_BYTES);
    assert_eq!(verified(&p).verify_child(0, LEAF), Err(Error::Binding));
    let p = parent(&leaf(), leaf().length());
    let mut changed = LEAF.to_vec();
    changed[12] ^= 1;
    assert_eq!(
        verified(&p).verify_child(0, &changed),
        Err(Error::Integrity)
    );
}

#[test]
fn child_position_is_bound_and_each_verified_step_strictly_decreases_level() {
    let first = Page::new(
        [1; 16],
        [2; 16],
        0,
        0,
        vec![Entry::Hole {
            length: CHUNK_BYTES,
        }],
    )
    .unwrap();
    let second = Page::new(
        [1; 16],
        [2; 16],
        CHUNK_BYTES,
        0,
        vec![Entry::Hole {
            length: CHUNK_BYTES,
        }],
    )
    .unwrap();
    let p = Page::new(
        [1; 16],
        [2; 16],
        0,
        1,
        vec![
            Entry::Child {
                length: CHUNK_BYTES,
                object: reference(first.digest()),
            },
            Entry::Child {
                length: CHUNK_BYTES,
                object: reference(second.digest()),
            },
        ],
    )
    .unwrap();
    let p = verified(&p);
    assert_eq!(p.verify_child(0, &first.encode()).unwrap().page(), &first);
    assert_eq!(p.verify_child(1, &second.encode()).unwrap().page(), &second);
    assert_eq!(p.verify_child(1, &first.encode()), Err(Error::Integrity));
    // Same referenced bytes at the wrong second position: hash passes, span binding fails.
    let wrong = Page::new(
        [1; 16],
        [2; 16],
        0,
        1,
        vec![
            Entry::Child {
                length: CHUNK_BYTES,
                object: reference(first.digest())
            };
            2
        ],
    )
    .unwrap();
    assert_eq!(
        verified(&wrong).verify_child(1, &first.encode()),
        Err(Error::Binding)
    );
    let mut pages = vec![leaf()];
    for level in 1..=MAX_LEVEL {
        let child = pages.last().unwrap();
        pages.push(
            Page::new(
                [1; 16],
                [2; 16],
                0,
                level,
                vec![Entry::Child {
                    length: child.length(),
                    object: reference(child.digest()),
                }],
            )
            .unwrap(),
        );
    }
    let mut p = verified(pages.last().unwrap());
    for child in pages[..MAX_LEVEL as usize].iter().rev() {
        p = p.verify_child(0, &child.encode()).unwrap();
    }
    assert_eq!(p.page().level(), 0);
    p.verify_data(1, DATA).unwrap();
}

#[test]
fn lookup_holes_data_page_edges_and_eof_without_materializing_sparse_bytes() {
    let p = verified_leaf();
    let p = p.page();
    let a = p.lookup(0).unwrap();
    assert_eq!(a.start, 0);
    assert_eq!(a.offset_in_entry, 0);
    assert!(matches!(a.entry, Entry::Hole { .. }));
    assert_eq!(
        p.lookup(CHUNK_BYTES - 1).unwrap().offset_in_entry,
        CHUNK_BYTES - 1
    );
    let b = p.lookup(CHUNK_BYTES).unwrap();
    assert_eq!(b.start, CHUNK_BYTES);
    assert_eq!(b.offset_in_entry, 0);
    assert!(matches!(b.entry, Entry::Data { .. }));
    assert_eq!(
        p.lookup(p.length() - 1).unwrap().offset_in_entry,
        DATA.len() as u64 - 1
    );
    assert!(p.lookup(p.length()).is_none());
    assert!(p.lookup(u64::MAX).is_none());
    assert!(Page::decode(EMPTY).unwrap().lookup(0).is_none());
    let sparse = Page::new(
        [1; 16],
        [2; 16],
        0,
        0,
        vec![Entry::Hole { length: MAX_LENGTH }],
    )
    .unwrap();
    assert_eq!(sparse.encode().len(), 73);
    assert_eq!(
        sparse.lookup(MAX_LENGTH - 1).unwrap().offset_in_entry,
        MAX_LENGTH - 1
    );
    assert!(sparse.lookup(MAX_LENGTH).is_none());
    let sub = Page::new(
        [1; 16],
        [2; 16],
        CHUNK_BYTES,
        0,
        vec![Entry::Hole {
            length: CHUNK_BYTES,
        }],
    )
    .unwrap();
    assert!(sub.lookup(CHUNK_BYTES - 1).is_none());
    assert!(sub.lookup(2 * CHUNK_BYTES).is_none());
}

#[test]
fn explicit_page_fanout_depth_and_signed_geometry_bounds_are_enforced() {
    let data = Entry::Data {
        length: CHUNK_BYTES,
        object: reference([7; 32]),
    };
    let p = Page::new([1; 16], [2; 16], 0, 0, vec![data.clone(); MAX_ENTRIES]).unwrap();
    assert_eq!(p.encode().len(), MAX_PAGE_BYTES);
    assert_eq!(Page::decode(&p.encode()).unwrap(), p);
    assert_eq!(
        Page::new([1; 16], [2; 16], 0, 0, vec![data.clone(); MAX_ENTRIES + 1]),
        Err(Error::Bounds)
    );
    assert_eq!(
        Page::new([1; 16], [2; 16], 0, MAX_LEVEL + 1, vec![]),
        Err(Error::Bounds)
    );
    assert_eq!(
        Page::decode(&vec![0; MAX_PAGE_BYTES + 1]),
        Err(Error::Bounds)
    );
    assert_eq!(
        Page::new([1; 16], [2; 16], MAX_LENGTH + 1, 0, vec![]),
        Err(Error::Bounds)
    );
    assert_eq!(
        Page::new(
            [1; 16],
            [2; 16],
            0,
            0,
            vec![Entry::Hole { length: u64::MAX }]
        ),
        Err(Error::Bounds)
    );
    let start = MAX_LENGTH - MAX_LENGTH % CHUNK_BYTES;
    assert_eq!(
        Page::new([1; 16], [2; 16], start, 0, vec![data]),
        Err(Error::Bounds)
    );
    let p = Page::new(
        [1; 16],
        [2; 16],
        start,
        0,
        vec![Entry::Hole {
            length: MAX_LENGTH - start,
        }],
    )
    .unwrap();
    assert!(p.lookup(MAX_LENGTH - 1).is_some());
    assert!(p.lookup(MAX_LENGTH).is_none());
}

#[test]
fn role_alignment_adjacent_holes_and_empty_nonroot_forms_are_rejected() {
    let d = Entry::Data {
        length: 1,
        object: reference([7; 32]),
    };
    let c = Entry::Child {
        length: 1,
        object: reference([7; 32]),
    };
    let h = Entry::Hole {
        length: CHUNK_BYTES,
    };
    for (start, level, entries) in [
        (1, 0, vec![h.clone()]),
        (CHUNK_BYTES, 0, vec![]),
        (0, 1, vec![]),
        (0, 0, vec![Entry::Hole { length: 0 }]),
        (0, 0, vec![h.clone(), h.clone()]),
        (0, 0, vec![d.clone(), h.clone()]),
        (0, 0, vec![Entry::Hole { length: 1 }, d.clone()]),
        (
            0,
            0,
            vec![Entry::Data {
                length: CHUNK_BYTES + 1,
                object: reference([7; 32]),
            }],
        ),
        (0, 0, vec![c.clone()]),
        (0, 1, vec![d]),
        (0, 1, vec![h]),
        (0, 1, vec![c.clone(), c]),
    ] {
        assert_eq!(
            Page::new([1; 16], [2; 16], start, level, entries),
            Err(Error::Shape)
        );
    }
    assert_eq!(
        Page::new([0; 16], [2; 16], 0, 0, vec![]),
        Err(Error::Binding)
    );
    assert_eq!(
        Page::new([1; 16], [0; 16], 0, 0, vec![]),
        Err(Error::Binding)
    );
    assert_eq!(
        Reference::new([0; 16], [4; 16], 0, [0; 32]),
        Err(Error::Binding)
    );
    assert_eq!(
        Reference::new([3; 16], [0; 16], 0, [0; 32]),
        Err(Error::Binding)
    );
}

#[test]
fn decoder_rejects_unknown_tags_flags_geometry_and_header_length_lies() {
    for (at, value) in [(0, b'X'), (8, MAX_LEVEL + 1), (9, 1), (64, 3)] {
        let mut b = LEAF.to_vec();
        b[at] = value;
        assert!(Page::decode(&b).is_err());
    }
    for (range, value) in [
        (10..12, (MAX_ENTRIES as u16 + 1).to_le_bytes().to_vec()),
        (52..60, 1u64.to_le_bytes().to_vec()),
        (60..64, 4096u32.to_le_bytes().to_vec()),
    ] {
        let mut b = LEAF.to_vec();
        b[range].copy_from_slice(&value);
        assert!(Page::decode(&b).is_err());
    }
}
