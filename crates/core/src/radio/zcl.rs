//! ZCL (Zigbee Cluster Library): Read Attributes, which is all the coordinator needs to identify
//! a device. Bytes in, types out; no I/O.

use std::fmt;

/// The Home Automation profile.
pub const PROFILE_HOME_AUTOMATION: u16 = 0x0104;
/// The Basic cluster.
pub const CLUSTER_BASIC: u16 = 0x0000;
/// Basic cluster attribute: manufacturer name.
pub const ATTRIBUTE_MANUFACTURER_NAME: u16 = 0x0004;
/// Basic cluster attribute: model identifier.
pub const ATTRIBUTE_MODEL_IDENTIFIER: u16 = 0x0005;

const COMMAND_READ_ATTRIBUTES_RESPONSE: u8 = 0x01;
const FRAME_TYPE_MASK: u8 = 0x03;
const FRAME_TYPE_GLOBAL: u8 = 0x00;
const MANUFACTURER_SPECIFIC: u8 = 0x04;
const STATUS_SUCCESS: u8 = 0x00;
const TYPE_CHARACTER_STRING: u8 = 0x42;

/// Something wrong in a ZCL frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZclError {
    TooShort,
    /// A well-formed frame that is not a Read Attributes response (a Default Response, for
    /// instance).
    NotReadResponse,
}

impl fmt::Display for ZclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZclError::TooShort => write!(f, "ZCL frame too short"),
            ZclError::NotReadResponse => write!(f, "not a Read Attributes response"),
        }
    }
}

impl std::error::Error for ZclError {}

/// A Read Attributes request: global command, client to server, default response disabled.
/// `10 <tsn> 00 <attribute LE>...`.
pub fn read_attributes_request(tsn: u8, attributes: &[u16]) -> Vec<u8> {
    let mut frame = vec![0x10, tsn, 0x00];
    for attribute in attributes {
        frame.extend_from_slice(&attribute.to_le_bytes());
    }
    frame
}

/// A Read Attributes response: its transaction number and, per attribute, the string it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadResponse {
    pub tsn: u8,
    /// `(attribute id, value)`; the value is `None` when the attribute is unsupported, is not a
    /// character string, or its string does not fit the frame.
    pub records: Vec<(u16, Option<String>)>,
}

impl ReadResponse {
    /// The value of one attribute, if the response has it as a string.
    pub fn string(&self, attribute: u16) -> Option<&str> {
        self.records
            .iter()
            .find(|(id, _)| *id == attribute)
            .and_then(|(_, value)| value.as_deref())
    }
}

/// Reads a Read Attributes response. A record that cannot be read (unsupported attribute, a
/// string longer than the frame, a type other than a character string) gives `None`; the
/// records after one whose size is unknown are dropped.
pub fn parse_read_attributes_response(frame: &[u8]) -> Result<ReadResponse, ZclError> {
    let control = *frame.first().ok_or(ZclError::TooShort)?;
    let mut at = 1;
    if control & MANUFACTURER_SPECIFIC != 0 {
        at += 2;
    }
    let tsn = *frame.get(at).ok_or(ZclError::TooShort)?;
    let command = *frame.get(at + 1).ok_or(ZclError::TooShort)?;
    if control & FRAME_TYPE_MASK != FRAME_TYPE_GLOBAL || command != COMMAND_READ_ATTRIBUTES_RESPONSE
    {
        return Err(ZclError::NotReadResponse);
    }
    let mut records = Vec::new();
    let mut rest = &frame[at + 2..];
    while rest.len() >= 3 {
        let id = u16::from_le_bytes([rest[0], rest[1]]);
        let status = rest[2];
        rest = &rest[3..];
        if status != STATUS_SUCCESS {
            records.push((id, None));
            continue;
        }
        let [data_type, tail @ ..] = rest else {
            records.push((id, None));
            break;
        };
        if *data_type != TYPE_CHARACTER_STRING {
            // The size of other types is not known here: stop reading.
            records.push((id, None));
            break;
        }
        let [length, tail @ ..] = tail else {
            records.push((id, None));
            break;
        };
        let length = usize::from(*length);
        let Some(bytes) = tail.get(..length) else {
            records.push((id, None));
            break;
        };
        records.push((id, text(bytes)));
        rest = &tail[length..];
    }
    Ok(ReadResponse { tsn, records })
}

/// A string as the device sends it: trailing 0x00 bytes removed, read lossily if not UTF-8. An
/// empty string is `None`.
fn text(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    let value = String::from_utf8_lossy(&bytes[..end]).into_owned();
    (!value.is_empty()).then_some(value)
}
