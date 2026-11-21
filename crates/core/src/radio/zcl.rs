//! ZCL (Zigbee Cluster Library): Read Attributes (to identify a device), Report Attributes and
//! Configure Reporting (to read its measures), Default Response, and the IAS zone status
//! notification. Bytes in, types out; no I/O.

use std::fmt;

/// The Home Automation profile.
pub const PROFILE_HOME_AUTOMATION: u16 = 0x0104;
/// The Basic cluster.
pub const CLUSTER_BASIC: u16 = 0x0000;
/// Basic cluster attribute: manufacturer name.
pub const ATTRIBUTE_MANUFACTURER_NAME: u16 = 0x0004;
/// Basic cluster attribute: model identifier.
pub const ATTRIBUTE_MODEL_IDENTIFIER: u16 = 0x0005;

/// The IAS Zone cluster.
pub const CLUSTER_IAS_ZONE: u16 = 0x0500;
/// IAS Zone attribute: zone state (0: not enrolled, 1: enrolled).
pub const ATTRIBUTE_ZONE_STATE: u16 = 0x0000;
/// IAS Zone attribute: zone type.
pub const ATTRIBUTE_ZONE_TYPE: u16 = 0x0001;
/// IAS Zone attribute: zone status (the alarm bits).
pub const ATTRIBUTE_ZONE_STATUS: u16 = 0x0002;
/// IAS Zone attribute: the address of the coordinator the device sends its alarms to (CIE).
pub const ATTRIBUTE_CIE_ADDRESS: u16 = 0x0010;
/// The zone id the Zone Enroll Response gives (Zigbee2MQTT's value).
pub const ZONE_ID: u8 = 23;

const COMMAND_READ_ATTRIBUTES_RESPONSE: u8 = 0x01;
const COMMAND_WRITE_ATTRIBUTES: u8 = 0x02;
const COMMAND_WRITE_ATTRIBUTES_RESPONSE: u8 = 0x04;
const TYPE_EUI64: u8 = 0xF0;
/// IAS Zone, from the coordinator: Zone Enroll Response.
const COMMAND_ZONE_ENROLL_RESPONSE: u8 = 0x00;
const COMMAND_CONFIGURE_REPORTING: u8 = 0x06;
const COMMAND_CONFIGURE_REPORTING_RESPONSE: u8 = 0x07;
const COMMAND_REPORT_ATTRIBUTES: u8 = 0x0A;
const COMMAND_DEFAULT_RESPONSE: u8 = 0x0B;
/// IAS Zone, from the device: zone status change notification.
const COMMAND_ZONE_STATUS_CHANGE: u8 = 0x00;
const FRAME_TYPE_MASK: u8 = 0x03;
const FRAME_TYPE_GLOBAL: u8 = 0x00;
const FRAME_TYPE_CLUSTER: u8 = 0x01;
const MANUFACTURER_SPECIFIC: u8 = 0x04;
/// Set when the frame goes from the server (the device) to the client.
const DIRECTION_FROM_SERVER: u8 = 0x08;
const DISABLE_DEFAULT_RESPONSE: u8 = 0x10;
const STATUS_SUCCESS: u8 = 0x00;
const TYPE_CHARACTER_STRING: u8 = 0x42;

/// Something wrong in a ZCL frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZclError {
    TooShort,
    /// A well-formed frame that is not a Read Attributes response (a Default Response, for
    /// instance).
    NotReadResponse,
    /// A well-formed frame that is not the command asked for.
    WrongCommand,
    /// A manufacturer-specific frame: its attributes are not the standard ones.
    ManufacturerSpecific,
}

impl fmt::Display for ZclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZclError::TooShort => write!(f, "ZCL frame too short"),
            ZclError::NotReadResponse => write!(f, "not a Read Attributes response"),
            ZclError::WrongCommand => write!(f, "not the expected ZCL command"),
            ZclError::ManufacturerSpecific => write!(f, "manufacturer-specific ZCL frame"),
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

/// A Write Attributes request for the CIE address (attribute 0x0010, type EUI64): global command
/// 0x02, client to server, default response disabled. `eui64` is most significant byte first, as
/// the core keeps addresses; the frame carries it little-endian.
/// `10 <tsn> 02 10 00 F0 <EUI64 LE>`.
pub fn write_cie_address_request(tsn: u8, eui64: &[u8; 8]) -> Vec<u8> {
    let mut frame = vec![0x10, tsn, COMMAND_WRITE_ATTRIBUTES];
    frame.extend_from_slice(&ATTRIBUTE_CIE_ADDRESS.to_le_bytes());
    frame.push(TYPE_EUI64);
    frame.extend(eui64.iter().rev());
    frame
}

/// A Write Attributes Response: whether every attribute was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteResponse {
    pub tsn: u8,
    /// The single status byte 0x00: everything was written. Any other answer is a refusal.
    pub written: bool,
}

/// Reads a Write Attributes Response (global command 0x04).
pub fn parse_write_attributes_response(frame: &[u8]) -> Result<WriteResponse, ZclError> {
    let (header, payload) = parse_header(frame)?;
    if header.frame_type != FrameType::Global || header.command != COMMAND_WRITE_ATTRIBUTES_RESPONSE
    {
        return Err(ZclError::WrongCommand);
    }
    if payload.is_empty() {
        return Err(ZclError::TooShort);
    }
    Ok(WriteResponse {
        tsn: header.tsn,
        written: payload == [STATUS_SUCCESS],
    })
}

/// The On/Off cluster.
pub const CLUSTER_ON_OFF: u16 = 0x0006;
/// The Level Control cluster.
pub const CLUSTER_LEVEL_CONTROL: u16 = 0x0008;
/// On/Off attribute: the state (boolean). Level Control attribute: the current level (uint8).
pub const ATTRIBUTE_ON_OFF: u16 = 0x0000;
pub const ATTRIBUTE_CURRENT_LEVEL: u16 = 0x0000;

/// A cluster-specific command the coordinator sends to a device (spec 011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// On/Off cluster, On (0x01).
    On,
    /// On/Off cluster, Off (0x00).
    Off,
    /// On/Off cluster, Toggle (0x02).
    Toggle,
    /// Level Control cluster, Move to Level with On/Off (0x04): the level 0..=254, immediately.
    MoveToLevel(u8),
}

impl Command {
    /// The cluster the command belongs to.
    pub fn cluster(self) -> u16 {
        match self {
            Command::On | Command::Off | Command::Toggle => CLUSTER_ON_OFF,
            Command::MoveToLevel(_) => CLUSTER_LEVEL_CONTROL,
        }
    }

    /// The command id inside its cluster (what a Default Response carries back).
    pub fn id(self) -> u8 {
        match self {
            Command::Off => 0x00,
            Command::On => 0x01,
            Command::Toggle => 0x02,
            Command::MoveToLevel(_) => 0x04,
        }
    }

    /// The ZCL frame: frame control 0x01 (cluster-specific, client to server, a Default Response
    /// asked for), the transaction number, the command id and its payload. Move to Level is
    /// `01 <tsn> 04 <level> 00 00 00 00` (level, transition time 0 on two bytes, options mask 0,
    /// options override 0), as Zigbee2MQTT sends it.
    pub fn frame(self, tsn: u8) -> Vec<u8> {
        let mut frame = vec![FRAME_TYPE_CLUSTER, tsn, self.id()];
        if let Command::MoveToLevel(level) = self {
            frame.extend_from_slice(&[level, 0, 0, 0, 0]);
        }
        frame
    }
}

/// A Zone Enroll Response: cluster-specific command 0x00, client to server, default response
/// disabled; status success, zone id 23. `11 <tsn> 00 00 17`.
pub fn zone_enroll_response(tsn: u8) -> Vec<u8> {
    vec![
        FRAME_TYPE_CLUSTER | DISABLE_DEFAULT_RESPONSE,
        tsn,
        COMMAND_ZONE_ENROLL_RESPONSE,
        STATUS_SUCCESS,
        ZONE_ID,
    ]
}

/// A Zone Enroll Request: a device asking to be enrolled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneEnrollRequest {
    pub tsn: u8,
    pub zone_type: u16,
    /// The manufacturer code the device gives in the payload.
    pub manufacturer_code: u16,
}

/// Reads a Zone Enroll Request (cluster-specific command 0x01 from the device).
pub fn parse_zone_enroll_request(frame: &[u8]) -> Result<ZoneEnrollRequest, ZclError> {
    let (header, payload) = parse_header(frame)?;
    if header.frame_type != FrameType::Cluster
        || !header.from_server
        || header.command != COMMAND_ZONE_ENROLL_REQUEST
    {
        return Err(ZclError::WrongCommand);
    }
    let [zone_type @ .., c0, c1] = payload.get(..4).ok_or(ZclError::TooShort)? else {
        return Err(ZclError::TooShort);
    };
    Ok(ZoneEnrollRequest {
        tsn: header.tsn,
        zone_type: u16::from_le_bytes([zone_type[0], zone_type[1]]),
        manufacturer_code: u16::from_le_bytes([*c0, *c1]),
    })
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

/// What kind of command a ZCL frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// A command every cluster has (Read Attributes, Report Attributes, Default Response...).
    Global,
    /// A command of one cluster.
    Cluster,
    /// Anything else (reserved).
    Other,
}

/// The header of a ZCL frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub frame_type: FrameType,
    /// The frame goes from the device (server) to the coordinator (client).
    pub from_server: bool,
    /// The sender does not want a Default Response.
    pub disable_default_response: bool,
    pub manufacturer: Option<u16>,
    pub tsn: u8,
    pub command: u8,
}

/// Reads the header of a ZCL frame and returns it with the payload that follows.
pub fn parse_header(frame: &[u8]) -> Result<(Header, &[u8]), ZclError> {
    let control = *frame.first().ok_or(ZclError::TooShort)?;
    let mut at = 1;
    let manufacturer = if control & MANUFACTURER_SPECIFIC != 0 {
        let code = frame.get(1..3).ok_or(ZclError::TooShort)?;
        at += 2;
        Some(u16::from_le_bytes([code[0], code[1]]))
    } else {
        None
    };
    let tsn = *frame.get(at).ok_or(ZclError::TooShort)?;
    let command = *frame.get(at + 1).ok_or(ZclError::TooShort)?;
    let frame_type = match control & FRAME_TYPE_MASK {
        FRAME_TYPE_GLOBAL => FrameType::Global,
        FRAME_TYPE_CLUSTER => FrameType::Cluster,
        _ => FrameType::Other,
    };
    let header = Header {
        frame_type,
        from_server: control & DIRECTION_FROM_SERVER != 0,
        disable_default_response: control & DISABLE_DEFAULT_RESPONSE != 0,
        manufacturer,
        tsn,
        command,
    };
    Ok((header, &frame[at + 2..]))
}

/// One attribute of a report, as the device sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeRecord {
    pub attribute: u16,
    pub data_type: u8,
    /// The value's bytes, little-endian, without the length byte of a string.
    pub value: Vec<u8>,
}

/// The attributes of a Report Attributes or a Read Attributes Response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeReport {
    pub tsn: u8,
    /// Every record whose size is known, in order. At most a frame's worth: a frame holds at
    /// most 255 bytes, so a few dozen records.
    pub records: Vec<AttributeRecord>,
}

/// The size in bytes of a value of this ZCL data type, or `None` when it is not known (arrays,
/// structures, sets, bags, and the types this code does not know). Strings are sized by their
/// length prefix, not here.
fn fixed_size(data_type: u8) -> Option<usize> {
    Some(match data_type {
        0x00 => 0,
        0x08..=0x0F => usize::from(data_type - 0x07), // data8 .. data64
        0x10 => 1,                                    // boolean
        0x18..=0x1F => usize::from(data_type - 0x17), // bitmap8 .. bitmap64
        0x20..=0x27 => usize::from(data_type - 0x1F), // uint8 .. uint64
        0x28..=0x2F => usize::from(data_type - 0x27), // int8 .. int64
        0x30 => 1,                                    // enum8
        0x31 => 2,                                    // enum16
        0x38 => 2,                                    // half precision
        0x39 => 4,                                    // single precision
        0x3A => 8,                                    // double precision
        0xE0..=0xE2 => 4,                             // time of day, date, UTC time
        0xE8 | 0xE9 => 2,                             // cluster id, attribute id
        0xEA => 4,                                    // BACnet object id
        0xF0 => 8,                                    // IEEE address
        0xF1 => 16,                                   // security key
        _ => return None,
    })
}

/// Reads a value of `data_type` at the start of `bytes`: the value's bytes and what follows.
/// `None` when the type's size is not known or the bytes are cut.
fn split_value(data_type: u8, bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let (skip, length) = match data_type {
        0x41 | 0x42 => (1, usize::from(*bytes.first()?)),
        0x43 | 0x44 => (
            2,
            usize::from(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?])),
        ),
        other => (0, fixed_size(other)?),
    };
    let value = bytes.get(skip..skip + length)?;
    Some((value, &bytes[skip + length..]))
}

/// Reads a Report Attributes (0x0A) or a Read Attributes Response (0x01): the records whose size
/// is known, as raw values. A record of a type of unknown size ends the reading (the records
/// before it count), and so does a record cut short. A Read Attributes Response record whose
/// status is not success has no value and is left out.
pub fn parse_attribute_report(frame: &[u8]) -> Result<AttributeReport, ZclError> {
    let (header, mut rest) = parse_header(frame)?;
    let with_status = match (header.frame_type, header.command) {
        (FrameType::Global, COMMAND_REPORT_ATTRIBUTES) => false,
        (FrameType::Global, COMMAND_READ_ATTRIBUTES_RESPONSE) => true,
        _ => return Err(ZclError::WrongCommand),
    };
    if header.manufacturer.is_some() {
        return Err(ZclError::ManufacturerSpecific);
    }
    let mut records = Vec::new();
    while rest.len() >= 3 {
        let attribute = u16::from_le_bytes([rest[0], rest[1]]);
        rest = &rest[2..];
        if with_status {
            let status = rest[0];
            rest = &rest[1..];
            if status != STATUS_SUCCESS {
                continue;
            }
        }
        let Some((&data_type, tail)) = rest.split_first() else {
            break;
        };
        let Some((value, tail)) = split_value(data_type, tail) else {
            break;
        };
        records.push(AttributeRecord {
            attribute,
            data_type,
            value: value.to_vec(),
        });
        rest = tail;
    }
    Ok(AttributeReport {
        tsn: header.tsn,
        records,
    })
}

/// Reads an IAS Zone status change notification (cluster-specific command 0x00 from the device):
/// the zone status bits.
pub fn parse_zone_status_change(frame: &[u8]) -> Result<u16, ZclError> {
    let (header, payload) = parse_header(frame)?;
    if header.frame_type != FrameType::Cluster
        || !header.from_server
        || header.command != COMMAND_ZONE_STATUS_CHANGE
    {
        return Err(ZclError::WrongCommand);
    }
    if header.manufacturer.is_some() {
        return Err(ZclError::ManufacturerSpecific);
    }
    let status = payload.get(..2).ok_or(ZclError::TooShort)?;
    Ok(u16::from_le_bytes([status[0], status[1]]))
}

/// What to ask a device to report on one attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportingRecord {
    pub attribute: u16,
    pub data_type: u8,
    /// Seconds.
    pub min_interval: u16,
    /// Seconds.
    pub max_interval: u16,
    /// The change that triggers a report, in the attribute's own units; `None` for discrete
    /// types (boolean, bitmap), which have none.
    pub change: Option<u16>,
}

/// A Configure Reporting request: global command 0x06, client to server, default response
/// disabled. Each record: direction 0, attribute, type, minimum and maximum interval, then the
/// reportable change in the attribute's own type and size (one byte for an 8-bit type, two for a
/// 16-bit one), left out for discrete types.
pub fn configure_reporting_request(tsn: u8, records: &[ReportingRecord]) -> Vec<u8> {
    let mut frame = vec![0x10, tsn, COMMAND_CONFIGURE_REPORTING];
    for record in records {
        frame.push(0x00); // direction: the device reports
        frame.extend_from_slice(&record.attribute.to_le_bytes());
        frame.push(record.data_type);
        frame.extend_from_slice(&record.min_interval.to_le_bytes());
        frame.extend_from_slice(&record.max_interval.to_le_bytes());
        if let Some(change) = record.change {
            match fixed_size(record.data_type) {
                Some(1) => frame.push(change as u8),
                _ => frame.extend_from_slice(&change.to_le_bytes()),
            }
        }
    }
    frame
}

/// A Configure Reporting Response: whether the command failed as a whole, and the attributes
/// the device refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureReportingResponse {
    pub tsn: u8,
    /// The status of a single failure byte: the whole command failed, no attribute is named.
    pub failed: Option<u8>,
    /// `(attribute, status)` of each refusal; empty when every attribute was accepted.
    pub refused: Vec<(u16, u8)>,
}

/// Reads a Configure Reporting Response (0x07): one status byte (0x00: everything accepted;
/// any other: the whole command failed), or records of status, direction, attribute (the
/// refused ones, per the standard; a record with status 0 is read as accepted).
pub fn parse_configure_reporting_response(
    frame: &[u8],
) -> Result<ConfigureReportingResponse, ZclError> {
    let (header, payload) = parse_header(frame)?;
    if header.frame_type != FrameType::Global
        || header.command != COMMAND_CONFIGURE_REPORTING_RESPONSE
    {
        return Err(ZclError::WrongCommand);
    }
    let (failed, refused) = match payload {
        [] => return Err(ZclError::TooShort),
        [STATUS_SUCCESS] => (None, Vec::new()),
        [status] => (Some(*status), Vec::new()),
        records => (
            None,
            records
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|record| record[0] != STATUS_SUCCESS)
                .map(|record| (u16::from_le_bytes([record[2], record[3]]), record[0]))
                .collect(),
        ),
    };
    Ok(ConfigureReportingResponse {
        tsn: header.tsn,
        failed,
        refused,
    })
}

/// A Default Response: which command it answers and with what status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultResponse {
    pub tsn: u8,
    pub command: u8,
    pub status: u8,
}

/// Reads a Default Response (global command 0x0B).
pub fn parse_default_response(frame: &[u8]) -> Result<DefaultResponse, ZclError> {
    let (header, payload) = parse_header(frame)?;
    if header.frame_type != FrameType::Global || header.command != COMMAND_DEFAULT_RESPONSE {
        return Err(ZclError::WrongCommand);
    }
    let [command, status, ..] = payload else {
        return Err(ZclError::TooShort);
    };
    Ok(DefaultResponse {
        tsn: header.tsn,
        command: *command,
        status: *status,
    })
}

/// Global commands that answer a request (read, write, configure, discover responses).
const GLOBAL_RESPONSES: [u8; 9] = [0x01, 0x04, 0x07, 0x09, 0x0D, 0x10, 0x12, 0x14, 0x16];
/// IAS Zone, from the device: Zone Enroll Request, which has its own response (Enroll Response).
const COMMAND_ZONE_ENROLL_REQUEST: u8 = 0x01;
/// Default Response status: the general command is not supported.
const STATUS_UNSUPPORTED_GENERAL_COMMAND: u8 = 0x82;
/// Default Response status: the cluster command is not supported.
const STATUS_UNSUPPORTED_CLUSTER_COMMAND: u8 = 0x81;

/// The Default Response to a received frame on `cluster`, or `None` when the frame does not ask
/// for one (its "disable default response" bit is set), is itself a Default Response, or is a
/// command that has its own response (the IAS Zone Enroll Request). Status: success for reports,
/// notifications and responses; "unsupported general command" (0x82) for a global request the
/// coordinator does not serve (Read Attributes...); "unsupported cluster command" (0x81) for a
/// cluster-specific command sent to the coordinator as a server. Direction reversed,
/// transaction number echoed, manufacturer code echoed when present.
pub fn default_response(cluster: u16, received: &[u8]) -> Option<Vec<u8>> {
    let (header, _) = parse_header(received).ok()?;
    if header.disable_default_response {
        return None;
    }
    let status = match header.frame_type {
        FrameType::Global if header.command == COMMAND_DEFAULT_RESPONSE => return None,
        FrameType::Global
            if header.command == COMMAND_REPORT_ATTRIBUTES
                || GLOBAL_RESPONSES.contains(&header.command) =>
        {
            STATUS_SUCCESS
        }
        FrameType::Global => STATUS_UNSUPPORTED_GENERAL_COMMAND,
        FrameType::Cluster
            if cluster == CLUSTER_IAS_ZONE && header.command == COMMAND_ZONE_ENROLL_REQUEST =>
        {
            return None;
        }
        FrameType::Cluster if header.from_server => STATUS_SUCCESS,
        FrameType::Cluster => STATUS_UNSUPPORTED_CLUSTER_COMMAND,
        FrameType::Other => return None,
    };
    let mut control = FRAME_TYPE_GLOBAL | DISABLE_DEFAULT_RESPONSE;
    if !header.from_server {
        control |= DIRECTION_FROM_SERVER;
    }
    let mut frame = vec![control];
    if let Some(code) = header.manufacturer {
        frame[0] |= MANUFACTURER_SPECIFIC;
        frame.extend_from_slice(&code.to_le_bytes());
    }
    frame.extend_from_slice(&[header.tsn, COMMAND_DEFAULT_RESPONSE, header.command, status]);
    Some(frame)
}
