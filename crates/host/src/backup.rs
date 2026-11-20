//! The backup of the network the host formed: `network.json` in the data folder, readable by its
//! owner only (the file holds the network's keys). This host targets macOS and Linux.
//!
//! Nothing reads the keys back; the file is kept so a later restore is possible. The PAN id and
//! the extended PAN id are read to tell the host's own network from another program's.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use mini_sowel_core::radio::{Network, NetworkKeys};

const FILE_NAME: &str = "network.json";

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

fn cannot_write(dir: &Path) -> String {
    format!("cannot write {}", dir.display())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// Makes sure the data folder exists and can be written, before the radio is touched.
pub fn check_writable(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|_| cannot_write(dir))?;
    let probe = dir.join(".write-test");
    fs::write(&probe, b"").map_err(|_| cannot_write(dir))?;
    fs::remove_file(&probe).map_err(|_| cannot_write(dir))
}

/// Writes `network.json` (mode 0600) for a network just formed. The file is written under a
/// temporary name, then renamed, so a stop in the middle never leaves half a file.
pub fn write(dir: &Path, network: &Network, keys: &NetworkKeys) -> Result<(), String> {
    let text = format!(
        "{{\n  \"panId\": \"0x{:04X}\",\n  \"extendedPanId\": \"0x{}\",\n  \"channel\": {},\n  \
         \"networkKey\": \"{}\",\n  \"trustCenterKey\": \"{}\"\n}}\n",
        network.pan_id,
        hex(&network.extended_pan_id),
        network.channel,
        hex(&keys.network_key),
        hex(&keys.trust_center_key)
    );
    let target = path(dir);
    keep_other_backup(dir, network);
    let temporary = dir.join(format!("{FILE_NAME}.tmp"));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        // The mode of a file that already existed is not changed by `open`.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, &target)
    })();
    result.map_err(|_| cannot_write(&target))
}

/// A backup of another network is kept, renamed `network-<PAN id>.json` (the PAN id of that
/// backup), before the new one is written. Best effort: a failure shows when the new file is
/// written.
fn keep_other_backup(dir: &Path, network: &Network) {
    let Ok(text) = fs::read_to_string(path(dir)) else {
        return;
    };
    if is_network(&text, network) {
        return;
    }
    let pan = field(&text, "panId")
        .map(|p| p.trim_start_matches("0x").to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let _ = fs::rename(path(dir), dir.join(format!("network-{pan}.json")));
}

fn is_network(text: &str, network: &Network) -> bool {
    field(text, "panId") == Some(format!("0x{:04X}", network.pan_id))
        && field(text, "extendedPanId") == Some(format!("0x{}", hex(&network.extended_pan_id)))
}

/// The value of `"name": "value"` in the file's text.
fn field(text: &str, name: &str) -> Option<String> {
    let after = text.split(&format!("\"{name}\": \"")).nth(1)?;
    Some(after.split('"').next()?.to_string())
}

/// True when the backup holds this network (same PAN id and extended PAN id).
pub fn matches(dir: &Path, network: &Network) -> bool {
    let Ok(text) = fs::read_to_string(path(dir)) else {
        return false;
    };
    is_network(&text, network)
}
