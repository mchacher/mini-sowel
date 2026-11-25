//! Spec 032 R4.3: an installation holds a header, one buffer and a hash state, whatever the
//! image's size: 1 MB through an adapter that keeps no byte peaks under 32 KB of heap. One test
//! only in this file, so the counting allocator sees nothing else.

#[path = "support/update_fake.rs"]
mod update_fake;

use std::io::{self, Read};

use ed25519_dalek::{Signer, SigningKey};
use peak_alloc::PeakAlloc;
use sha2::{Digest, Sha256};
use solite_core::update::{HEADER_BYTES, Updates, Version, signed_bytes};
use update_fake::FakeAdapter;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

const IMAGE_BYTES: usize = 1024 * 1024;
const PEAK_BOUND: usize = 32 * 1024;

fn byte(index: usize) -> u8 {
    u8::try_from((index * 31 + index / 7) % 256).unwrap()
}

/// The header, then the image, made on the fly: nothing is held.
struct Stream {
    header: [u8; HEADER_BYTES],
    position: usize,
}

impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let total = HEADER_BYTES + IMAGE_BYTES;
        let count = out.len().min(total - self.position);
        for (i, slot) in out[..count].iter_mut().enumerate() {
            let at = self.position + i;
            *slot = if at < HEADER_BYTES {
                self.header[at]
            } else {
                byte(at - HEADER_BYTES)
            };
        }
        self.position += count;
        Ok(count)
    }
}

#[test]
fn a_one_megabyte_installation_peaks_under_32_kb_of_heap() {
    let mut seed = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut seed))
        .unwrap();
    let signing = SigningKey::from_bytes(&seed);
    let public: String = signing
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut hash = Sha256::new();
    (0..IMAGE_BYTES).for_each(|i| hash.update([byte(i)]));
    let digest: [u8; 32] = hash.finalize().into();
    let signed = signed_bytes(
        Version::new(0, 2, 0),
        u32::try_from(IMAGE_BYTES).unwrap(),
        &digest,
    );
    let mut header = [0u8; HEADER_BYTES];
    header[..64].copy_from_slice(&signed);
    header[64..].copy_from_slice(&signing.sign(&signed).to_bytes());

    let (updates, _) = Updates::from_key_text(Some(&public));
    let mut adapter = FakeAdapter::new();
    adapter.keep = false;
    adapter.calls = Vec::with_capacity(1024);
    let mut source = Stream {
        header,
        position: 0,
    };
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();
    let response = updates.install(&mut source, &mut adapter);
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);
    assert_eq!(response.status, 200, "{:?}", response.body);
    assert_eq!(adapter.written, IMAGE_BYTES);
    println!(
        "update: 1 MB installed, peak heap {peak} bytes ({} KB), bound {PEAK_BOUND}",
        peak / 1024
    );
    assert!(peak < PEAK_BOUND, "peak {peak} bytes");
}
