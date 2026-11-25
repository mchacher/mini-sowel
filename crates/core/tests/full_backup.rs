//! Spec 035: the full backup, export and restore, through the in-memory files of
//! `support/backup_fake.rs`. The archive of R3 is also built here, byte by byte, by an encoder
//! written from the spec and not from the code, so that a refusal can be provoked at each check.
//! The key derivation runs at `MIN_ITERATIONS` here (the header says so), not at the box's 100,000.

mod support;

use std::io::{Cursor, Read};
use std::rc::Rc;

use chacha20::ChaCha20;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use solite_core::api::Response;
use solite_core::auth::pbkdf2_sha256;
use solite_core::backup::{
    BACKUP_ITERATIONS, MAX_ARCHIVE_BYTES, MAX_BACKUP_ITERATIONS, MAX_EXPORT_MS, MAX_FILE_BYTES,
    Phase, apply_pending,
};
use solite_core::engine::{load_home, load_zones};
use solite_core::home::Home;
use solite_core::radio::backup::text as zigbee_text;
use solite_core::radio::{Network, NetworkKeys};
use support::backup_fake::{Clock, MARK, MARKER, MemFiles};

const PASSWORD: &str = "correct horse battery";
const ITER: u32 = 1000;
const NETWORK_KEY: [u8; 16] = [0xA1; 16];
const WIFI_PASSWORD: &str = "hunter2-wifi-pass";

fn random() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        for byte in buffer.iter_mut() {
            n = n.wrapping_add(37);
            *byte = n;
        }
        Ok(())
    }
}

fn zigbee(pan: u16, ext: [u8; 8], key: [u8; 16]) -> String {
    zigbee_text(
        &Network {
            pan_id: pan,
            extended_pan_id: ext,
            channel: 20,
            formed: true,
            from_backup: false,
            keys: None,
        },
        &NetworkKeys {
            network_key: key,
            trust_center_key: [0xB2; 16],
        },
    )
}

/// The seven files of a box.
fn full_box() -> MemFiles {
    let files = MemFiles::new();
    // More than one chunk of 4 KB.
    let devices = format!("[{}]", "{\"id\":\"d\"},".repeat(900) + "{\"id\":\"last\"}");
    files.put("devices.json", devices.as_bytes());
    files.put("zones.json", br#"[{"id":"z"}]"#);
    files.put("equipments.json", br#"[{"id":"e"}]"#);
    files.put("recipes.json", br#"[{"id":"r"}]"#);
    files.put("settings.json", br#"{"home.name":"Annex"}"#);
    files.put(
        "zigbee.json",
        zigbee(0x1A2B, [1, 2, 3, 4, 5, 6, 7, 8], NETWORK_KEY).as_bytes(),
    );
    files.put(
        "wifi.json",
        format!("{{\"ssid\":\"Home\",\"password\":\"{WIFI_PASSWORD}\"}}").as_bytes(),
    );
    files
}

/// Another box, with other content in every file.
fn other_box() -> MemFiles {
    let files = MemFiles::with(&[
        ("devices.json", "[]"),
        ("zones.json", "[{\"id\":\"old\"}]"),
        ("equipments.json", "[]"),
        ("recipes.json", "[]"),
        ("settings.json", "{}"),
        ("auth.json", "{\"version\":1}"),
        (
            "wifi.json",
            "{\"ssid\":\"Other\",\"password\":\"keep-me-please\"}",
        ),
    ]);
    files.put(
        "zigbee.json",
        zigbee(0x7777, [9, 9, 9, 9, 9, 9, 9, 9], [0xCC; 16]).as_bytes(),
    );
    files
}

fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn export_with(
    files: &MemFiles,
    header: Option<&str>,
    iterations: u32,
) -> Result<Vec<u8>, Response> {
    let home = Home::new(0);
    let ticket = home.backup_begin(false).unwrap();
    let mut reader = ticket.export_with_iterations(
        header,
        Box::new(files.clone()),
        Box::new(Clock::new()),
        &mut random(),
        iterations,
    )?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(home.backup_state().phase(), Phase::Idle, "the job ended");
    Ok(bytes)
}

fn export(files: &MemFiles) -> Vec<u8> {
    export_with(files, Some(&percent(PASSWORD)), ITER).unwrap()
}

fn answer(response: Response) -> (u16, Value) {
    (
        response.status,
        serde_json::from_str(response.body.as_deref().unwrap_or("null")).unwrap(),
    )
}

fn restore_with(
    files: &MemFiles,
    archive: &[u8],
    header: Option<&str>,
    wifi: bool,
) -> (u16, Value) {
    let home = Home::new(0);
    let ticket = home.backup_begin(true).unwrap();
    let response = ticket.restore(
        wifi,
        Some(archive.len() as u64),
        header,
        &mut Cursor::new(archive),
        &mut files.clone(),
        &mut Clock::new(),
    );
    answer(response)
}

fn restore(files: &MemFiles, archive: &[u8], wifi: bool) -> (u16, Value) {
    restore_with(files, archive, Some(&percent(PASSWORD)), wifi)
}

fn apply(files: &MemFiles) {
    apply_pending(&mut files.clone()).unwrap();
}

fn error_of(result: &(u16, Value)) -> String {
    result.1["error"].as_str().unwrap_or("").to_string()
}

fn refused(files: &MemFiles, archive: &[u8]) -> (u16, String) {
    let before = files.snapshot();
    let result = restore(files, archive, true);
    assert_eq!(files.snapshot(), before, "a refused restore changed a file");
    (result.0, error_of(&result))
}

// ---- An encoder written from the spec ----

struct Entry {
    name: &'static str,
    flags: u8,
    data: Vec<u8>,
    chunk: usize,
}

fn entry(name: &'static str, flags: u8, data: &[u8]) -> Entry {
    Entry {
        name,
        flags,
        data: data.to_vec(),
        chunk: 4096,
    }
}

fn hmac_of(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

/// A valid archive of `entries`, as R3 describes it.
fn build(password: &str, iterations: u32, entries: &[Entry]) -> Vec<u8> {
    let salt = [1u8; 16];
    let nonce = [2u8; 8];
    let master = pbkdf2_sha256(password.as_bytes(), &salt, iterations);
    let encryption = hmac_of(&master, &[b"solite backup encryption"]);
    let authentication = hmac_of(&master, &[b"solite backup authentication"]);
    let mut out = b"SOLITEBK".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&iterations.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&hmac_of(&authentication, &[b"solite backup key check"]));
    let mut manifest_files = Vec::new();
    for (index, e) in entries.iter().enumerate() {
        out.push(1);
        out.push(e.name.len() as u8);
        out.extend_from_slice(e.name.as_bytes());
        out.push(e.flags);
        let mut stored = e.data.clone();
        if e.flags & 1 == 1 {
            let mut full = [0u8; 12];
            full[..8].copy_from_slice(&nonce);
            full[8..].copy_from_slice(&(index as u32).to_be_bytes());
            ChaCha20::new(&encryption.into(), &full.into()).apply_keystream(&mut stored);
        }
        for chunk in stored.chunks(e.chunk) {
            out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            out.extend_from_slice(chunk);
        }
        out.extend_from_slice(&0u16.to_le_bytes());
        manifest_files.push(json!({
            "name": e.name,
            "size": stored.len(),
            "sha256": Sha256::digest(&stored).iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "encrypted": e.flags & 1 == 1,
        }));
    }
    let manifest = json!({"version": 1, "solite": "test", "files": manifest_files}).to_string();
    out.push(0);
    out.extend_from_slice(&(manifest.len() as u16).to_le_bytes());
    out.extend_from_slice(manifest.as_bytes());
    let mac = hmac_of(&authentication, &[&out]);
    out.extend_from_slice(&mac);
    out
}

// ---- Round trips ----

#[test]
fn a_full_box_exported_and_restored_with_wifi_gives_byte_identical_files() {
    let source = full_box();
    let archive = export(&source);
    assert_eq!(&archive[..8], b"SOLITEBK");
    let target = other_box();
    let (status, body) = restore(&target, &archive, true);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["wifi"], true);
    assert_eq!(body["files"], 7);
    assert_eq!(
        body["message"],
        "Backup restored: the box restarts, log in again after it"
    );
    // Nothing is replaced before the next start.
    assert_eq!(target.text("zones.json").unwrap(), "[{\"id\":\"old\"}]");
    apply(&target);
    for name in [
        "devices.json",
        "zones.json",
        "equipments.json",
        "recipes.json",
        "settings.json",
        "zigbee.json",
        "wifi.json",
    ] {
        assert_eq!(target.get(name), source.get(name), "{name}");
    }
    // The account is the box's own.
    assert_eq!(target.text("auth.json").unwrap(), "{\"version\":1}");
    assert!(target.nothing_staged());
}

#[test]
fn a_restore_without_wifi_keeps_the_wifi_of_the_box() {
    let source = full_box();
    let archive = export(&source);
    let target = other_box();
    let (status, body) = restore(&target, &archive, false);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["wifi"], false);
    apply(&target);
    assert!(target.text("wifi.json").unwrap().contains("keep-me-please"));
    assert_eq!(target.get("devices.json"), source.get("devices.json"));
    assert_eq!(target.get("zigbee.json"), source.get("zigbee.json"));
    assert!(!target.log().iter().any(|l| l == "stage:wifi.json"));
}

#[test]
fn wifi_asked_on_a_box_without_wifi_is_ignored() {
    let archive = export(&full_box());
    let mut target = other_box();
    target.wifi = false;
    target.restarts = false;
    let (status, body) = restore(&target, &archive, true);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["wifi"], false);
    assert_eq!(
        body["message"],
        "Backup restored: restart the host to load it"
    );
    apply(&target);
    assert!(target.text("wifi.json").unwrap().contains("keep-me-please"));
}

#[test]
fn an_accented_and_emoji_password_round_trips() {
    let password = "pässwörd-é-🚀-ok";
    let source = full_box();
    let archive = export_with(&source, Some(&percent(password)), ITER).unwrap();
    let target = other_box();
    let (status, body) = restore_with(&target, &archive, Some(&percent(password)), true);
    assert_eq!(status, 200, "{body}");
    apply(&target);
    assert_eq!(target.get("devices.json"), source.get("devices.json"));
    // A composed and a combining accent are two passwords.
    let (status, _) = restore_with(
        &target,
        &archive,
        Some(&percent("pa\u{308}sswörd-é-🚀-ok")),
        true,
    );
    assert_eq!(status, 400);
}

#[test]
fn the_same_archive_restored_twice_gives_the_same_files() {
    let source = full_box();
    let archive = export(&source);
    let target = other_box();
    for _ in 0..2 {
        let (status, body) = restore(&target, &archive, true);
        assert_eq!(status, 200, "{body}");
        apply(&target);
    }
    for name in ["devices.json", "zones.json", "zigbee.json", "wifi.json"] {
        assert_eq!(target.get(name), source.get(name), "{name}");
    }
    // The second restore found the same network: nothing was set aside by it.
    let asides: Vec<String> = target
        .names()
        .into_iter()
        .filter(|n| n.starts_with("zigbee-"))
        .collect();
    assert_eq!(asides, vec!["zigbee-7777.json".to_string()]);
}

#[test]
fn an_empty_home_gives_a_manifest_only_archive_that_removes_the_home_files() {
    let source = MemFiles::new();
    let archive = export(&source);
    // The header, the end byte, the manifest's length and text, the MAC.
    assert_eq!(archive[70], 0);
    let target = other_box();
    let (status, body) = restore(&target, &archive, true);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["files"], 0, "the removals are not counted");
    apply(&target);
    for name in [
        "devices.json",
        "zones.json",
        "equipments.json",
        "recipes.json",
        "settings.json",
    ] {
        assert!(!target.has(name), "{name} removed");
    }
    // The network and the Wi-Fi stay.
    assert!(target.text("zigbee.json").unwrap().contains("0x7777"));
    assert!(target.text("wifi.json").unwrap().contains("keep-me-please"));
    assert!(target.has("auth.json"));
}

#[test]
fn the_secrets_are_encrypted_in_the_archive_and_the_rest_is_plain() {
    let source = full_box();
    let archive = export(&source);
    let contains = |needle: &[u8]| archive.windows(needle.len()).any(|w| w == needle);
    let key_text: String = NETWORK_KEY.iter().map(|b| format!("{b:02X}")).collect();
    assert!(
        !contains(key_text.as_bytes()),
        "the network key is in clear"
    );
    assert!(
        !contains(WIFI_PASSWORD.as_bytes()),
        "the Wi-Fi password is in clear"
    );
    assert!(!contains(b"networkKey"), "the zigbee file is in clear");
    // The search works: a plain file is found.
    assert!(contains(b"{\"id\":\"last\"}"));
}

#[test]
fn a_live_file_missing_with_its_new_file_present_is_exported_from_the_new_file() {
    let source = full_box();
    let zones = source.get("zones.json").unwrap();
    source.put("zones.json.new", b"[{\"id\":\"from-new\"}]");
    // The fake reads `.new` only when the live file is missing, as the adapters do.
    source.fs.borrow_mut().files.remove("zones.json");
    let archive = export(&source);
    let target = MemFiles::new();
    assert_eq!(restore(&target, &archive, true).0, 200);
    apply(&target);
    assert_eq!(
        target.text("zones.json").unwrap(),
        "[{\"id\":\"from-new\"}]"
    );
    assert_ne!(target.get("zones.json").unwrap(), zones);
}

// ---- The password ----

#[test]
fn a_wrong_password_is_refused_before_anything_is_staged() {
    let archive = export(&full_box());
    let target = other_box();
    let before = target.snapshot();
    let (status, body) = restore_with(
        &target,
        &archive,
        Some(&percent("wrong horse battery")),
        true,
    );
    assert_eq!(status, 400);
    assert_eq!(body["error"], "Wrong backup password");
    assert_eq!(target.snapshot(), before);
    assert!(
        target.log().is_empty(),
        "nothing staged or discarded: {:?}",
        target.log()
    );
}

#[test]
fn a_missing_or_invalid_password_header_is_400_backup_password_required_both_ways() {
    let source = full_box();
    let archive = export(&source);
    for header in [
        None,
        Some(""),
        Some("%zz"),
        Some("%F"),
        Some("%FF"),
        Some("ab%"),
    ] {
        let export = export_with(&source, header, ITER).unwrap_err();
        assert_eq!(
            (export.status, export.body.as_deref()),
            (400, Some(r#"{"error":"Backup password required"}"#)),
            "export {header:?}"
        );
        let target = other_box();
        let before = target.snapshot();
        let (status, body) = restore_with(&target, &archive, header, true);
        assert_eq!(
            (status, body["error"].as_str()),
            (400, Some("Backup password required")),
            "restore {header:?}"
        );
        assert_eq!(target.snapshot(), before);
    }
}

#[test]
fn an_export_password_under_8_or_over_128_characters_is_refused() {
    let source = full_box();
    let long = "x".repeat(129);
    let longest = "x".repeat(128);
    for (password, ok) in [
        ("short", false),
        ("1234567", false),
        ("12345678", true),
        (long.as_str(), false),
        (longest.as_str(), true),
    ] {
        let result = export_with(&source, Some(&percent(password)), ITER);
        match (result, ok) {
            (Ok(_), true) => {}
            (Err(e), false) => assert_eq!(
                (e.status, e.body.as_deref()),
                (
                    400,
                    Some(r#"{"error":"Backup password must be 8 to 128 characters"}"#)
                ),
                "{} characters",
                password.chars().count()
            ),
            (other, _) => panic!(
                "{} characters: {:?}",
                password.chars().count(),
                other.is_ok()
            ),
        }
    }
    // Characters, not bytes: 8 emoji are 32 bytes.
    assert!(export_with(&source, Some(&percent("🚀🚀🚀🚀🚀🚀🚀🚀")), ITER).is_ok());
}

#[test]
fn the_export_takes_the_boxs_iteration_count_by_default() {
    let home = Home::new(0);
    let ticket = home.backup_begin(false).unwrap();
    let mut reader = ticket
        .export(
            Some(&percent(PASSWORD)),
            Box::new(MemFiles::new()),
            Box::new(Clock::new()),
            &mut random(),
        )
        .unwrap();
    let mut header = [0u8; 70];
    reader.read_exact(&mut header).unwrap();
    assert_eq!(
        u32::from_le_bytes(header[10..14].try_into().unwrap()),
        BACKUP_ITERATIONS
    );
    assert_eq!(BACKUP_ITERATIONS, 100_000);
}

// ---- Refusals: the archive checked whole, no live file changed ----

#[test]
fn a_truncated_archive_is_refused_at_every_cut_and_nothing_stays_staged() {
    let archive = export(&full_box());
    let len = archive.len();
    let cuts = [
        5,
        8,
        9,
        10,
        40,
        69,
        70,
        71,
        73,
        90,
        4200,
        len / 2,
        len - 120,
        len - 40,
        len - 33,
        len - 32,
        len - 1,
    ];
    for cut in cuts {
        let target = other_box();
        let (status, message) = refused(&target, &archive[..cut]);
        assert_eq!(
            (status, message.as_str()),
            (400, "Invalid backup: truncated"),
            "cut at {cut} of {len}"
        );
        assert!(target.nothing_staged(), "cut at {cut}");
    }
}

#[test]
fn every_byte_changed_anywhere_in_the_archive_is_refused() {
    let source = MemFiles::with(&[("zones.json", "[{\"id\":\"z\"}]"), ("settings.json", "{}")]);
    source.put(
        "zigbee.json",
        zigbee(0x1A2B, [1, 2, 3, 4, 5, 6, 7, 8], NETWORK_KEY).as_bytes(),
    );
    let archive = export(&source);
    for at in 0..archive.len() {
        let mut changed = archive.clone();
        changed[at] ^= 0x01;
        let target = other_box();
        let (status, message) = refused(&target, &changed);
        assert!(
            status == 400 && message.starts_with("Invalid backup")
                || message == "Wrong backup password"
                || message == "Backup made by a newer version of Solite",
            "byte {at}: {status} {message}"
        );
        assert!(target.nothing_staged(), "byte {at}");
    }
}

#[test]
fn a_byte_changed_in_a_plain_chunk_an_encrypted_chunk_the_manifest_and_the_mac_says_why() {
    let archive = export(&full_box());
    let find = |needle: &[u8]| {
        archive
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap()
    };
    let in_plain = find(b"\"id\":\"last\"");
    let in_manifest = find(b"\"version\":1");
    // The encrypted chunk of zigbee.json: after its entry header.
    let in_secret = find(b"zigbee.json") + "zigbee.json".len() + 1 + 2 + 5;
    let in_mac = archive.len() - 5;
    for (at, what) in [
        (in_plain, "plain"),
        (in_secret, "secret"),
        (in_manifest, "manifest"),
        (in_mac, "mac"),
    ] {
        let mut changed = archive.clone();
        changed[at] ^= 0x40;
        let target = other_box();
        let (status, message) = refused(&target, &changed);
        assert_eq!(status, 400, "{what}");
        assert_eq!(
            message, "Invalid backup: the file is damaged or was changed",
            "{what}"
        );
    }
}

#[test]
fn bytes_after_the_mac_are_refused() {
    let mut archive = export(&full_box());
    archive.push(0);
    let target = other_box();
    let (status, message) = refused(&target, &archive);
    assert_eq!(
        (status, message.as_str()),
        (400, "Invalid backup: data after the end")
    );
}

#[test]
fn a_newer_format_is_refused_with_nothing_read_after_it_and_nothing_staged() {
    let mut archive = export(&full_box());
    archive[8..10].copy_from_slice(&2u16.to_le_bytes());
    let target = other_box();
    let (status, message) = refused(&target, &archive);
    assert_eq!(
        (status, message.as_str()),
        (400, "Backup made by a newer version of Solite")
    );
    assert!(target.log().is_empty());
    // Not even the password is needed to say so.
    let (status, body) = restore_with(&target, &archive[..10], Some("a%20password"), true);
    assert_eq!(status, 400);
    assert_eq!(body["error"], "Backup made by a newer version of Solite");
}

#[test]
fn format_zero_is_refused() {
    let mut archive = export(&full_box());
    archive[8..10].copy_from_slice(&0u16.to_le_bytes());
    let (status, message) = refused(&other_box(), &archive);
    assert_eq!(status, 400);
    assert!(message.starts_with("Invalid backup: "), "{message}");
}

#[test]
fn a_file_that_is_not_a_solite_backup_is_refused() {
    for bytes in [
        &b"NOTSOLITE and more bytes after that"[..],
        &b"abc"[..],
        &b""[..],
        &b"{\"version\":1,\"settings\":{},\"zones\":[]}"[..],
    ] {
        let (status, message) = refused(&other_box(), bytes);
        assert_eq!(
            (status, message.as_str()),
            (400, "Invalid backup: not a Solite backup"),
            "{bytes:?}"
        );
    }
}

#[test]
fn iterations_out_of_range_are_refused_before_the_key_is_derived() {
    let archive = export(&full_box());
    for iterations in [0u32, 999, MAX_BACKUP_ITERATIONS + 1, u32::MAX] {
        let mut changed = archive.clone();
        changed[10..14].copy_from_slice(&iterations.to_le_bytes());
        let (status, message) = refused(&other_box(), &changed);
        assert_eq!(
            (status, message.as_str()),
            (400, "Invalid backup: iterations out of range"),
            "{iterations}"
        );
    }
}

#[test]
fn an_archive_over_the_bound_streamed_is_413_and_a_declared_length_over_it_is_413_unread() {
    // Three plain files of 900 KB each: past 2 MiB before any is complete.
    let big = vec![b'a'; 900 * 1024];
    let archive = build(
        PASSWORD,
        ITER,
        &[
            entry("devices.json", 0, &big),
            entry("zones.json", 0, &big),
            entry("equipments.json", 0, &big),
        ],
    );
    assert!(archive.len() > MAX_ARCHIVE_BYTES);
    let target = other_box();
    let (status, message) = refused(&target, &archive);
    assert_eq!((status, message.as_str()), (413, "Backup too large"));
    assert!(target.nothing_staged());

    struct Unread;
    impl Read for Unread {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("the body was read");
        }
    }
    let home = Home::new(0);
    let ticket = home.backup_begin(true).unwrap();
    let response = ticket.restore(
        false,
        Some(MAX_ARCHIVE_BYTES as u64 + 1),
        Some(&percent(PASSWORD)),
        &mut Unread,
        &mut target.clone(),
        &mut Clock::new(),
    );
    assert_eq!(
        answer(response),
        (413, json!({"error": "Backup too large"}))
    );
    assert_eq!(home.backup_state().phase(), Phase::Idle);
}

#[test]
fn an_archive_of_exactly_the_files_at_their_bound_is_accepted() {
    // One file at `MAX_FILE_BYTES` exactly.
    let file = vec![b'x'; MAX_FILE_BYTES];
    let archive = build(PASSWORD, ITER, &[entry("devices.json", 0, &file)]);
    let target = MemFiles::new();
    let (status, body) = restore(&target, &archive, false);
    assert_eq!(status, 200, "{body}");
    apply(&target);
    assert_eq!(target.get("devices.json").unwrap().len(), MAX_FILE_BYTES);
}

/// An archive of exactly `total` bytes: two files, the second's size tuned.
fn archive_of_exactly(total: usize) -> Vec<u8> {
    let first = vec![b'a'; MAX_FILE_BYTES];
    let mut second = MAX_FILE_BYTES - 1000;
    for _ in 0..10 {
        let archive = build(
            PASSWORD,
            ITER,
            &[
                entry("devices.json", 0, &first),
                entry("zones.json", 0, &vec![b'b'; second]),
            ],
        );
        if archive.len() == total {
            return archive;
        }
        second = (second as isize + total as isize - archive.len() as isize) as usize;
    }
    panic!("no archive of {total} bytes");
}

#[test]
fn an_archive_of_exactly_2_mib_is_accepted_and_one_byte_more_is_413() {
    let exact = archive_of_exactly(MAX_ARCHIVE_BYTES);
    assert_eq!(exact.len(), MAX_ARCHIVE_BYTES);
    let target = MemFiles::new();
    let (status, body) = restore(&target, &exact, false);
    assert_eq!(status, 200, "{body}");

    // One byte past the bound: refused as too large (the MAC would not hold either).
    let mut over = exact.clone();
    over.push(0);
    let target = MemFiles::new();
    let (status, message) = refused(&target, &over);
    assert_eq!((status, message.as_str()), (413, "Backup too large"));
    assert!(target.nothing_staged());
}

#[test]
fn a_file_over_the_bound_an_unknown_name_a_name_twice_a_secret_in_clear_and_a_long_chunk_are_refused()
 {
    let small = b"[]".as_slice();
    let too_big = vec![b'x'; MAX_FILE_BYTES + 1];
    let long_chunk = Entry {
        chunk: 4097,
        ..entry("devices.json", 0, &vec![b'y'; 8000])
    };
    let cases: Vec<(&str, Vec<Entry>, &str)> = vec![
        (
            "a file over MAX_FILE_BYTES",
            vec![entry("devices.json", 0, &too_big)],
            "Invalid backup: devices.json too large",
        ),
        (
            "an unknown name",
            vec![entry("auth.json", 0, small)],
            "Invalid backup: unknown file auth.json",
        ),
        (
            "a name twice",
            vec![entry("zones.json", 0, small), entry("zones.json", 0, small)],
            "Invalid backup: zones.json twice",
        ),
        (
            "a secret stored in clear",
            vec![entry("wifi.json", 0, small)],
            "Invalid backup: wifi.json stored wrongly",
        ),
        (
            "a home file stored encrypted",
            vec![entry("zones.json", 1, small)],
            "Invalid backup: zones.json stored wrongly",
        ),
        (
            "a chunk over 4096",
            vec![long_chunk],
            "Invalid backup: chunk too long",
        ),
    ];
    for (what, entries, expected) in cases {
        let archive = build(PASSWORD, ITER, &entries);
        let target = other_box();
        let (status, message) = refused(&target, &archive);
        assert_eq!((status, message.as_str()), (400, expected), "{what}");
        assert!(target.nothing_staged(), "{what}");
    }
}

#[test]
fn the_encoder_of_this_file_builds_what_the_restore_accepts() {
    // The control of the refusals above: a valid archive from the same encoder passes.
    let archive = build(
        PASSWORD,
        ITER,
        &[
            entry("zones.json", 0, b"[{\"id\":\"built\"}]"),
            entry("wifi.json", 1, b"{\"ssid\":\"x\"}"),
        ],
    );
    let target = other_box();
    let (status, body) = restore(&target, &archive, true);
    assert_eq!(status, 200, "{body}");
    apply(&target);
    assert_eq!(target.text("zones.json").unwrap(), "[{\"id\":\"built\"}]");
    assert_eq!(target.text("wifi.json").unwrap(), "{\"ssid\":\"x\"}");
}

// ---- An export that fails ----

#[test]
fn an_export_that_fails_mid_stream_gives_a_file_a_restore_refuses() {
    let source = full_box();
    *source.fail_read.borrow_mut() = Some(("devices.json".to_string(), 5000));
    let home = Home::new(0);
    let ticket = home.backup_begin(false).unwrap();
    let mut reader = ticket
        .export_with_iterations(
            Some(&percent(PASSWORD)),
            Box::new(source.clone()),
            Box::new(Clock::new()),
            &mut random(),
            ITER,
        )
        .unwrap();
    let mut partial = Vec::new();
    let error = reader.read_to_end(&mut partial).unwrap_err();
    assert!(error.to_string().contains("devices.json"), "{error}");
    assert!(partial.len() > 4096, "the first chunk was sent");
    assert_eq!(home.backup_state().phase(), Phase::Idle, "the job ended");
    let target = other_box();
    let (status, message) = refused(&target, &partial);
    assert_eq!(
        (status, message.as_str()),
        (400, "Invalid backup: truncated")
    );
}

#[test]
fn an_export_past_its_time_limit_ends_without_its_mac_and_frees_the_saver() {
    let source = full_box();
    let home = Home::new(0);
    let state = home.backup_state();
    let ticket = home.backup_begin(false).unwrap();
    let clock = Clock::new();
    let now = Rc::clone(&clock.now);
    let mut reader = ticket
        .export_with_iterations(
            Some(&percent(PASSWORD)),
            Box::new(source),
            Box::new(clock),
            &mut random(),
            ITER,
        )
        .unwrap();
    assert_eq!(state.phase(), Phase::Exporting);
    assert!(state.try_write().is_none(), "the Saver waits");
    let mut first = [0u8; 100];
    reader.read_exact(&mut first).unwrap();
    // The client stops reading for two minutes.
    now.set(now.get() + MAX_EXPORT_MS);
    let error = reader.read(&mut first).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(state.phase(), Phase::Idle);
    assert!(state.try_write().is_some(), "the Saver writes again");
    // And it stays ended.
    assert!(reader.read(&mut first).is_err());
}

#[test]
fn an_export_waits_for_a_write_in_progress_then_goes_on_or_gives_503_after_5_seconds() {
    let source = full_box();
    // The mark clears after 3 pauses: the export waits, then goes on.
    let home = Home::new(0);
    let state = home.backup_state();
    let mark = Rc::new(std::cell::RefCell::new(state.try_write()));
    assert!(mark.borrow().is_some());
    let ticket = home.backup_begin(false).unwrap();
    let mut clock = Clock::new();
    let release = Rc::clone(&mark);
    clock.hook = Some(Box::new(move |pauses| {
        if pauses == 3 {
            release.borrow_mut().take();
        }
    }));
    let pauses = Rc::clone(&clock.pauses);
    let reader = ticket.export_with_iterations(
        Some(&percent(PASSWORD)),
        Box::new(source.clone()),
        Box::new(clock),
        &mut random(),
        ITER,
    );
    assert!(reader.is_ok());
    assert_eq!(
        pauses.get(),
        3,
        "no pause for the derivation of 1000 iterations"
    );
    assert_eq!(state.phase(), Phase::Exporting);
    drop(reader);

    // The mark never clears: 503 after 5 s (10 ms per pause), the state back to idle.
    let _writing = state.try_write().unwrap();
    let ticket = home.backup_begin(false).unwrap();
    let error = ticket
        .export_with_iterations(
            Some(&percent(PASSWORD)),
            Box::new(source),
            Box::new(Clock::new()),
            &mut random(),
            ITER,
        )
        .err()
        .unwrap();
    assert_eq!(
        (error.status, error.body.as_deref()),
        (503, Some(r#"{"error":"Busy saving, try again"}"#))
    );
    assert_eq!(state.phase(), Phase::Idle);
}

// ---- The commit and the next start ----

#[test]
fn the_commit_syncs_every_staged_copy_then_writes_the_marker_then_sets_pending() {
    let archive = export(&full_box());
    let target = other_box();
    let home = Home::new(0);
    let mut files = target.clone();
    files.probe = Some(home.backup_state());
    let ticket = home.backup_begin(true).unwrap();
    let response = ticket.restore(
        true,
        None,
        Some(&percent(PASSWORD)),
        &mut Cursor::new(&archive),
        &mut files,
        &mut Clock::new(),
    );
    assert_eq!(answer(response).0, 200);
    let log = target.log();
    let commit = log.iter().position(|l| l == "commit").unwrap();
    let finishes: Vec<usize> = log
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with("finish:"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(finishes.len(), 7);
    assert!(finishes.iter().all(|i| *i < commit), "{log:?}");
    assert_eq!(
        target.fs.borrow().phase_at_commit,
        Some(Phase::Restoring),
        "the state is set to pending after the marker"
    );
    assert_eq!(home.backup_state().phase(), Phase::Pending);
    assert!(target.has(MARKER));
}

#[test]
fn an_interrupted_application_is_finished_at_the_next_start() {
    let source = full_box();
    let archive = export(&source);
    let target = other_box();
    assert_eq!(restore(&target, &archive, true).0, 200);
    // A power cut after two files.
    target.promotes_left.set(Some(2));
    assert!(apply_pending(&mut target.clone()).is_err());
    assert!(target.has(MARKER), "the marker still names what is left");
    assert_eq!(target.get("devices.json"), source.get("devices.json"));
    assert_ne!(target.get("equipments.json"), source.get("equipments.json"));
    target.promotes_left.set(None);
    assert_eq!(apply_pending(&mut target.clone()).unwrap(), 7);
    assert!(!target.has(MARKER));
    for name in [
        "devices.json",
        "zones.json",
        "equipments.json",
        "recipes.json",
        "settings.json",
        "zigbee.json",
        "wifi.json",
    ] {
        assert_eq!(target.get(name), source.get(name), "{name}");
    }
    assert!(target.nothing_staged());
}

#[test]
fn a_staged_copy_without_a_marker_is_removed_at_the_start() {
    let target = other_box();
    target.put("devices.json.restore", b"half a file");
    let before: Vec<_> = target
        .snapshot()
        .into_iter()
        .filter(|(n, _)| !n.ends_with(".json.restore"))
        .collect();
    assert_eq!(apply_pending(&mut target.clone()).unwrap(), 0);
    assert!(target.nothing_staged());
    assert_eq!(target.snapshot().into_iter().collect::<Vec<_>>(), before);
}

#[test]
fn a_cut_or_unparsable_marker_or_one_naming_a_file_outside_the_list_is_discarded_whole() {
    for marker in [
        "promote devices.json\n",
        "promote devices.json\npromote zones.json",
        "promote ../etc/passwd\nend\n",
        "promote auth.json\nend\n",
        "remove auth.json\nend\n",
        "garbage\nend\n",
        "promote devices.json extra\nend\n",
        "rename devices.json\nend\n",
        "promote devices.json\nend\nmore\n",
        "",
    ] {
        let target = other_box();
        target.put("devices.json.restore", b"new devices");
        target.put(MARKER, marker.as_bytes());
        let live = target.get("devices.json");
        assert_eq!(apply_pending(&mut target.clone()).unwrap(), 0, "{marker:?}");
        assert!(target.nothing_staged(), "{marker:?}");
        assert_eq!(target.get("devices.json"), live, "{marker:?}");
        assert!(target.has("auth.json"), "{marker:?}");
    }
}

#[test]
fn a_zigbee_file_of_another_network_is_set_aside_by_its_pan_and_the_restart_mark_is_written() {
    let archive = export(&full_box());
    let target = other_box();
    assert_eq!(restore(&target, &archive, false).0, 200);
    let old = target.get("zigbee.json").unwrap();
    assert!(!target.has(MARK));
    apply(&target);
    assert_eq!(
        target.get("zigbee-7777.json").unwrap(),
        old,
        "kept under its PAN id"
    );
    assert!(target.text("zigbee.json").unwrap().contains("0x1A2B"));
    assert!(
        target.has(MARK),
        "the network start leaves the other network"
    );
    let log = target.log();
    let aside = log.iter().position(|l| l == "aside:zigbee.json").unwrap();
    let mark = log.iter().position(|l| l == "mark").unwrap();
    let promote = log.iter().position(|l| l == "promote:zigbee.json").unwrap();
    assert!(aside < mark && mark < promote, "{log:?}");
}

#[test]
fn a_zigbee_file_of_the_same_network_is_replaced_in_place_without_an_aside() {
    let archive = export(&full_box());
    let target = other_box();
    // The same PAN and extended PAN, other keys.
    target.put(
        "zigbee.json",
        zigbee(0x1A2B, [1, 2, 3, 4, 5, 6, 7, 8], [0xDD; 16]).as_bytes(),
    );
    assert_eq!(restore(&target, &archive, false).0, 200);
    apply(&target);
    assert!(target.text("zigbee.json").unwrap().contains("A1A1A1"));
    assert!(target.names().iter().all(|n| !n.starts_with("zigbee-")));
    assert!(target.has(MARK));
}

#[test]
fn an_archive_without_zigbee_leaves_the_current_network_and_writes_no_mark() {
    let source = MemFiles::with(&[("zones.json", "[]")]);
    let archive = export(&source);
    let target = other_box();
    assert_eq!(restore(&target, &archive, false).0, 200);
    apply(&target);
    assert!(target.text("zigbee.json").unwrap().contains("0x7777"));
    assert!(!target.has(MARK));
}

#[test]
fn a_file_the_marker_removes_is_gone_with_its_new_file_when_the_home_loads() {
    let archive = export(&MemFiles::with(&[("settings.json", "{}")]));
    let target = other_box();
    target.put("zones.json.new", b"[{\"id\":\"left-by-a-power-cut\"}]");
    target.fs.borrow_mut().files.remove("zones.json");
    assert_eq!(restore(&target, &archive, false).0, 200);
    apply(&target);
    // The box's storage would read `zones.json.new` for a missing `zones.json`.
    assert!(load_zones(&&target).unwrap().is_empty());
    assert!(!target.has("zones.json.new"));
    assert!(load_home(&&target, 0).is_ok());
}

#[test]
fn a_restore_of_a_refused_archive_leaves_the_state_idle_and_a_good_one_leaves_it_pending() {
    let archive = export(&full_box());
    let home = Home::new(0);
    let ticket = home.backup_begin(true).unwrap();
    assert_eq!(home.backup_state().phase(), Phase::Restoring);
    let response = ticket.restore(
        true,
        None,
        Some(&percent("not the password")),
        &mut Cursor::new(&archive),
        &mut other_box(),
        &mut Clock::new(),
    );
    assert_eq!(response.status, 400);
    assert_eq!(home.backup_state().phase(), Phase::Idle);
    let ticket = home.backup_begin(true).unwrap();
    let response = ticket.restore(
        true,
        None,
        Some(&percent(PASSWORD)),
        &mut Cursor::new(&archive),
        &mut other_box(),
        &mut Clock::new(),
    );
    assert_eq!(response.status, 200);
    assert_eq!(home.backup_state().phase(), Phase::Pending);
    assert_eq!(
        home.backup_begin(false).unwrap_err().message,
        "A restore waits for the restart"
    );
}
