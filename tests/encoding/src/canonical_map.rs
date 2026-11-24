use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const DOMAIN: &[u8] = b"earnproof:canonical-map:v1\0";

fn canonical_map_hash(entries: &[(&str, &str)]) -> Result<String, &'static str> {
    if entries.len() > 64 {
        return Err("too many entries");
    }
    let mut keys = BTreeSet::new();
    for (key, _) in entries {
        if !keys.insert(key.as_bytes()) {
            return Err("duplicate key");
        }
    }
    let mut ordered = entries.to_vec();
    ordered.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));

    let mut encoded = Vec::from(DOMAIN);
    encoded.extend_from_slice(&(ordered.len() as u32).to_be_bytes());
    for (key, value) in ordered {
        encoded.extend_from_slice(&(key.len() as u32).to_be_bytes());
        encoded.extend_from_slice(key.as_bytes());
        encoded.extend_from_slice(&(value.len() as u32).to_be_bytes());
        encoded.extend_from_slice(value.as_bytes());
    }
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

#[test]
fn published_map_vectors_are_permutation_stable() {
    let first = [("country", "NG"), ("level", "3"), ("schema", "income-v1")];
    let reverse = [("schema", "income-v1"), ("level", "3"), ("country", "NG")];
    let expected = "795681f67c3ce6df060a8547f52fc69818861349b9b2837cd5dd304f62a194b6";
    assert_eq!(canonical_map_hash(&first).unwrap(), expected);
    assert_eq!(canonical_map_hash(&reverse).unwrap(), expected);

    let utf8 = [("é", "utf8"), ("a", "first")];
    assert_eq!(
        canonical_map_hash(&utf8).unwrap(),
        "5abc35bd4fb67be331e74ccfafb6d5aaf0962dfc57a1225fbc9eab3665d26eb9"
    );
    assert!(include_str!("../../fixtures/encoding/canonical-map-v1.json").contains(expected));
}

#[test]
fn duplicate_keys_are_rejected_before_hashing() {
    assert_eq!(
        canonical_map_hash(&[("schema", "one"), ("schema", "two")]),
        Err("duplicate key")
    );
}
