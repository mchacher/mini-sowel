//! The text of the Zigbee network's backup, shared by the host (`network.json`) and the firmware
//! (`zigbee.json`): the same format, written by whoever owns the files. The file holds the
//! network and trust-centre keys in plain text; protecting it is the owner's job.

use super::{Network, NetworkKeys};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// The backup of a network just formed, as the text to write.
pub fn text(network: &Network, keys: &NetworkKeys) -> String {
    format!(
        "{{\n  \"panId\": \"0x{:04X}\",\n  \"extendedPanId\": \"0x{}\",\n  \"channel\": {},\n  \
         \"networkKey\": \"{}\",\n  \"trustCenterKey\": \"{}\"\n}}\n",
        network.pan_id,
        hex(&network.extended_pan_id),
        network.channel,
        hex(&keys.network_key),
        hex(&keys.trust_center_key)
    )
}

/// The value of `"name": "value"` in a backup's text.
fn field(text: &str, name: &str) -> Option<String> {
    let after = text.split(&format!("\"{name}\": \"")).nth(1)?;
    Some(after.split('"').next()?.to_string())
}

/// True when the backup's text holds this network (same PAN id and extended PAN id).
pub fn is_network(text: &str, network: &Network) -> bool {
    field(text, "panId") == Some(format!("0x{:04X}", network.pan_id))
        && field(text, "extendedPanId") == Some(format!("0x{}", hex(&network.extended_pan_id)))
}

/// The PAN id of a backup's text, without `0x` (`1A2B`), or `unknown` when it has none: the part
/// of the name a backup of another network is kept under (`network-1A2B.json`).
pub fn pan_label(text: &str) -> String {
    field(text, "panId")
        .map(|p| p.trim_start_matches("0x").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
