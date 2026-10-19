//! Untrusted persisted manifest-base identity. No database, provider access,
//! historical lookup, authorization, content handle or write capability.
use super::{Error, Result};
use std::fmt;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const MAGIC: &[u8; 8] = b"S3DBASE1";
pub const ENCODED_BYTES: usize = 88;

/// Exact historical version identity, without credentials, S3 keys or cached
/// namespace attributes. Its reference is the original edit CAS expected value,
/// not the predecessor guard stored in the payload's reservation.
#[derive(Clone, PartialEq, Eq)]
pub struct Descriptor {
    volume: String,
    node: String,
    reservation: String,
    reference: String,
}
impl fmt::Debug for Descriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestBaseIdentity([untrusted intent])")
    }
}
impl Descriptor {
    /// Construct structural identity intent only. This does not establish that
    /// the version exists, is authorized or may be published/acknowledged.
    pub fn from_hex(volume: &str, node: &str, reservation: &str, reference: &str) -> Result<Self> {
        for (field, length) in [(volume, 32), (node, 32), (reservation, 32), (reference, 64)] {
            if field.len() != length
                || !field
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::Encoding);
            }
        }
        if [volume, node, reservation].contains(&"00000000000000000000000000000000") {
            return Err(Error::Binding);
        }
        Ok(Self {
            volume: volume.into(),
            node: node.into(),
            reservation: reservation.into(),
            reference: reference.into(),
        })
    }

    pub fn volume(&self) -> &str {
        &self.volume
    }
    pub fn node(&self) -> &str {
        &self.node
    }
    pub fn reservation(&self) -> &str {
        &self.reservation
    }
    pub fn payload_reference(&self) -> &str {
        &self.reference
    }
    pub fn encode(&self) -> [u8; ENCODED_BYTES] {
        let mut out = [0; ENCODED_BYTES];
        out[..8].copy_from_slice(MAGIC);
        let mut at = 8;
        for field in [&self.volume, &self.node, &self.reservation, &self.reference] {
            for pair in field.as_bytes().as_chunks::<2>().0 {
                // Private constructors establish canonical lowercase hex.
                let nibble = |b: u8| {
                    if b.is_ascii_digit() {
                        b - b'0'
                    } else {
                        b - b'a' + 10
                    }
                };
                out[at] = (nibble(pair[0]) << 4) | nibble(pair[1]);
                at += 1;
            }
        }
        out
    }
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() != ENCODED_BYTES || &encoded[..8] != MAGIC {
            return Err(Error::Encoding);
        }
        let value = Self {
            volume: hex(&encoded[8..24]),
            node: hex(&encoded[24..40]),
            reservation: hex(&encoded[40..56]),
            reference: hex(&encoded[56..88]),
        };
        if [&value.volume, &value.node, &value.reservation]
            .iter()
            .any(|id| id.as_str() == "00000000000000000000000000000000")
        {
            return Err(Error::Binding);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const V: &str = "00112233445566778899aabbccddeeff";
    const N: &str = "102132435465768798a9bacbdcedfe0f";
    const R: &str = "ffeeddccbbaa99887766554433221100";
    const P: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const GOLDEN: [u8; 88] = [
        0x53, 0x33, 0x44, 0x42, 0x41, 0x53, 0x45, 0x31, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x10, 0x21, 0x32, 0x43, 0x54, 0x65,
        0x76, 0x87, 0x98, 0xa9, 0xba, 0xcb, 0xdc, 0xed, 0xfe, 0x0f, 0xff, 0xee, 0xdd, 0xcc, 0xbb,
        0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00, 0x01, 0x23, 0x45, 0x67,
        0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45,
        0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
    ];
    #[test]
    fn exact_persisted_vector_preserves_original_identity_and_redaction() {
        let d = Descriptor::from_hex(V, N, R, P).unwrap();
        assert_eq!(d.encode(), GOLDEN);
        let loaded = Descriptor::decode(&GOLDEN).unwrap();
        assert_eq!(loaded, d);
        assert_eq!(
            (
                loaded.volume(),
                loaded.node(),
                loaded.reservation(),
                loaded.payload_reference()
            ),
            (V, N, R, P)
        );
        let debug = format!("{loaded:?}");
        for field in [V, N, R, P] {
            assert!(!debug.contains(field));
        }
    }
    #[test]
    fn malformed_persisted_shape_and_reserved_ids_are_refused_without_authorizing_content() {
        for length in 0..88 {
            assert_eq!(Descriptor::decode(&GOLDEN[..length]), Err(Error::Encoding));
        }
        let mut extra = GOLDEN.to_vec();
        extra.push(0);
        assert_eq!(Descriptor::decode(&extra), Err(Error::Encoding));
        for at in 0..8 {
            let mut changed = GOLDEN;
            changed[at] ^= 1;
            assert_eq!(Descriptor::decode(&changed), Err(Error::Encoding));
        }
        for start in [8, 24, 40] {
            let mut changed = GOLDEN;
            changed[start..start + 16].fill(0);
            assert_eq!(Descriptor::decode(&changed), Err(Error::Binding));
        }
        // A different nonzero identity or zero reference remains untrusted
        // intent. Only the authority-backed reader may authorize its content.
        let mut changed = GOLDEN;
        changed[24] ^= 1;
        assert_ne!(
            Descriptor::decode(&changed).unwrap(),
            Descriptor::decode(&GOLDEN).unwrap()
        );
        changed[56..88].fill(0);
        assert_eq!(
            Descriptor::decode(&changed).unwrap().payload_reference(),
            "0".repeat(64)
        );
    }
    #[test]
    fn hex_constructor_refuses_noncanonical_fields_and_reserved_identity() {
        for index in 0..4 {
            let length = if index == 3 { 64 } else { 32 };
            for bad in [
                String::new(),
                "A".repeat(length),
                "g".repeat(length),
                "é".repeat(length / 2),
                "0".repeat(length - 1),
                "0".repeat(length + 1),
            ] {
                let mut fields = [V, N, R, P];
                fields[index] = &bad;
                assert_eq!(
                    Descriptor::from_hex(fields[0], fields[1], fields[2], fields[3]),
                    Err(Error::Encoding)
                );
            }
        }
        let zero = "0".repeat(32);
        for index in 0..3 {
            let mut fields = [V, N, R, P];
            fields[index] = &zero;
            assert_eq!(
                Descriptor::from_hex(fields[0], fields[1], fields[2], fields[3]),
                Err(Error::Binding)
            );
        }
    }
}
