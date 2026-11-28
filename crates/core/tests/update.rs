//! Signed updates (spec 032): the header's checks, the streamed installation through a fake
//! adapter (refusals, power cuts, one at a time), the public key and the versions, the probation,
//! and the routes behind the gate. Key pairs are made here from random bytes: no key is written in
//! any file.

mod support;

use std::io::{self, Cursor, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use solite_core::api::{Reach, Reply, Request, handle, handle_guarded};
use solite_core::auth::{Account, Language, PasswordHash};
use solite_core::devices::Id;
use solite_core::home::Home;
use solite_core::update::{
    CHUNK_BYTES, HEADER_BYTES, Health, Probation, Updates, Verdict, Version, check_header,
    parse_public_key, signed_bytes,
};
use support::update_fake::FakeAdapter;
use support::{HOST, PORT, T0, counter};

// ---- Keys, files and readers ----

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("random bytes");
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A key pair made now, and the public key as the build would carry it.
struct Keys {
    signing: SigningKey,
    public_hex: String,
}

fn keys() -> Keys {
    let signing = SigningKey::from_bytes(&random_bytes::<32>());
    let public_hex = hex(&signing.verifying_key().to_bytes());
    Keys {
        signing,
        public_hex,
    }
}

fn updates(keys: &Keys) -> Updates {
    let (updates, line) = Updates::from_key_text(Some(&keys.public_hex));
    assert!(updates.has_key(), "{line}");
    updates
}

fn running() -> Version {
    Version::parse(solite_core::VERSION).unwrap()
}

fn newer() -> Version {
    Version::new(9, 9, 0)
}

/// An image of `len` bytes that is not all the same.
fn image(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 7 + i / 251 + 3) % 256).unwrap())
        .collect()
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A header of 128 bytes signed with `keys`, for these fields (the digest and the length are not
/// forced to match any image).
fn header(keys: &Keys, version: Version, length: u32, digest: &[u8; 32]) -> Vec<u8> {
    let signed = signed_bytes(version, length, digest);
    let mut out = signed.to_vec();
    out.extend_from_slice(&keys.signing.sign(&signed).to_bytes());
    assert_eq!(out.len(), HEADER_BYTES);
    out
}

/// The signed update file of R1: the header, then the image.
fn file(keys: &Keys, version: Version, image: &[u8]) -> Vec<u8> {
    let mut out = header(
        keys,
        version,
        u32::try_from(image.len()).unwrap(),
        &sha256(image),
    );
    out.extend_from_slice(image);
    out
}

fn install(updates: &Updates, bytes: &[u8], adapter: &mut FakeAdapter) -> (u16, Value) {
    install_from(updates, &mut Cursor::new(bytes.to_vec()), adapter)
}

fn install_from(
    updates: &Updates,
    source: &mut dyn Read,
    adapter: &mut FakeAdapter,
) -> (u16, Value) {
    let response = updates.install(source, adapter);
    let body = response.body.expect("a body");
    (response.status, serde_json::from_str(&body).unwrap())
}

fn refused((status, body): &(u16, Value), expected_status: u16, message: &str) {
    assert_eq!(*status, expected_status, "{body}");
    assert_eq!(body, &json!({ "error": message }));
}

/// Nothing bootable was left: the boot record is unchanged and the slot holds nothing.
fn untouched(adapter: &FakeAdapter) {
    assert!(!adapter.next_boot_is_inactive);
    assert!(!adapter.pending);
    assert!(adapter.slot.is_empty());
}

/// Reads its bytes, then fails like a client that vanished.
struct FailAfter {
    data: Cursor<Vec<u8>>,
    left: usize,
    panic: bool,
}

impl Read for FailAfter {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            if self.panic {
                panic!("the installer is dropped mid-image");
            }
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, "gone"));
        }
        let take = out.len().min(self.left);
        let count = self.data.read(&mut out[..take])?;
        self.left -= count;
        Ok(count)
    }
}

/// Gives a few bytes at a time, as a socket does.
struct Trickle {
    data: Cursor<Vec<u8>>,
    step: usize,
}

impl Read for Trickle {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let take = out.len().min(self.step);
        self.data.read(&mut out[..take])
    }
}

// ---- The installation ----

#[test]
fn a_valid_file_is_installed_into_the_inactive_slot_and_booted_on_probation() {
    let keys = keys();
    let updates = updates(&keys);
    let image = image(10_000);
    let mut adapter = FakeAdapter::new();
    let (status, body) = install(&updates, &file(&keys, newer(), &image), &mut adapter);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({"success": true, "message": "Installed 9.9.0, restarting"})
    );
    assert_eq!(adapter.slot, image);
    assert_eq!(adapter.count("finish"), 1);
    assert!(adapter.next_boot_is_inactive);
    assert!(adapter.pending);
    assert_eq!(adapter.count("abort"), 0);
}

#[test]
fn a_signature_by_another_key_is_refused_before_anything_is_written() {
    let (ours, other) = (keys(), keys());
    let updates = updates(&ours);
    let mut adapter = FakeAdapter::new();
    let answer = install(&updates, &file(&other, newer(), &image(5000)), &mut adapter);
    refused(&answer, 400, "Invalid signature");
    assert_eq!(adapter.count("begin"), 0);
    untouched(&adapter);
}

/// The identity point: a small-order public key. With R the identity and S = 0, the signature
/// `(R, S)` satisfies the plain equation for any message; only `verify_strict` refuses it (weak
/// key), which is what keeps a forged header out.
#[test]
fn a_signature_that_only_plain_verification_accepts_is_refused() {
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let weak_key = VerifyingKey::from_bytes(&identity).expect("the identity is a valid point");
    let honest = keys();
    let mut bytes: [u8; HEADER_BYTES] = header(&honest, newer(), 5000, &sha256(&image(5000)))
        .try_into()
        .unwrap();
    bytes[64..].fill(0);
    bytes[64] = 1; // R = identity, S = 0
    let refusal = check_header(&bytes, &weak_key, running(), 1 << 20).unwrap_err();
    assert_eq!(
        (refusal.status, refusal.message.as_str()),
        (400, "Invalid signature")
    );
    // The same bytes pass plain verification: the test proves the strict check is what refused.
    let signature = ed25519_dalek::Signature::from_bytes(bytes[64..].try_into().unwrap());
    assert!(weak_key.verify(&bytes[..64], &signature).is_ok());
}

#[test]
fn a_header_byte_changed_after_signing_is_an_invalid_signature() {
    let keys = keys();
    let updates = updates(&keys);
    let mut bytes = file(&keys, newer(), &image(5000));
    bytes[16] += 1; // the patch: still newer, no longer what was signed
    let mut adapter = FakeAdapter::new();
    refused(
        &install(&updates, &bytes, &mut adapter),
        400,
        "Invalid signature",
    );
    assert!(adapter.calls.is_empty());
}

#[test]
fn an_image_byte_changed_after_signing_does_not_match_its_signature_and_is_aborted() {
    let keys = keys();
    let updates = updates(&keys);
    let mut bytes = file(&keys, newer(), &image(9000));
    bytes[HEADER_BYTES + 4500] ^= 0x01;
    let mut adapter = FakeAdapter::new();
    refused(
        &install(&updates, &bytes, &mut adapter),
        400,
        "Image does not match its signature",
    );
    assert_eq!(adapter.count("abort"), 1);
    untouched(&adapter);
}

#[test]
fn a_file_cut_in_the_image_is_truncated_and_aborted() {
    let keys = keys();
    let updates = updates(&keys);
    let bytes = file(&keys, newer(), &image(9000));
    let mut adapter = FakeAdapter::new();
    refused(
        &install(&updates, &bytes[..HEADER_BYTES + 4000], &mut adapter),
        400,
        "Truncated update",
    );
    assert_eq!(adapter.count("abort"), 1);
    untouched(&adapter);
}

#[test]
fn fewer_than_128_bytes_or_nothing_is_truncated_and_nothing_is_written() {
    let keys = keys();
    let updates = updates(&keys);
    let bytes = file(&keys, newer(), &image(500));
    for cut in [100, 0] {
        let mut adapter = FakeAdapter::new();
        refused(
            &install(&updates, &bytes[..cut], &mut adapter),
            400,
            "Truncated update",
        );
        assert!(adapter.calls.is_empty());
    }
}

#[test]
fn one_byte_more_than_declared_is_refused_and_aborted() {
    let keys = keys();
    let updates = updates(&keys);
    let mut bytes = file(&keys, newer(), &image(5000));
    bytes.push(0);
    let mut adapter = FakeAdapter::new();
    refused(
        &install(&updates, &bytes, &mut adapter),
        400,
        "Update longer than declared",
    );
    assert_eq!(adapter.count("abort"), 1);
    untouched(&adapter);
}

#[test]
fn a_version_equal_or_lower_is_not_newer_and_nothing_is_written() {
    let keys = keys();
    let updates = updates(&keys);
    for version in [running(), Version::new(0, 0, 9)] {
        let mut adapter = FakeAdapter::new();
        refused(
            &install(&updates, &file(&keys, version, &image(500)), &mut adapter),
            400,
            &format!(
                "Version {version} is not newer than {}",
                solite_core::VERSION
            ),
        );
        assert!(adapter.calls.is_empty());
    }
}

#[test]
fn an_image_larger_than_the_slot_is_413_before_any_write() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = FakeAdapter::new();
    adapter.capacity = 1000;
    // The header alone: the length it declares is already too long.
    let bytes = header(&keys, newer(), 1001, &sha256(b"x"));
    refused(
        &install(&updates, &bytes, &mut adapter),
        413,
        "Image too large for the update slot",
    );
    assert!(adapter.calls.is_empty());
}

#[test]
fn a_length_of_zero_or_a_reserved_byte_set_is_a_malformed_header() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = FakeAdapter::new();
    let zero = header(&keys, newer(), 0, &sha256(b""));
    refused(
        &install(&updates, &zero, &mut adapter),
        400,
        "Malformed update header",
    );
    for offset in [18, 56, 63] {
        let mut bytes = file(&keys, newer(), &image(500));
        bytes[offset] = 1;
        refused(
            &install(&updates, &bytes, &mut adapter),
            400,
            "Malformed update header",
        );
    }
    assert!(adapter.calls.is_empty());
}

#[test]
fn a_wrong_magic_or_format_is_refused_with_its_own_message() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = FakeAdapter::new();
    let mut bytes = file(&keys, newer(), &image(500));
    bytes[0] = b'X';
    refused(
        &install(&updates, &bytes, &mut adapter),
        400,
        "Not a Solite update",
    );
    let mut bytes = file(&keys, newer(), &image(500));
    bytes[8] = 2;
    refused(
        &install(&updates, &bytes, &mut adapter),
        400,
        "Unsupported update format 2",
    );
    assert!(adapter.calls.is_empty());
}

#[test]
fn a_build_without_a_key_answers_503() {
    let keys = keys();
    let (updates, line) = Updates::from_key_text(None);
    assert_eq!(line, "update: no key, updates refused");
    let mut adapter = FakeAdapter::new();
    refused(
        &install(&updates, &file(&keys, newer(), &image(500)), &mut adapter),
        503,
        "Updates are not configured: this build has no update key",
    );
    assert!(adapter.calls.is_empty());
}

#[test]
fn the_start_line_says_no_key_an_invalid_key_or_the_first_eight_characters() {
    let keys = keys();
    let (updates, line) = Updates::from_key_text(None);
    assert_eq!(
        (updates.has_key(), line.as_str()),
        (false, "update: no key, updates refused")
    );
    let (updates, line) = Updates::from_key_text(Some(&"zz".repeat(32)));
    assert_eq!(
        (updates.has_key(), line.as_str()),
        (false, "update: invalid key, updates refused")
    );
    let (updates, line) = Updates::from_key_text(Some(&keys.public_hex.to_ascii_uppercase()));
    assert!(updates.has_key());
    assert_eq!(line, format!("update: key {}", &keys.public_hex[..8]));
}

#[test]
fn a_reader_that_fails_after_half_the_image_is_a_truncated_update_and_the_state_is_idle() {
    let keys = keys();
    let updates = updates(&keys);
    let bytes = file(&keys, newer(), &image(10_000));
    let mut adapter = FakeAdapter::new();
    let mut cut = FailAfter {
        data: Cursor::new(bytes.clone()),
        left: HEADER_BYTES + 5000,
        panic: false,
    };
    refused(
        &install_from(&updates, &mut cut, &mut adapter),
        400,
        "Truncated update",
    );
    assert_eq!(adapter.count("abort"), 1);
    untouched(&adapter);
    // Idle again: the next installation goes through.
    let (status, body) = install(&updates, &bytes, &mut adapter);
    assert_eq!(status, 200, "{body}");
}

#[test]
fn a_flash_that_refuses_writes_is_a_500_update_failed_and_aborted() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = FakeAdapter::new();
    adapter.fail_writes_after = Some(4096);
    refused(
        &install(
            &updates,
            &file(&keys, newer(), &image(10_000)),
            &mut adapter,
        ),
        500,
        "Update failed: flash write refused",
    );
    assert_eq!(adapter.count("abort"), 1);
    untouched(&adapter);
}

#[test]
fn an_installer_dropped_mid_image_leaves_the_running_image_and_an_idle_state() {
    let keys = keys();
    let updates = updates(&keys);
    let bytes = file(&keys, newer(), &image(10_000));
    let mut adapter = FakeAdapter::new();
    let mut dropped = FailAfter {
        data: Cursor::new(bytes.clone()),
        left: HEADER_BYTES + 5000,
        panic: true,
    };
    let caught = catch_unwind(AssertUnwindSafe(|| {
        install_from(&updates, &mut dropped, &mut adapter)
    }));
    assert!(caught.is_err());
    assert!(!adapter.next_boot_is_inactive && !adapter.pending);
    assert_eq!(adapter.count("finish"), 0);
    // The guard released the state.
    let mut again = FakeAdapter::new();
    let (status, body) = install(&updates, &bytes, &mut again);
    assert_eq!(status, 200, "{body}");
}

#[test]
fn a_finish_refused_by_the_platform_is_a_500_and_aborted() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = FakeAdapter::new();
    adapter.fail_finish = true;
    refused(
        &install(&updates, &file(&keys, newer(), &image(3000)), &mut adapter),
        500,
        "Update failed: image refused by the platform",
    );
    assert_eq!(adapter.count("abort"), 1);
    untouched(&adapter);
}

/// A reader that gives its header, says it is waiting, then blocks until it is released.
struct Blocking {
    head: Cursor<Vec<u8>>,
    waiting: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

impl Read for Blocking {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = self.head.read(out)?;
        if count > 0 {
            return Ok(count);
        }
        let _ = self.waiting.send(());
        let _ = self.release.recv();
        Ok(0)
    }
}

#[test]
fn a_second_installation_while_one_runs_is_409_in_progress() {
    let keys = keys();
    let updates = Arc::new(updates(&keys));
    let bytes = file(&keys, newer(), &image(5000));
    let (waiting, is_waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let first = {
        let updates = Arc::clone(&updates);
        let head = bytes[..HEADER_BYTES + 100].to_vec();
        std::thread::spawn(move || {
            let mut source = Blocking {
                head: Cursor::new(head),
                waiting,
                release: released,
            };
            install_from(&updates, &mut source, &mut FakeAdapter::new())
        })
    };
    is_waiting.recv().unwrap();
    let mut adapter = FakeAdapter::new();
    refused(
        &install(&updates, &bytes, &mut adapter),
        409,
        "Update already in progress",
    );
    assert!(adapter.calls.is_empty());
    release.send(()).unwrap();
    // The first one ended short of its image, and the state is idle again.
    refused(&first.join().unwrap(), 400, "Truncated update");
    let (status, body) = install(&updates, &bytes, &mut adapter);
    assert_eq!(status, 200, "{body}");
}

#[test]
fn an_installation_after_a_successful_one_is_409_until_the_restart() {
    let keys = keys();
    let updates = updates(&keys);
    let bytes = file(&keys, newer(), &image(3000));
    let mut adapter = FakeAdapter::new();
    assert_eq!(install(&updates, &bytes, &mut adapter).0, 200);
    adapter.pending = false; // even once the record says nothing is pending
    let calls = adapter.calls.len();
    refused(
        &install(&updates, &bytes, &mut adapter),
        409,
        "An update is installed and waits for the restart",
    );
    assert_eq!(adapter.calls.len(), calls);
}

#[test]
fn an_installation_while_the_running_image_is_on_probation_is_409() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = FakeAdapter::new();
    adapter.pending = true;
    refused(
        &install(&updates, &file(&keys, newer(), &image(3000)), &mut adapter),
        409,
        "The running image is not confirmed yet",
    );
    assert!(adapter.calls.is_empty());
}

#[test]
fn the_image_is_written_in_chunks_of_at_most_4096_bytes_whatever_the_reads() {
    let keys = keys();
    let image = image(3 * CHUNK_BYTES + 123);
    let bytes = file(&keys, newer(), &image);
    for step in [7, 1000, 100_000] {
        let updates = updates(&keys);
        let mut adapter = FakeAdapter::new();
        let mut source = Trickle {
            data: Cursor::new(bytes.clone()),
            step,
        };
        let (status, body) = install_from(&updates, &mut source, &mut adapter);
        assert_eq!(status, 200, "step {step}: {body}");
        assert!(adapter.max_write <= CHUNK_BYTES, "step {step}");
        assert_eq!(adapter.slot, image);
        // Chunks are filled whatever the reads were: four writes for this image.
        assert_eq!(adapter.count("write"), 4, "step {step}");
    }
}

// ---- The public key and the versions ----

#[test]
fn a_public_key_is_64_hex_characters_of_a_valid_strong_point() {
    let keys = keys();
    assert!(parse_public_key(&keys.public_hex).is_some());
    assert!(parse_public_key(&keys.public_hex.to_ascii_uppercase()).is_some());
    assert!(parse_public_key(&keys.public_hex[..63]).is_none());
    assert!(parse_public_key(&format!("{}0", keys.public_hex)).is_none());
    assert!(parse_public_key(&"zz".repeat(32)).is_none());
    // A 32-byte value that is not a point of the curve: found from random bytes.
    let not_a_point = loop {
        let bytes = random_bytes::<32>();
        if VerifyingKey::from_bytes(&bytes).is_err() {
            break hex(&bytes);
        }
    };
    assert!(parse_public_key(&not_a_point).is_none());
}

#[test]
fn a_small_order_public_key_is_refused() {
    // The identity point (y = 1) and the point of order 2 (y = -1): public, weak, never a key.
    let identity = format!("01{}", "00".repeat(31));
    let order_two = format!("ecff{}7f", "ff".repeat(29));
    assert!(VerifyingKey::from_bytes(&parse_hex32(&identity)).is_ok());
    assert!(parse_public_key(&identity).is_none());
    assert!(parse_public_key(&order_two).is_none());
}

fn parse_hex32(text: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap();
    }
    out
}

#[test]
fn versions_parse_exactly_three_numbers_and_compare_as_tuples() {
    assert_eq!(Version::parse("0.2.0"), Some(Version::new(0, 2, 0)));
    assert!(Version::new(0, 2, 0) > Version::new(0, 1, 9));
    assert!(Version::new(1, 0, 0) > Version::new(0, 99, 99));
    assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
    for bad in [
        "1.2",
        "1.2.3.4",
        "70000.0.0",
        "1.2.x",
        "",
        "1..3",
        "-1.0.0",
        "1.2.3-rc1",
        "+1.2.3",
    ] {
        assert_eq!(Version::parse(bad), None, "{bad}");
    }
}

// ---- The probation ----

fn tick(updates: &Updates, adapter: &mut FakeAdapter, seconds: u64, healthy: bool) {
    tick_with(updates, adapter, seconds, health(healthy, healthy, true));
}

fn tick_with(updates: &Updates, adapter: &mut FakeAdapter, seconds: u64, health: Health) {
    updates.watch(adapter, seconds * 1000, health);
}

fn health(engine: bool, api: bool, psram: bool) -> Health {
    Health { engine, api, psram }
}

fn pending_adapter() -> FakeAdapter {
    let mut adapter = FakeAdapter::new();
    adapter.pending = true;
    adapter
}

#[test]
fn a_program_not_on_probation_never_confirms_or_rolls_back() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = FakeAdapter::new();
    for seconds in 0..=400 {
        tick(&updates, &mut adapter, seconds, seconds % 2 == 0);
    }
    assert!(adapter.calls.is_empty());
}

#[test]
fn healthy_from_5_seconds_confirms_at_65_once_and_installations_are_accepted_after() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = pending_adapter();
    for seconds in 0..=64 {
        tick(&updates, &mut adapter, seconds, seconds >= 5);
        assert_eq!(adapter.count("confirm"), 0, "at {seconds} s");
    }
    tick(&updates, &mut adapter, 65, true);
    assert_eq!(adapter.count("confirm"), 1);
    for seconds in 66..=200 {
        tick(&updates, &mut adapter, seconds, true);
    }
    assert_eq!(adapter.count("confirm"), 1);
    assert_eq!(adapter.count("roll_back"), 0);
    assert!(!adapter.pending);
    let (status, body) = install(&updates, &file(&keys, newer(), &image(3000)), &mut adapter);
    assert_eq!(status, 200, "{body}");
}

#[test]
fn never_healthy_rolls_back_at_300_seconds_once() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..=299 {
        tick(&updates, &mut adapter, seconds, false);
    }
    assert_eq!(adapter.count("roll_back"), 0);
    for seconds in 300..=400 {
        tick(&updates, &mut adapter, seconds, false);
    }
    assert_eq!(adapter.count("roll_back"), 1);
    assert_eq!(adapter.count("confirm"), 0);
    assert!(!adapter.pending);
}

#[test]
fn a_break_in_health_restarts_the_minute() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..=30 {
        tick(&updates, &mut adapter, seconds, true);
    }
    for seconds in 31..=99 {
        tick(&updates, &mut adapter, seconds, false);
    }
    for seconds in 100..=159 {
        tick(&updates, &mut adapter, seconds, true);
        assert_eq!(adapter.count("confirm"), 0, "at {seconds} s");
    }
    tick(&updates, &mut adapter, 160, true);
    assert_eq!(adapter.count("confirm"), 1);
}

#[test]
fn the_deadline_wins_over_a_confirmation_that_is_due_later() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..280 {
        tick(&updates, &mut adapter, seconds, false);
    }
    for seconds in 280..=300 {
        tick(&updates, &mut adapter, seconds, true);
    }
    assert_eq!(adapter.count("roll_back"), 1);
    assert_eq!(adapter.count("confirm"), 0);
}

#[test]
fn healthy_with_the_psram_for_60_seconds_confirms() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..60 {
        tick_with(&updates, &mut adapter, seconds, health(true, true, true));
        assert_eq!(adapter.count("confirm"), 0, "at {seconds} s");
    }
    tick_with(&updates, &mut adapter, 60, health(true, true, true));
    assert_eq!(adapter.count("confirm"), 1);
    assert_eq!(adapter.count("roll_back"), 0);
}

#[test]
fn an_engine_and_an_api_up_without_the_psram_roll_back_at_300_seconds() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..=299 {
        tick_with(&updates, &mut adapter, seconds, health(true, true, false));
    }
    assert_eq!(adapter.count("roll_back"), 0);
    assert_eq!(adapter.count("confirm"), 0);
    tick_with(&updates, &mut adapter, 300, health(true, true, false));
    assert_eq!(adapter.count("roll_back"), 1);
    assert_eq!(adapter.count("confirm"), 0);
}

#[test]
fn the_psram_present_with_the_engine_down_is_not_healthy() {
    assert!(!health(false, true, true).healthy());
    assert!(!health(true, false, true).healthy());
    assert!(!health(true, true, false).healthy());
    assert!(health(true, true, true).healthy());
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..=100 {
        tick_with(&updates, &mut adapter, seconds, health(false, true, true));
    }
    assert_eq!(adapter.count("confirm"), 0);
}

#[test]
fn the_psram_missing_after_30_seconds_then_back_starts_the_60_seconds_again() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    for seconds in 0..=30 {
        tick_with(&updates, &mut adapter, seconds, health(true, true, true));
    }
    tick_with(&updates, &mut adapter, 31, health(true, true, false));
    for seconds in 32..=91 {
        tick_with(&updates, &mut adapter, seconds, health(true, true, true));
        assert_eq!(adapter.count("confirm"), 0, "at {seconds} s");
    }
    tick_with(&updates, &mut adapter, 92, health(true, true, true));
    assert_eq!(adapter.count("confirm"), 1);
}

#[test]
fn the_reason_of_an_unhealthy_probation_is_reported_once_per_change() {
    let mut probation = Probation::new(0);
    let mut reported = Vec::new();
    for second in 0..300u64 {
        // No PSRAM for 100 s, then the engine down for 100 s, then healthy, then no PSRAM again.
        let state = match second {
            0..100 => health(true, true, false),
            100..200 => health(false, true, true),
            200..250 => health(true, true, true),
            _ => health(true, true, false),
        };
        probation.tick(second * 1000, state);
        if let Some(reason) = probation.reason_changed(state) {
            reported.push((second, reason));
        }
    }
    assert_eq!(
        reported,
        [(0, "no PSRAM"), (100, "engine stopped"), (250, "no PSRAM")]
    );
    assert_eq!(
        probation.tick(300_000, health(true, true, true)),
        Verdict::RollBack
    );
}

#[test]
fn a_confirm_that_fails_is_tried_again_at_the_next_tick() {
    let (updates, _) = Updates::from_key_text(None);
    let mut adapter = pending_adapter();
    adapter.confirm_failures = 1;
    for seconds in 0..=60 {
        tick(&updates, &mut adapter, seconds, true);
    }
    assert_eq!(adapter.count("confirm"), 1);
    assert!(adapter.pending);
    tick(&updates, &mut adapter, 61, true);
    assert_eq!(adapter.count("confirm"), 2);
    assert!(!adapter.pending);
    tick(&updates, &mut adapter, 62, true);
    assert_eq!(adapter.count("confirm"), 2);
}

#[test]
fn a_roll_back_that_fails_is_tried_again_and_installations_are_accepted_after() {
    let keys = keys();
    let updates = updates(&keys);
    let mut adapter = pending_adapter();
    adapter.next_boot_is_inactive = true;
    adapter.roll_back_failures = 1;
    for seconds in 0..=300 {
        tick(&updates, &mut adapter, seconds, false);
    }
    assert_eq!(adapter.count("roll_back"), 1);
    assert!(adapter.pending);
    refused(
        &install(&updates, &file(&keys, newer(), &image(3000)), &mut adapter),
        409,
        "The running image is not confirmed yet",
    );
    tick(&updates, &mut adapter, 301, false);
    assert_eq!(adapter.count("roll_back"), 2);
    assert!(!adapter.pending);
    assert!(!adapter.next_boot_is_inactive);
    tick(&updates, &mut adapter, 302, false);
    assert_eq!(adapter.count("roll_back"), 2);
    let (status, body) = install(&updates, &file(&keys, newer(), &image(3000)), &mut adapter);
    assert_eq!(status, 200, "{body}");
}

// ---- The routes behind the gate ----

const UPDATE: &str = "/api/v1/system/update";
const VERSION_PATH: &str = "/api/v1/system/version";

struct Server {
    home: Mutex<Home>,
    random: Mutex<support::Random>,
}

impl Server {
    /// A home with an account, and the access token of a login.
    fn with_token() -> (Server, String) {
        let mut home = Home::new(T0);
        home.load_auth(Some(Account {
            id: Id::parse("11111111-1111-4111-8111-111111111111").unwrap(),
            username: "admin".into(),
            display_name: "Owner".into(),
            language: Language::En,
            created_at: T0,
            password: PasswordHash::new("correct horse battery", [7; 16]),
        }));
        let server = Server {
            home: Mutex::new(home),
            random: Mutex::new(counter()),
        };
        let body = br#"{"username":"admin","password":"correct horse battery"}"#;
        let (status, login) = whole(server.send(
            "POST",
            "/api/v1/auth/login",
            None,
            Some("application/json"),
            body,
        ));
        assert_eq!(status, 200, "{login}");
        let token = login["accessToken"].as_str().unwrap().to_string();
        (server, token)
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Reply {
        self.send_to(method, path, Some(HOST), token, content_type, body)
    }

    fn send_to(
        &self,
        method: &str,
        path: &str,
        host: Option<&str>,
        token: Option<&str>,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Reply {
        let authorization = token.map(|t| format!("Bearer {t}"));
        let mut random = |buffer: &mut [u8]| (self.random.lock().unwrap())(buffer);
        handle_guarded(
            &self.home,
            &Request {
                method,
                path,
                host,
                port: PORT,
                reach: Reach::Loopback,
                content_type,
                authorization: authorization.as_deref(),
                content_length: Some(body.len() as u64),
                body,
            },
            T0,
            &mut random,
        )
    }
}

/// (status, body as JSON) of a whole answer; anything else fails the test.
fn whole(reply: Reply) -> (u16, Value) {
    match reply {
        Reply::Whole(response) => (
            response.status,
            response
                .body
                .map_or(Value::Null, |t| serde_json::from_str(&t).unwrap()),
        ),
        other => panic!("not a whole answer: {other:?}"),
    }
}

#[test]
fn the_update_route_without_a_token_is_401_and_the_version_route_too() {
    let (server, _) = Server::with_token();
    for (method, path) in [("POST", UPDATE), ("GET", VERSION_PATH)] {
        let (status, body) =
            whole(server.send(method, path, None, Some("application/octet-stream"), b""));
        assert_eq!(status, 401, "{method} {path}: {body}");
        assert_eq!(body, json!({"error": "Authentication required"}));
    }
    let (status, _) = whole(server.send(
        "POST",
        UPDATE,
        Some(&"0".repeat(32)),
        Some("application/octet-stream"),
        b"",
    ));
    assert_eq!(status, 401);
}

#[test]
fn the_update_route_refuses_a_foreign_host_before_anything_else() {
    let (server, token) = Server::with_token();
    let (status, body) = whole(server.send_to(
        "POST",
        UPDATE,
        Some("evil.example:8080"),
        Some(&token),
        Some("application/octet-stream"),
        b"",
    ));
    assert_eq!((status, body), (403, json!({"error": "Forbidden host"})));
}

#[test]
fn the_update_route_with_a_json_content_type_is_415() {
    let (server, token) = Server::with_token();
    let (status, body) =
        whole(server.send("POST", UPDATE, Some(&token), Some("application/json"), b"x"));
    assert_eq!(
        (status, body),
        (415, json!({"error": "Unsupported Media Type"}))
    );
    let (status, _) = whole(server.send("POST", UPDATE, Some(&token), None, b"x"));
    assert_eq!(status, 415);
}

#[test]
fn the_update_route_with_a_token_and_octet_stream_is_left_to_the_streaming_caller() {
    let (server, token) = Server::with_token();
    for content_type in ["application/octet-stream", "Application/Octet-Stream; x=1"] {
        let reply = server.send("POST", UPDATE, Some(&token), Some(content_type), b"x");
        assert!(matches!(reply, Reply::Update), "{content_type}: {reply:?}");
    }
    // The route is recognised on its path and method alone, query string aside.
    assert!(solite_core::api::is_update_request("POST", UPDATE));
    assert!(solite_core::api::is_update_request(
        "POST",
        "/api/v1/system/update?x=1"
    ));
    assert!(!solite_core::api::is_update_request("GET", UPDATE));
    assert!(!solite_core::api::is_update_request(
        "POST",
        "/api/v1/system/update/"
    ));
}

#[test]
fn the_version_route_answers_sowels_version_info_subset() {
    let (server, token) = Server::with_token();
    let (status, body) = whole(server.send("GET", VERSION_PATH, Some(&token), None, b""));
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"current": solite_core::VERSION, "latest": null, "updateAvailable": false, "releaseUrl": null})
    );
}

#[test]
fn other_methods_on_the_two_routes_are_405() {
    let (server, token) = Server::with_token();
    for (method, path) in [
        ("PUT", VERSION_PATH),
        ("POST", VERSION_PATH),
        ("GET", UPDATE),
        ("PUT", UPDATE),
        ("DELETE", UPDATE),
    ] {
        let (status, body) =
            whole(server.send(method, path, Some(&token), Some("application/json"), b"{}"));
        assert_eq!(status, 405, "{method} {path}: {body}");
    }
}

#[test]
fn the_plain_handler_cannot_stream_an_update_and_answers_501() {
    let mut home = Home::new(T0);
    let mut random = counter();
    for content_type in ["application/octet-stream", "application/json"] {
        let response = handle(
            &mut home,
            &Request {
                method: "POST",
                path: UPDATE,
                host: Some(HOST),
                port: PORT,
                reach: Reach::Loopback,
                content_type: Some(content_type),
                authorization: None,
                content_length: Some(20_000),
                body: &[0u8; 20_000],
            },
            T0,
            &mut random,
        );
        assert_eq!(response.status, 501, "{content_type}");
        assert_eq!(
            serde_json::from_str::<Value>(&response.body.unwrap()).unwrap(),
            json!({"error": "Updates need the streaming handler"})
        );
    }
}

#[test]
fn another_route_still_refuses_a_40_kb_json_body() {
    let (server, token) = Server::with_token();
    let body = vec![b' '; 40 * 1024];
    let (status, _) = whole(server.send(
        "POST",
        "/api/v1/zones",
        Some(&token),
        Some("application/json"),
        &body,
    ));
    assert_eq!(status, 413);
}

// ---- The restart route (spec 043) ----

const RESTART: &str = "/api/v1/system/restart";

fn restart(server: &Server, token: &str) -> Reply {
    server.send("POST", RESTART, Some(token), None, b"")
}

fn refused_restart(reply: Reply, message: &str) {
    let (status, body) = whole(reply);
    assert_eq!((status, body), (409, json!({ "error": message })));
}

#[test]
fn a_restart_with_a_session_answers_reply_restart() {
    let (server, token) = Server::with_token();
    let reply = restart(&server, &token);
    assert!(matches!(reply, Reply::Restart), "{reply:?}");
}

#[test]
fn a_restart_without_a_session_is_401() {
    let (server, _) = Server::with_token();
    let (status, body) = whole(server.send("POST", RESTART, None, None, b""));
    assert_eq!(
        (status, body),
        (401, json!({"error": "Authentication required"}))
    );
    let (status, _) = whole(server.send("POST", RESTART, Some(&"0".repeat(32)), None, b""));
    assert_eq!(status, 401);
}

#[test]
fn another_method_on_the_restart_route_is_405() {
    let (server, token) = Server::with_token();
    for method in ["GET", "PUT", "DELETE"] {
        let (status, _) = whole(server.send(method, RESTART, Some(&token), None, b""));
        assert_eq!(status, 405, "{method}");
    }
}

#[test]
fn a_restart_during_an_update_being_received_is_409_and_goes_through_once_it_ended() {
    let keys = keys();
    let updates = Arc::new(updates(&keys));
    let (server, token) = Server::with_token();
    server
        .home
        .lock()
        .unwrap()
        .set_update_status(updates.status());
    let bytes = file(&keys, newer(), &image(5000));
    let (waiting, is_waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let first = {
        let updates = Arc::clone(&updates);
        let head = bytes[..HEADER_BYTES + 100].to_vec();
        std::thread::spawn(move || {
            let mut source = Blocking {
                head: Cursor::new(head),
                waiting,
                release: released,
            };
            install_from(&updates, &mut source, &mut FakeAdapter::new())
        })
    };
    is_waiting.recv().unwrap();
    refused_restart(restart(&server, &token), "Update already in progress");
    release.send(()).unwrap();
    first.join().unwrap();
    assert!(matches!(restart(&server, &token), Reply::Restart));
}

#[test]
fn a_restart_during_an_update_check_or_a_pull_is_409_with_its_text() {
    let (server, token) = Server::with_token();
    let check = server
        .home
        .lock()
        .unwrap()
        .release_ticket(true, T0)
        .unwrap();
    refused_restart(restart(&server, &token), "Update check already in progress");
    drop(check);
    let pull = server
        .home
        .lock()
        .unwrap()
        .release_ticket(false, T0)
        .unwrap();
    refused_restart(restart(&server, &token), "Update already in progress");
    drop(pull);
    assert!(matches!(restart(&server, &token), Reply::Restart));
}

#[test]
fn a_restart_during_a_backup_is_409_and_waiting_for_a_restore_is_allowed() {
    let (server, token) = Server::with_token();
    let export = server.home.lock().unwrap().backup_begin(false).unwrap();
    refused_restart(restart(&server, &token), "Backup already in progress");
    drop(export);
    let restore = server.home.lock().unwrap().backup_begin(true).unwrap();
    refused_restart(restart(&server, &token), "Backup already in progress");
    drop(restore);
    let mut restore = server.home.lock().unwrap().backup_begin(true).unwrap();
    restore.set_pending();
    assert!(matches!(restart(&server, &token), Reply::Restart));
}

#[test]
fn a_restart_during_a_probation_is_409_and_goes_through_once_confirmed() {
    let (updates, _) = Updates::from_key_text(None);
    let (server, token) = Server::with_token();
    server
        .home
        .lock()
        .unwrap()
        .set_update_status(updates.status());
    let mut adapter = pending_adapter();
    tick(&updates, &mut adapter, 0, true);
    refused_restart(
        restart(&server, &token),
        "The update is being checked, restart in a minute",
    );
    for seconds in 1..=60 {
        tick(&updates, &mut adapter, seconds, true);
    }
    assert_eq!(adapter.count("confirm"), 1);
    assert!(matches!(restart(&server, &token), Reply::Restart));
}

#[test]
fn a_restart_while_an_update_is_installed_and_waits_for_its_restart_goes_through() {
    let keys = keys();
    let updates = updates(&keys);
    let (server, token) = Server::with_token();
    server
        .home
        .lock()
        .unwrap()
        .set_update_status(updates.status());
    let mut adapter = FakeAdapter::new();
    let (status, body) = install(&updates, &file(&keys, newer(), &image(3000)), &mut adapter);
    assert_eq!(status, 200, "{body}");
    // The new image is a probation of the next boot, not of this one.
    assert!(matches!(restart(&server, &token), Reply::Restart));
}

#[test]
fn the_plain_handler_does_not_restart() {
    let (server, token) = Server::with_token();
    let authorization = format!("Bearer {token}");
    let mut home = server.home.lock().unwrap();
    let response = handle(
        &mut home,
        &Request {
            method: "POST",
            path: RESTART,
            host: Some(HOST),
            port: PORT,
            reach: Reach::Loopback,
            content_type: None,
            authorization: Some(&authorization),
            content_length: Some(0),
            body: b"",
        },
        T0,
        &mut counter(),
    );
    assert_eq!(response.status, 501);
}
