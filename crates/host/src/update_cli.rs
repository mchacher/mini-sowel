//! `--update-keygen` and `--update-sign` (spec 032, R7): the signing side of the update file, in
//! the host only (the core verifies). A refusal is a message and exit 2, with nothing written.
//! `--update-verify` (spec 038, R4) checks a signed file as the box does, installing nothing.
//! `--recipe-sign` and `--recipe-verify` (spec 042, R1.6) do the same for a script recipe's signed
//! file (`.srs`), with the recipe key: a key pair is made with `--update-keygen`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use solite_core::recipes::install::{describe_script, file as recipe_file};
use solite_core::recipes::script::{ScriptRuntimes, SystemRuntimes};
use solite_core::update::{
    HEADER_BYTES, SLOT_BYTES, Version, check_header, image_digest, parse_public_key, signed_bytes,
};

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
    /// `--update-verify <file> --public-key <64 hex>`.
    Verify { file: PathBuf, public_key: String },
    /// `--recipe-sign <file.js> --key <file> --out <file.srs>`.
    RecipeSign {
        script: PathBuf,
        key: PathBuf,
        out: PathBuf,
    },
    /// `--recipe-verify <file.srs> [--public-key <64 hex>]`: the key of the build without one.
    RecipeVerify {
        file: PathBuf,
        public_key: Option<String>,
    },
}

/// The recipe key of this build (spec 042 R1.5).
const RECIPE_PUBLIC_KEY: Option<&str> = option_env!("SOLITE_RECIPE_PUBLIC_KEY");

/// Runs a command, printing its result lines; the error is the exit status and the line to print
/// before it: 2 for a refusal, 1 for `--recipe-verify` finding the file wrong.
pub fn run(command: &Command) -> Result<(), (u8, String)> {
    match command {
        Command::Keygen(file) => keygen(file),
        Command::Sign {
            image,
            version,
            key,
            out,
        } => sign(image, version, key, out),
        Command::Verify { file, public_key } => verify(file, public_key),
        Command::RecipeSign { script, key, out } => recipe_sign(script, key, out),
        Command::RecipeVerify { file, public_key } => {
            return recipe_verify(file, public_key.as_deref());
        }
    }
    .map_err(|message| (2, message))
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

/// Checks a signed file as the box does (the header with a running version of 0.0.0 and the
/// slot's capacity, then the image's length and digest against the header), installing nothing.
fn verify(file: &Path, public_key: &str) -> Result<(), String> {
    let refuse = |reason: String| format!("update-verify: {reason}");
    let key = parse_public_key(public_key).ok_or_else(|| {
        refuse("the public key is not 64 hexadecimal characters of an Ed25519 public key".into())
    })?;
    let bytes =
        fs::read(file).map_err(|e| refuse(format!("cannot read {}: {e}", file.display())))?;
    let header: &[u8; HEADER_BYTES] = bytes
        .get(..HEADER_BYTES)
        .and_then(|head| head.try_into().ok())
        .ok_or_else(|| {
            refuse(format!(
                "{} is {} bytes, shorter than the {HEADER_BYTES}-byte header",
                file.display(),
                bytes.len()
            ))
        })?;
    let checked = check_header(header, &key, Version::new(0, 0, 0), SLOT_BYTES)
        .map_err(|refusal| refuse(refusal.message))?;
    let image = &bytes[HEADER_BYTES..];
    let length = checked.length as usize;
    if image.len() < length {
        return Err(refuse(format!(
            "the image is {} bytes, shorter than the header says ({length})",
            image.len()
        )));
    }
    if image.len() > length {
        return Err(refuse(format!(
            "the image is {} bytes, longer than the header says ({length})",
            image.len()
        )));
    }
    if image_digest(image) != checked.digest {
        return Err(refuse(
            "the image's SHA-256 differs from the header's".into(),
        ));
    }
    println!(
        "verified: {}, version {}, {} bytes, key {}",
        file.display(),
        checked.version,
        image.len(),
        &hex(&key.to_bytes())[..8]
    );
    Ok(())
}

// ---- Script recipes (spec 042 R1.6) ----

/// The runtimes the tools load a script with: the Mac's own.
fn runtimes() -> std::sync::Arc<dyn ScriptRuntimes> {
    std::sync::Arc::new(SystemRuntimes)
}

fn clock() -> solite_core::recipes::ScriptClock {
    std::sync::Arc::new(crate::serve::monotonic_ms)
}

/// Signs a script: its id and version are read from the script itself (it is loaded under the
/// rules of an installed script, so a script with no `x.y.z` version is refused).
fn recipe_sign(script: &Path, key_path: &Path, out_path: &Path) -> Result<(), String> {
    let key_text = fs::read_to_string(key_path)
        .map_err(|e| format!("recipe-sign: cannot read {}: {e}", key_path.display()))?;
    let seed = unhex32(key_text.trim()).ok_or_else(|| {
        format!(
            "recipe-sign: {} is not a key file (64 hexadecimal characters)",
            key_path.display()
        )
    })?;
    let source = fs::read_to_string(script)
        .map_err(|e| format!("recipe-sign: cannot read {}: {e}", script.display()))?;
    if source.len() > recipe_file::MAX_SOURCE_BYTES {
        return Err(format!(
            "recipe-sign: {} is {} bytes, a signed recipe holds at most {}",
            script.display(),
            source.len(),
            recipe_file::MAX_SOURCE_BYTES
        ));
    }
    let summary = describe_script(&source, &runtimes(), &clock())
        .map_err(|reason| format!("recipe-sign: {}: {reason}", script.display()))?;
    let version = summary
        .version
        .as_deref()
        .and_then(Version::parse)
        .ok_or_else(|| {
            format!(
                "recipe-sign: {} declares no x.y.z version",
                script.display()
            )
        })?;
    if out_path.symlink_metadata().is_ok() {
        return Err(format!(
            "recipe-sign: {} exists, nothing is overwritten",
            out_path.display()
        ));
    }
    let signing = SigningKey::from_bytes(&seed);
    let length =
        u32::try_from(source.len()).map_err(|_| "recipe-sign: script too large".to_string())?;
    let signed =
        recipe_file::signed_bytes(version, length, &recipe_file::digest(source.as_bytes()));
    let mut file = Vec::with_capacity(HEADER_BYTES + source.len());
    file.extend_from_slice(&signed);
    file.extend_from_slice(&signing.sign(&signed).to_bytes());
    file.extend_from_slice(source.as_bytes());
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out_path)
        .map_err(|e| format!("recipe-sign: cannot create {}: {e}", out_path.display()))?;
    out.write_all(&file)
        .and_then(|()| out.sync_all())
        .map_err(|e| {
            let _ = fs::remove_file(out_path);
            format!("recipe-sign: cannot write {}: {e}", out_path.display())
        })?;
    println!(
        "signed: {}, {} {version}, {} bytes, key {}",
        out_path.display(),
        summary.id,
        file.len(),
        &hex(&signing.verifying_key().to_bytes())[..8]
    );
    Ok(())
}

/// Checks a signed script as the box does (R1.2 and R1.3 with the recipe key), installing nothing:
/// exit 1 with the reason when it is refused, 2 when the check cannot be made.
fn recipe_verify(file: &Path, public_key: Option<&str>) -> Result<(), (u8, String)> {
    let usage = |reason: String| (2, format!("recipe-verify: {reason}"));
    let refuse = |reason: String| (1, format!("recipe-verify: {reason}"));
    let text = public_key.or(RECIPE_PUBLIC_KEY).ok_or_else(|| {
        usage("this build has no recipe key: give --public-key <64 hex>".to_string())
    })?;
    let key = parse_public_key(text).ok_or_else(|| {
        usage("the public key is not 64 hexadecimal characters of an Ed25519 public key".into())
    })?;
    let bytes =
        fs::read(file).map_err(|e| usage(format!("cannot read {}: {e}", file.display())))?;
    let signed = recipe_file::check(&bytes, &key).map_err(refuse)?;
    let summary = describe_script(&signed.source, &runtimes(), &clock()).map_err(refuse)?;
    if summary.version.as_deref().and_then(Version::parse) != Some(signed.version) {
        return Err(refuse("version differs from the signed one".to_string()));
    }
    println!(
        "verified: {}, {} {}, {} bytes, key {}",
        file.display(),
        summary.id,
        signed.version,
        bytes.len(),
        &hex(&key.to_bytes())[..8]
    );
    Ok(())
}
