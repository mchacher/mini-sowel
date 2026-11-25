//! `--update-keygen` and `--update-sign` (spec 032, R7): the signing side of the update file, in
//! the host only (the core verifies). A refusal is a message and exit 2, with nothing written.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use solite_core::update::{HEADER_BYTES, SLOT_BYTES, Version, image_digest, signed_bytes};

use crate::random::SystemRandom;

/// What the command line asks of the update tools.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `--update-keygen <file>`.
    Keygen(PathBuf),
    /// `--update-sign <image> --version <x.y.z> --key <file> --out <file>`.
    Sign {
        image: PathBuf,
        version: String,
        key: PathBuf,
        out: PathBuf,
    },
}

/// Runs a command, printing its result lines; the error is the line to print before exit 2.
pub fn run(command: &Command) -> Result<(), String> {
    match command {
        Command::Keygen(file) => keygen(file),
        Command::Sign {
            image,
            version,
            key,
            out,
        } => sign(image, version, key, out),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        let mut pair = text[2 * i..2 * i + 2].chars();
        let high = pair.next()?.to_digit(16)?;
        let low = pair.next()?.to_digit(16)?;
        *byte = (high * 16 + low) as u8;
    }
    Some(out)
}

/// True when the file's folder or one above it holds a `.git` (a repository's working tree).
fn inside_a_working_tree(file: &Path) -> Result<bool, String> {
    let folder = match file.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let folder = folder
        .canonicalize()
        .map_err(|e| format!("update-keygen: {}: {e}", folder.display()))?;
    Ok(folder.ancestors().any(|dir| dir.join(".git").exists()))
}

fn keygen(file: &Path) -> Result<(), String> {
    if file.symlink_metadata().is_ok() {
        return Err(format!(
            "update-keygen: {} exists: a signing key is never overwritten",
            file.display()
        ));
    }
    if inside_a_working_tree(file)? {
        return Err(format!(
            "update-keygen: {} is inside a git working tree: keep the signing key outside the repository",
            file.display()
        ));
    }
    let mut seed = [0u8; 32];
    SystemRandom::open()?.fill(&mut seed)?;
    let public = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(file)
        .map_err(|e| format!("update-keygen: cannot create {}: {e}", file.display()))?;
    out.write_all(format!("{}\n", hex(&seed)).as_bytes())
        .and_then(|()| out.sync_all())
        .map_err(|e| format!("update-keygen: cannot write {}: {e}", file.display()))?;
    println!("public key: {}", hex(&public));
    Ok(())
}

/// The version of an ESP32 application image: its first byte is `0xE9` and the application
/// descriptor's magic `0xABCD5432` is at offset 32; the version is the 32 bytes at offset 48,
/// NUL-terminated. `None` for bytes without a descriptor.
fn descriptor_version(image: &[u8]) -> Option<String> {
    if image.len() < 80 || image[0] != 0xE9 {
        return None;
    }
    if image[32..36] != 0xABCD_5432u32.to_le_bytes() {
        return None;
    }
    let field = &image[48..80];
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    Some(String::from_utf8_lossy(&field[..end]).into_owned())
}

fn sign(image_path: &Path, version: &str, key_path: &Path, out_path: &Path) -> Result<(), String> {
    let parsed = Version::parse(version).ok_or_else(|| {
        format!("update-sign: version {version:?} is not x.y.z of numbers up to 65535")
    })?;
    let key_text = fs::read_to_string(key_path)
        .map_err(|e| format!("update-sign: cannot read {}: {e}", key_path.display()))?;
    let seed = unhex32(key_text.trim()).ok_or_else(|| {
        format!(
            "update-sign: {} is not a key file (64 hexadecimal characters)",
            key_path.display()
        )
    })?;
    let image = fs::read(image_path)
        .map_err(|e| format!("update-sign: cannot read {}: {e}", image_path.display()))?;
    if image.is_empty() {
        return Err(format!("update-sign: {} is empty", image_path.display()));
    }
    if image.len() as u64 > SLOT_BYTES {
        return Err(format!(
            "update-sign: {} is {} bytes, the update slot holds {SLOT_BYTES}",
            image_path.display(),
            image.len()
        ));
    }
    match descriptor_version(&image) {
        Some(inside) if inside != parsed.to_string() => {
            return Err(format!(
                "update-sign: the image's application descriptor says version {inside:?}, not {parsed}"
            ));
        }
        Some(_) => {}
        None => println!("signed: no application descriptor, version not checked"),
    }
    if out_path.symlink_metadata().is_ok() {
        return Err(format!(
            "update-sign: {} exists, nothing is overwritten",
            out_path.display()
        ));
    }
    let signing = SigningKey::from_bytes(&seed);
    let length =
        u32::try_from(image.len()).map_err(|_| "update-sign: image too large".to_string())?;
    let signed = signed_bytes(parsed, length, &image_digest(&image));
    let mut file = Vec::with_capacity(HEADER_BYTES + image.len());
    file.extend_from_slice(&signed);
    file.extend_from_slice(&signing.sign(&signed).to_bytes());
    file.extend_from_slice(&image);
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .map_err(|e| format!("update-sign: cannot create {}: {e}", out_path.display()))?;
    out.write_all(&file)
        .and_then(|()| out.sync_all())
        .map_err(|e| {
            let _ = fs::remove_file(out_path);
            format!("update-sign: cannot write {}: {e}", out_path.display())
        })?;
    println!(
        "signed: {}, version {parsed}, {} bytes, key {}",
        out_path.display(),
        image.len(),
        &hex(&signing.verifying_key().to_bytes())[..8]
    );
    Ok(())
}
