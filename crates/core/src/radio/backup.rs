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

/// A network read back from a backup's text (spec 035, R7): what a restored box forms again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredNetwork {
    pub pan_id: u16,
    /// Most significant byte first, as the file shows it (the reverse of the frame's order).
    pub extended_pan_id: [u8; 8],
    pub channel: u8,
    pub keys: NetworkKeys,
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let digits = text.strip_prefix("0x").unwrap_or(text).as_bytes();
    if digits.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (byte, pair) in out.iter_mut().zip(digits.chunks(2)) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

/// The number after `"channel":` in a backup's text.
fn channel(text: &str) -> Option<u8> {
    let after = text.split("\"channel\":").nth(1)?;
    after
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// The network of a backup's text: `None` when a field is missing or is not valid (a PAN id of
/// 0x0000 or 0xFFFF, an extended PAN id of all 0x00 or all 0xFF, a channel outside 11 to 26, a
/// key that is not 16 bytes in hexadecimal).
pub fn parse(text: &str) -> Option<RestoredNetwork> {
    let pan_id = u16::from_be_bytes(unhex::<2>(&field(text, "panId")?)?);
    let extended_pan_id = unhex::<8>(&field(text, "extendedPanId")?)?;
    let channel = channel(text)?;
    let keys = NetworkKeys {
        network_key: unhex::<16>(&field(text, "networkKey")?)?,
        trust_center_key: unhex::<16>(&field(text, "trustCenterKey")?)?,
    };
    let valid = pan_id != 0x0000
        && pan_id != 0xFFFF
        && extended_pan_id != [0x00; 8]
        && extended_pan_id != [0xFF; 8]
        && (11..=26).contains(&channel);
    valid.then_some(RestoredNetwork {
        pan_id,
        extended_pan_id,
        channel,
        keys,
    })
}
