//! Diagnostic only: no production hash choice or build profile is changed.
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};

fn hash(backend: usize, domain: &[u8], data: &[u8], chunk: usize) -> [u8; 32] {
    match backend {
        0 => {
            let mut h = Sha256::new();
            h.update(b"");
            h.update(domain);
            for part in data.chunks(chunk) {
                h.update(part);
            }
            h.finalize().into()
        }
        1 => {
            let mut h = ring::digest::Context::new(&ring::digest::SHA256);
            h.update(b"");
            h.update(domain);
            for part in data.chunks(chunk) {
                h.update(part);
            }
            h.finish().as_ref().try_into().unwrap()
        }
        2 => {
            let mut h = aws_lc_rs::digest::Context::new(&aws_lc_rs::digest::SHA256);
            h.update(b"");
            h.update(domain);
            for part in data.chunks(chunk) {
                h.update(part);
            }
            h.finish().as_ref().try_into().unwrap()
        }
        _ => unreachable!(),
    }
}

#[test]
#[ignore = "bounded native-runner measurement, not performance acceptance"]
fn sha256_backend_equivalence_and_debug_cost() {
    assert!(
        black_box(cfg!(debug_assertions)),
        "use the unchanged ordinary debug profile"
    );
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    eprintln!(
        "SHA256_CPU arch={} sha={} sse41={} ssse3={} avx2={}",
        std::env::consts::ARCH,
        std::is_x86_feature_detected!("sha"),
        std::is_x86_feature_detected!("sse4.1"),
        std::is_x86_feature_detected!("ssse3"),
        std::is_x86_feature_detected!("avx2")
    );
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    eprintln!(
        "SHA256_CPU arch={} x86_features=unavailable",
        std::env::consts::ARCH
    );
    // Published SHA-256 empty-message vector is independent of all three libraries.
    let empty = [
        0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9,
        0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52,
        0xb8, 0x55,
    ];
    for backend in 0..3 {
        assert_eq!(hash(backend, b"", b"", 1), empty);
    }
    for (input, hex) in [
        (
            b"abc".as_slice(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".as_slice(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ] {
        let expected: Vec<u8> = hex
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        for backend in 0..3 {
            for chunk in [1, 17, 64] {
                assert_eq!(hash(backend, b"", input, chunk).as_slice(), expected);
            }
        }
    }
    let domain = b"s3drive-sha256-backend-diagnostic-v1\0";
    let names = ["sha2-0.11.1", "ring-0.17.14", "aws-lc-rs-1.18.2"];
    for len in [0, 17, 55, 56, 63, 64, 65, 256, 1024, 4096, 16384, 65536] {
        let data: Vec<u8> = (0..len).map(|i| ((i * 37 + 11) % 256) as u8).collect();
        // Also exercise exact total-message padding boundaries without the domain.
        let unprefixed: [u8; 32] = Sha256::digest(&data).into();
        for backend in 0..3 {
            for chunk in [1, 17, 64] {
                assert_eq!(hash(backend, b"", &data, chunk), unprefixed);
            }
        }
        let mut full = domain.to_vec();
        full.extend_from_slice(&data);
        let expected: [u8; 32] = Sha256::digest(&full).into();
        assert_eq!(
            ring::digest::digest(&ring::digest::SHA256, &full).as_ref(),
            expected
        );
        assert_eq!(
            aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, &full).as_ref(),
            expected
        );
        let mut chunks = vec![len.max(1), 17];
        if len <= 256 {
            chunks.push(1);
        }
        chunks.sort_unstable();
        chunks.dedup();
        for chunk in chunks {
            // Rotate order, report all rounds, impose no speed threshold.
            for round in 0..3 {
                for offset in 0..3 {
                    let backend = (round + offset) % 3;
                    assert_eq!(hash(backend, domain, &data, chunk), expected);
                    let start = Instant::now();
                    for _ in 0..128 {
                        let got = hash(backend, black_box(domain), black_box(&data), chunk);
                        assert_eq!(black_box(got), expected);
                    }
                    eprintln!(
                        "SHA256_COST backend={} bytes={} chunk={} round={} hashes=128 elapsed_us={}",
                        names[backend],
                        len,
                        chunk,
                        round,
                        start.elapsed().as_micros()
                    );
                }
            }
        }
    }
}
