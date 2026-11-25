//! The backup of the network the host formed: `network.json` in the data folder, readable by its
//! owner only (the file holds the network's keys). This host targets macOS and Linux.
//!
//! Nothing reads the keys back; the file is kept so a later restore is possible. The PAN id and
//! the extended PAN id are read to tell the host's own network from another program's.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use solite_core::backup::BackupFiles;
use solite_core::radio::backup::{is_network, pan_label, parse, text as backup_text};
use solite_core::radio::{
    Network, NetworkKeys, NetworkSettings, Radio, RadioError, RadioTransport,
};

use crate::backup_files::{DataFiles, NETWORK_FILE as FILE_NAME};

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

fn cannot_write(dir: &Path) -> String {
    format!("cannot write {}", dir.display())
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
    let text = backup_text(network, keys);
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
    let pan = pan_label(&text);
    let _ = fs::rename(path(dir), dir.join(format!("network-{pan}.json")));
}

/// True when the backup holds this network (same PAN id and extended PAN id).
pub fn matches(dir: &Path, network: &Network) -> bool {
    let Ok(text) = fs::read_to_string(path(dir)) else {
        return false;
    };
    is_network(&text, network)
}

/// Starts the radio's network, for `--pair` and `--serve`: resumes the one the radio has, or forms
/// one (the keys are written to the backup the moment the radio accepts it, before any later step
/// that can fail). Returns the network, and whether it is one this host has no backup of.
pub fn start<T: RadioTransport, C: FnMut() -> u64>(
    radio: &mut Radio<T, C>,
    data: &Path,
    fill: &mut dyn FnMut(&mut [u8]) -> Result<(), String>,
) -> Result<(Network, bool), String> {
    // The restored network (spec 035): the file's values when the radio has none, or, with the
    // mark a restore left, the radio is moved onto it.
    let mut files = DataFiles::new(data);
    let restored = match fs::read_to_string(path(data)) {
        Ok(text) => {
            let parsed = parse(&text);
            if parsed.is_none() {
                log::warn!("backup: {FILE_NAME} cannot be read, a network is formed as usual");
            }
            parsed
        }
        Err(_) => None,
    };
    let replace = files.restore_mark()?;
    let network = radio
        .start_network(
            &NetworkSettings::default(),
            restored.as_ref(),
            replace,
            fill,
            &mut |network| match &network.keys {
                Some(keys) => write(data, network, keys),
                None => Ok(()),
            },
        )
        .map_err(|e| match e {
            // The backup's own message, as it is.
            RadioError::OnFormed(message) => message,
            other => format!("radio: {other}"),
        })?;
    if replace {
        files.clear_restore_mark()?;
    }
    let foreign = network.keys.is_none() && !matches(data, &network);
    Ok((network, foreign))
}
