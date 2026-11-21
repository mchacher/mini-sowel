//! The simulated radio's flash: the network and the devices that joined, kept in a JSON file in
//! the host's data folder (or the box's data partition) so a second run finds them.

use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::devices::SimDevice;
use crate::ncp::StoredNetwork;

/// Network key then trust-centre key.
pub type Keys = ([u8; 16], [u8; 16]);

/// The network as the file keeps it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NetworkMemory {
    /// 16 hexadecimal digits, as the radio receives it (little-endian).
    extended_pan_wire: String,
    pan_id: u16,
    channel: u8,
    node_type: u8,
    /// 32 hexadecimal digits each, when the radio received them.
    network_key: Option<String>,
    trust_center_key: Option<String>,
}

/// One joined device as the file keeps it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeviceMemory {
    /// 16 hexadecimal digits, as on the wire.
    eui64_wire: String,
    nwk: u16,
    joined: bool,
    bound: bool,
    configured: Vec<u16>,
    configured_attributes: Vec<(u16, u16)>,
    reports_sent: bool,
    relay_on: bool,
    cie_written: bool,
    enrolled: bool,
    /// 16 hexadecimal digits, as on the wire: the CIE address written to the IAS zone; absent
    /// (zeros) in a file written before it was kept.
    #[serde(default)]
    cie: String,
    /// The level of a dimmer; absent in a file written before spec 011.
    #[serde(default)]
    level: u8,
    /// The device answers no On/Off or Level Control frame (a test switch, spec 011).
    #[serde(default)]
    silent: bool,
    /// The radio's address table does not know the device (a test switch).
    #[serde(default)]
    lookup_unknown: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Memory {
    network: Option<NetworkMemory>,
    pub devices: Vec<DeviceMemory>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != 2 * N || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

impl DeviceMemory {
    pub fn eui_wire(&self) -> [u8; 8] {
        unhex(&self.eui64_wire).unwrap_or([0; 8])
    }

    /// Puts the remembered state back into the device.
    pub fn apply_to(&self, device: &mut SimDevice) {
        device.nwk = self.nwk;
        device.joined = self.joined;
        device.bound = self.bound;
        device.configured = self.configured.clone();
        device.configured_attributes = self.configured_attributes.clone();
        device.reports_sent = self.reports_sent;
        device.relay_on = self.relay_on;
        device.cie_written = self.cie_written;
        device.enrolled = self.enrolled;
        device.cie = unhex(&self.cie).unwrap_or([0; 8]);
        device.level = self.level;
        device.silent = self.silent;
        device.lookup_unknown = self.lookup_unknown;
    }
}

impl Memory {
    pub fn new(
        network: Option<&StoredNetwork>,
        keys: Option<&Keys>,
        devices: &[SimDevice],
    ) -> Memory {
        Memory {
            network: network.map(|n| NetworkMemory {
                extended_pan_wire: hex(&n.extended_pan_wire),
                pan_id: n.pan_id,
                channel: n.channel,
                node_type: n.node_type,
                network_key: keys.map(|k| hex(&k.0)),
                trust_center_key: keys.map(|k| hex(&k.1)),
            }),
            devices: devices
                .iter()
                .filter(|d| d.joined)
                .map(|d| DeviceMemory {
                    eui64_wire: hex(&d.eui_wire),
                    nwk: d.nwk,
                    joined: d.joined,
                    bound: d.bound,
                    configured: d.configured.clone(),
                    configured_attributes: d.configured_attributes.clone(),
                    reports_sent: d.reports_sent,
                    relay_on: d.relay_on,
                    cie_written: d.cie_written,
                    enrolled: d.enrolled,
                    cie: hex(&d.cie),
                    level: d.level,
                    silent: d.silent,
                    lookup_unknown: d.lookup_unknown,
                })
                .collect(),
        }
    }

    /// The network and the keys, when the file holds a well-formed network.
    pub fn network_and_keys(&self) -> (Option<StoredNetwork>, Option<Keys>) {
        let Some(n) = &self.network else {
            return (None, None);
        };
        let Some(extended_pan_wire) = unhex(&n.extended_pan_wire) else {
            return (None, None);
        };
        let network = StoredNetwork {
            extended_pan_wire,
            pan_id: n.pan_id,
            channel: n.channel,
            node_type: n.node_type,
        };
        let keys = match (&n.network_key, &n.trust_center_key) {
            (Some(nk), Some(tc)) => unhex(nk).zip(unhex(tc)),
            _ => None,
        };
        (Some(network), keys)
    }
}

/// Reads the memory file: `None` when there is none. A file that cannot be read or parsed is an
/// error naming it.
pub fn load(file: &Path) -> Result<Option<Memory>, String> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("sim memory: {}: {e}", file.display())),
    };
    let memory: Memory =
        serde_json::from_str(&text).map_err(|e| format!("sim memory: {}: {e}", file.display()))?;
    // A file that names an address the format cannot hold is as broken as invalid JSON.
    if let Some(bad) = memory
        .devices
        .iter()
        .find(|d| unhex::<8>(&d.eui64_wire).is_none())
    {
        return Err(format!(
            "sim memory: {}: bad device address \"{}\"",
            file.display(),
            bad.eui64_wire
        ));
    }
    if let Some(n) = &memory.network
        && (unhex::<8>(&n.extended_pan_wire).is_none()
            || n.network_key
                .as_deref()
                .is_some_and(|k| unhex::<16>(k).is_none())
            || n.trust_center_key
                .as_deref()
                .is_some_and(|k| unhex::<16>(k).is_none()))
    {
        return Err(format!(
            "sim memory: {}: bad network identifiers",
            file.display()
        ));
    }
    Ok(Some(memory))
}

/// Writes the memory file: a temporary file, then a rename, so a stop in the middle never leaves
/// half a file. Creates the folder if it is missing.
pub fn save(file: &Path, memory: &Memory) -> Result<(), String> {
    let cannot =
        |e: &dyn std::fmt::Display| format!("sim memory: {}: cannot write: {e}", file.display());
    if let Some(dir) = file.parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir).map_err(|e| cannot(&e))?;
    }
    let text = serde_json::to_string_pretty(memory).map_err(|e| cannot(&e))?;
    let mut temporary = file.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    // Owner only, like the host's own backup. The ESP32's FAT partition has no permissions: the
    // file is created with the default mode there.
    #[cfg(unix)]
    options.mode(0o600);
    let mut out = options.open(&temporary).map_err(|e| cannot(&e))?;
    out.write_all(text.as_bytes()).map_err(|e| cannot(&e))?;
    // A rename onto an existing file is refused on the ESP32's FAT partition: the old file goes
    // first (the new one is complete by now).
    if fs::rename(&temporary, file).is_err() {
        let _ = fs::remove_file(file);
        fs::rename(&temporary, file).map_err(|e| cannot(&e))?;
    }
    Ok(())
}
