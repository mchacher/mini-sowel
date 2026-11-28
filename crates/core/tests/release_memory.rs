//! Spec 039 R4: a pull holds no more than an upload does: 1 MB pulled through a fetcher that
//! generates its body and an adapter that keeps no byte peaks under 32 KB of heap (the manifest
//! and the installation's buffer included). One test only in this file, so the counting allocator
//! sees nothing else.

#[path = "support/update_fake.rs"]
mod update_fake;

use std::io::{self, Read};

use ed25519_dalek::{Signer, SigningKey};
use peak_alloc::PeakAlloc;
use sha2::{Digest, Sha256};
use solite_core::backup::Pacing;
use solite_core::home::Home;
use solite_core::update::release::{DEFAULT_MANIFEST_URL, Fetched, Fetcher, Limits};
use solite_core::update::{HEADER_BYTES, Updates, Version, signed_bytes};
use update_fake::FakeAdapter;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

const IMAGE_BYTES: usize = 1024 * 1024;
const PEAK_BOUND: usize = 32 * 1024;
const T0: u64 = 1_795_252_323_120;
const ASSET: &str = "https://example.com/solite-9.9.0.sup";

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
        let count = out.len().min(total - self.position).min(1460);
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

struct Source {
    manifest: String,
    header: [u8; HEADER_BYTES],
}

impl Fetcher for Source {
    fn get(&mut self, url: &str, _idle_ms: u64) -> Result<Fetched, String> {
        if url == DEFAULT_MANIFEST_URL {
            return Ok(Fetched::Body {
                length: Some(self.manifest.len() as u64),
                etag: None,
                body: Box::new(io::Cursor::new(self.manifest.clone().into_bytes())),
            });
        }
        assert_eq!(url, ASSET);
        Ok(Fetched::Body {
            length: Some((HEADER_BYTES + IMAGE_BYTES) as u64),
            etag: None,
            body: Box::new(Stream {
                header: self.header,
                position: 0,
            }),
        })
    }
}

struct Steady;

impl Pacing for Steady {
    fn now_ms(&mut self) -> u64 {
        1_000
    }

    fn pause(&mut self) {}

    fn wait_ms(&mut self, _ms: u64) {}
}

#[test]
fn a_one_megabyte_pull_peaks_under_32_kb_of_heap() {
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
        Version::new(9, 9, 0),
        u32::try_from(IMAGE_BYTES).unwrap(),
        &digest,
    );
    let mut header = [0u8; HEADER_BYTES];
    header[..64].copy_from_slice(&signed);
    header[64..].copy_from_slice(&signing.sign(&signed).to_bytes());
    // The file's own hash, as the manifest carries it.
    let mut file_hash = Sha256::new();
    file_hash.update(header);
    (0..IMAGE_BYTES).for_each(|i| file_hash.update([byte(i)]));
    let file_digest: String = file_hash
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let manifest = format!(
        r#"{{"format":1,"version":"9.9.0","minVersion":null,"image":{{"size":{},"sha256":"{file_digest}","url":"{ASSET}"}}}}"#,
        HEADER_BYTES + IMAGE_BYTES
    );

    let (updates, _) = Updates::from_key_text(Some(&public));
    let home = Home::new(T0);
    let ticket = home.release_ticket(false, T0).unwrap();
    let mut adapter = FakeAdapter::new();
    adapter.keep = false;
    adapter.calls = Vec::with_capacity(1024);
    let mut source = Source { manifest, header };
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();
    let response = ticket.pull(
        &updates,
        &mut source,
        &mut adapter,
        &mut Steady,
        Limits::CHECK,
        Limits::DOWNLOAD,
    );
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);
    assert_eq!(response.status, 200, "{:?}", response.body);
    assert_eq!(adapter.written, IMAGE_BYTES);
    println!(
        "update: 1 MB pulled, peak heap {peak} bytes ({} KB), bound {PEAK_BOUND}",
        peak / 1024
    );
    assert!(peak < PEAK_BOUND, "peak {peak} bytes");
}
