use super::*;
#[test]
fn legacy_json_and_content_references_match_independent_frozen_bytes() {
    for (raw, sha) in [
        (
            include_bytes!("fixtures/unguarded-payload.json").as_slice(),
            include_str!("fixtures/unguarded-reference.sha256"),
        ),
        (
            include_bytes!("fixtures/guarded-payload.json").as_slice(),
            include_str!("fixtures/guarded-reference.sha256"),
        ),
    ] {
        let p: Payload = serde_json::from_slice(raw).unwrap();
        assert!(p.valid());
        assert_eq!(serde_json::to_vec(&p).unwrap(), raw);
        assert_eq!(payload_reference(&Some(p)).as_deref(), Some(sha.trim()));
    }
    let raw = include_bytes!("fixtures/legacy-apply.json");
    let r: Apply = serde_json::from_slice(raw).unwrap();
    r.validate().unwrap();
    assert_eq!(serde_json::to_vec(&r).unwrap(), raw);
}
