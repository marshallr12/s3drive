//! SHA256 for bounded private proof metadata only. Domain bytes, serialization,
//! field order and persisted digests remain unchanged. Payload hashes and the
//! multipart/authority whole-file streaming contexts deliberately stay on sha2:
//! ring rejects messages above 2^64-1 bits, while those APIs admit larger sizes.
//! This context contains inline state and a fixed pending buffer, not an EVP heap
//! allocation. It is not a validated-proof cache and retains no database data.

pub(crate) struct ProofSha256(ring::digest::Context);
impl ProofSha256 {
    pub(crate) fn new() -> Self {
        Self(ring::digest::Context::new(&ring::digest::SHA256))
    }
    pub(crate) fn update(&mut self, bytes: impl AsRef<[u8]>) {
        self.0.update(bytes.as_ref());
    }
    pub(crate) fn finalize(self) -> [u8; 32] {
        // SHA256's output length is fixed by the constructor, never caller input.
        self.0.finish().as_ref().try_into().expect("SHA256 output")
    }
}
impl Default for ProofSha256 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub(crate) fn reference_json<T: serde::Serialize>(domain: &[u8], value: &T) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut old = Sha256::new();
    old.update(domain);
    old.update(serde_json::to_vec(value).unwrap());
    old.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn native_retry_validation_work_proof_hash_matches_independent_sha2() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<ProofSha256>();
        for (input, expected) in [
            (
                b"".as_slice(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".as_slice(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".as_slice(),
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
        ] {
            let mut h = ProofSha256::new();
            h.update(input);
            let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, expected);
        }
        let bytes: Vec<u8> = (0..1_048_576)
            .map(|i| ((i * 37 + 11) % 256) as u8)
            .collect();
        for len in [0, 17, 55, 56, 63, 64, 65, 256, 4096, bytes.len()] {
            for domain in [
                b"".as_slice(),
                b"s3drive-retention-anchor-v1\0",
                b"s3drive-open-input-v1\0",
            ] {
                for chunk in [1, 17, 64, 4093, bytes.len()] {
                    let mut old = Sha256::new();
                    let mut new = ProofSha256::new();
                    old.update(domain);
                    new.update(domain);
                    for part in bytes[..len].chunks(chunk) {
                        old.update(part);
                        new.update(part);
                        new.update([]);
                    }
                    assert_eq!(new.finalize().as_slice(), old.finalize().as_slice());
                }
            }
        }
        // Default reset is exact and does not retain any prior message state.
        let mut h = ProofSha256::default();
        for input in [b"prefix".as_slice(), b"", b"independent next part"] {
            h.update(input);
            assert_eq!(
                std::mem::take(&mut h).finalize().as_slice(),
                Sha256::digest(input).as_slice()
            );
        }
    }
}
