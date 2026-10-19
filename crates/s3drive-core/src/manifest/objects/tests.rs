use super::*;

const RAW: &[u8] = include_bytes!("fixtures/root-descriptor.bin");
const PAGE: &[u8] = include_bytes!("../fixtures/root.bin");
fn root() -> RootDescriptor {
    RootDescriptor::decode(RAW).unwrap()
}
fn namespace() -> Namespace {
    Namespace::new("managed/".into(), [1; 16], [2; 16], [3; 16]).unwrap()
}

#[test]
fn independent_root_and_key_vectors_fix_all_bytes_and_address_components() {
    let r = root();
    assert_eq!(r.encode(), RAW);
    assert_eq!(r.volume(), &[1; 16]);
    assert_eq!(r.node(), &[2; 16]);
    assert_eq!(r.length(), 1048592);
    assert_eq!(r.page().store(), &[3; 16]);
    assert_eq!(r.page().generation(), &[6; 16]);
    assert_eq!(r.page().ordinal(), 11);
    assert_eq!(r.verify_page(PAGE).unwrap().page().length(), r.length());
    // Python JSON is deliberately read without a Rust serialization dependency.
    let keys = include_str!("fixtures/keys.json");
    for role in [Role::Data, Role::Page] {
        let key = namespace().key(role, r.page()).unwrap();
        let label = match role {
            Role::Data => "data",
            Role::Page => "page",
        };
        assert!(keys.contains(&format!("\"{label}\": \"{}\"", key.as_str())));
        assert!(key.as_str().is_ascii());
    }
    assert_eq!(
        namespace().root_key(&r).unwrap(),
        namespace().key(Role::Page, r.page()).unwrap()
    );
}

#[test]
fn descriptor_exact_width_version_ids_and_signed_length_are_strict() {
    for end in 0..RAW.len() {
        assert!(RootDescriptor::decode(&RAW[..end]).is_err());
    }
    let mut trailing = RAW.to_vec();
    trailing.push(0);
    assert_eq!(RootDescriptor::decode(&trailing), Err(Error::Bounds));
    for at in 0..8 {
        let mut changed = RAW.to_vec();
        changed[at] ^= 1;
        assert_eq!(RootDescriptor::decode(&changed), Err(Error::Encoding));
    }
    for start in [8, 24, 48, 64] {
        let mut changed = RAW.to_vec();
        changed[start..start + 16].fill(0);
        assert_eq!(RootDescriptor::decode(&changed), Err(Error::Binding));
    }
    let r = root();
    for length in [0, MAX_LENGTH] {
        let v = RootDescriptor::new([1; 16], [2; 16], length, r.page().clone()).unwrap();
        assert_eq!(RootDescriptor::decode(&v.encode()).unwrap(), v);
    }
    for length in [MAX_LENGTH + 1, u64::MAX] {
        assert_eq!(
            RootDescriptor::new([1; 16], [2; 16], length, r.page().clone()),
            Err(Error::Bounds)
        );
        let mut raw = RAW.to_vec();
        raw[40..48].copy_from_slice(&length.to_le_bytes());
        assert_eq!(RootDescriptor::decode(&raw), Err(Error::Bounds));
    }
    assert_eq!(
        RootDescriptor::new([0; 16], [2; 16], 0, r.page().clone()),
        Err(Error::Binding)
    );
    assert_eq!(
        RootDescriptor::new([1; 16], [0; 16], 0, r.page().clone()),
        Err(Error::Binding)
    );
}

#[test]
fn page_verification_binds_hash_file_length_and_root_start() {
    let r = root();
    for (volume, node, length) in [
        ([8; 16], [2; 16], r.length()),
        ([1; 16], [8; 16], r.length()),
        ([1; 16], [2; 16], r.length() + 1),
    ] {
        let changed = RootDescriptor::new(volume, node, length, r.page().clone()).unwrap();
        assert_eq!(changed.verify_page(PAGE), Err(Error::Binding));
    }
    let mut raw = RAW.to_vec();
    raw[88] ^= 1;
    assert_eq!(
        root().verify_page(&PAGE[..PAGE.len() - 1]),
        Err(Error::Encoding)
    );
    assert_eq!(
        RootDescriptor::decode(&raw).unwrap().verify_page(PAGE),
        Err(Error::Integrity)
    );
    let empty = include_bytes!("../fixtures/empty.bin");
    let p = Page::decode(empty).unwrap();
    let r = RootDescriptor::new(
        [1; 16],
        [2; 16],
        0,
        Reference::new([3; 16], [6; 16], 0, p.digest()).unwrap(),
    )
    .unwrap();
    r.verify_page(empty).unwrap();
}

#[test]
fn role_and_every_identity_dimension_separate_keys_but_hash_does_not() {
    let n = namespace();
    let r = root();
    let original = n.key(Role::Data, r.page()).unwrap();
    let mut keys = std::collections::HashSet::new();
    assert!(keys.insert(original.as_str().to_owned()));
    for other in [
        n.key(Role::Page, r.page()).unwrap(),
        Namespace::new("other/".into(), [1; 16], [2; 16], [3; 16])
            .unwrap()
            .key(Role::Data, r.page())
            .unwrap(),
        Namespace::new("managed/".into(), [9; 16], [2; 16], [3; 16])
            .unwrap()
            .key(Role::Data, r.page())
            .unwrap(),
        Namespace::new("managed/".into(), [1; 16], [9; 16], [3; 16])
            .unwrap()
            .key(Role::Data, r.page())
            .unwrap(),
        Namespace::new("managed/".into(), [1; 16], [2; 16], [9; 16])
            .unwrap()
            .key(
                Role::Data,
                &Reference::new([9; 16], [6; 16], 11, *r.page().sha256()).unwrap(),
            )
            .unwrap(),
        n.key(
            Role::Data,
            &Reference::new([3; 16], [9; 16], 11, *r.page().sha256()).unwrap(),
        )
        .unwrap(),
        n.key(
            Role::Data,
            &Reference::new([3; 16], [6; 16], 12, *r.page().sha256()).unwrap(),
        )
        .unwrap(),
    ] {
        assert!(keys.insert(other.as_str().to_owned()), "identity alias");
    }
    let changed_digest = Reference::new([3; 16], [6; 16], 11, [0; 32]).unwrap();
    assert_eq!(n.key(Role::Data, &changed_digest).unwrap(), original);
    // Same operation-owned address with different bytes needs a conflict. This
    // builder does not provide a write, overwrite fallback or adoption permit.
}

#[test]
fn mismatched_mapping_or_root_context_is_refused_and_debug_is_private() {
    let r = root();
    let wrong_store = Reference::new([9; 16], [6; 16], 11, [0; 32]).unwrap();
    assert_eq!(
        namespace().key(Role::Page, &wrong_store),
        Err(Error::Binding)
    );
    for (volume, node, store) in [
        ([9; 16], [2; 16], [3; 16]),
        ([1; 16], [9; 16], [3; 16]),
        ([1; 16], [2; 16], [9; 16]),
    ] {
        let n = Namespace::new("managed/".into(), volume, node, store).unwrap();
        assert_eq!(n.root_key(&r), Err(Error::Binding));
    }
    assert_eq!(format!("{:?}", namespace()), "ManifestNamespace([private])");
    assert_eq!(format!("{r:?}"), "ManifestRootDescriptor([private])");
    assert_eq!(
        format!("{:?}", namespace().root_key(&r).unwrap()),
        "ManifestObjectKey([private])"
    );
}

#[test]
fn roots_are_bounded_components_without_normalization_and_ordinals_are_fixed_hex() {
    for bad in [
        "", "/", "abc", "../", "./", "a/../", "a//", "a/./", "a%2f/", "a\\/", "a?/", "a#/", "a /",
        "a\0/", "é/", "a\n/",
    ] {
        assert_eq!(
            Namespace::new(bad.into(), [1; 16], [2; 16], [3; 16]),
            Err(Error::Shape)
        );
    }
    for (v, n, s) in [
        ([0; 16], [2; 16], [3; 16]),
        ([1; 16], [0; 16], [3; 16]),
        ([1; 16], [2; 16], [0; 16]),
    ] {
        assert_eq!(
            Namespace::new("managed/".into(), v, n, s),
            Err(Error::Binding)
        );
    }
    assert_eq!(
        Namespace::new(format!("{}/", "a".repeat(128)), [1; 16], [2; 16], [3; 16]),
        Err(Error::Shape)
    );
    let n = Namespace::new(
        format!("{}/", "a".repeat(127)),
        [0xab; 16],
        [0xcd; 16],
        [3; 16],
    )
    .unwrap();
    for ordinal in [0, u64::MAX] {
        let reference = Reference::new([3; 16], [0xef; 16], ordinal, [0; 32]).unwrap();
        let key = n.key(Role::Data, &reference).unwrap();
        assert_eq!(key.as_str().len(), 284);
        assert!(key.as_str().ends_with(&format!("/{ordinal:016x}")));
        assert!(!key.as_str().contains("AB"));
    }
    Namespace::new(".s3drive/A_b-1.x/".into(), [1; 16], [2; 16], [3; 16]).unwrap();
}

#[test]
fn parsed_descriptor_and_valid_page_do_not_authenticate_address_metadata() {
    // The page hash authenticates page bytes, not the authority response that
    // supplied it. A syntactically valid changed generation still verifies the
    // same page but addresses a different key. The authority must authenticate
    // the entire descriptor; consumers must not promote parsed metadata to trust.
    let original = root();
    let mut changed = RAW.to_vec();
    changed[64] ^= 1;
    let changed = RootDescriptor::decode(&changed).unwrap();
    changed.verify_page(PAGE).unwrap();
    assert_ne!(
        namespace().root_key(&changed).unwrap(),
        namespace().root_key(&original).unwrap()
    );
}

#[test]
fn canonical_empty_root_matches_independent_bytes_digests_and_key() {
    use sha2::{Digest, Sha256};
    let (root, body) = RootDescriptor::empty([1; 16], [2; 16], [3; 16], [4; 16]).unwrap();
    assert_eq!(body, include_bytes!("fixtures/empty-page.bin"));
    assert_eq!(
        &root.encode(),
        include_bytes!("fixtures/empty-root-descriptor.bin")
    );
    assert_eq!(root.length(), 0);
    assert_eq!(root.page().ordinal(), 1 << 56);
    let verified = root.verify_page(&body).unwrap();
    assert!(verified.page().entries().is_empty());
    let fixture = include_str!("fixtures/empty-vector.json");
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert!(fixture.contains(&format!(
        "\"raw_sha256\": \"{}\"",
        hex(&Sha256::digest(&body))
    )));
    assert!(fixture.contains(&format!(
        "\"page_digest\": \"{}\"",
        hex(root.page().sha256())
    )));
    assert_ne!(&Sha256::digest(&body)[..], root.page().sha256());
    let ns = Namespace::new("managed/".into(), [1; 16], [2; 16], [3; 16]).unwrap();
    assert!(fixture.contains(&format!(
        "\"key\": \"{}\"",
        ns.root_key(&root).unwrap().as_str()
    )));
}
#[test]
fn canonical_empty_root_refuses_every_reserved_identity() {
    for at in 0..4 {
        let mut ids = [[1; 16], [2; 16], [3; 16], [4; 16]];
        ids[at] = [0; 16];
        assert!(RootDescriptor::empty(ids[0], ids[1], ids[2], ids[3]).is_err());
    }
}
