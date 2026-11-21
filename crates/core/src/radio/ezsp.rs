//! EZSP (UG100), the commands the host sends to the radio. No I/O: frames in, frames out.

use std::cell::Cell;
use std::fmt;

/// Frame id of `version`.
pub const ID_VERSION: u16 = 0x0000;
/// Frame id of `networkState`.
pub const ID_NETWORK_STATE: u16 = 0x0018;
/// Frame id of `getEui64`.
pub const ID_GET_EUI64: u16 = 0x0026;

/// Oldest EZSP protocol version supported (EmberZNet 7.2).
pub const MIN_PROTOCOL_VERSION: u8 = 13;
/// Newest EZSP protocol version supported, and the one asked for first.
pub const MAX_PROTOCOL_VERSION: u8 = 19;
/// The only stack type accepted: mesh.
pub const STACK_TYPE_MESH: u8 = 2;

const FC_RESPONSE: u8 = 0x80;
const FC_OVERFLOW: u8 = 0x01;
const FC_TRUNCATED: u8 = 0x02;
const FC_ASYNC_CALLBACK: u8 = 0x10;
/// High byte of the frame control in the extended format (frame format version 1).
const FC_HIGH_EXTENDED: u8 = 0x01;

/// Something wrong in an EZSP frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EzspError {
    TooShort,
    NotAResponse,
    Truncated,
    NoCommandPending,
    WrongSequence { expected: u8, got: u8 },
    WrongFrameId { expected: u16, got: u16 },
    BadLength { expected: usize, got: usize },
    UnsupportedVersion { found: u8 },
    UnsupportedStackType(u8),
}

impl fmt::Display for EzspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EzspError::TooShort => write!(f, "EZSP frame too short"),
            EzspError::NotAResponse => write!(f, "EZSP frame is not a response"),
            EzspError::Truncated => write!(f, "EZSP response truncated by the radio"),
            EzspError::NoCommandPending => write!(f, "EZSP response without a command"),
            EzspError::WrongSequence { expected, got } => {
                write!(f, "EZSP response with sequence {got}, expected {expected}")
            }
            EzspError::WrongFrameId { expected, got } => {
                write!(
                    f,
                    "EZSP response with frame id 0x{got:04X}, expected 0x{expected:04X}"
                )
            }
            EzspError::BadLength { expected, got } => {
                write!(f, "EZSP response of {got} bytes, expected {expected}")
            }
            EzspError::UnsupportedVersion { found } => write!(
                f,
                "EZSP protocol version {found} is not supported (supported: {MIN_PROTOCOL_VERSION} to {MAX_PROTOCOL_VERSION})"
            ),
            EzspError::UnsupportedStackType(t) => {
                write!(
                    f,
                    "stack type {t} is not supported (expected {STACK_TYPE_MESH})"
                )
            }
        }
    }
}

impl std::error::Error for EzspError {}

/// State of the radio's network, as `networkState` reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    NoNetwork,
    Joining,
    Joined,
    JoinedNoParent,
    Leaving,
    Unknown(u8),
}

impl NetworkState {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => NetworkState::NoNetwork,
            1 => NetworkState::Joining,
            2 => NetworkState::Joined,
            3 => NetworkState::JoinedNoParent,
            6 => NetworkState::Leaving,
            other => NetworkState::Unknown(other),
        }
    }
}

impl fmt::Display for NetworkState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkState::NoNetwork => write!(f, "no network"),
            NetworkState::Joining => write!(f, "joining"),
            NetworkState::Joined => write!(f, "joined"),
            NetworkState::JoinedNoParent => write!(f, "joined with no parent"),
            NetworkState::Leaving => write!(f, "leaving"),
            NetworkState::Unknown(n) => write!(f, "unknown ({n})"),
        }
    }
}

/// The answer to `version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub protocol: u8,
    pub stack_type: u8,
    pub stack_version: u16,
}

/// Writes a stack version as `major.minor.patch.build` (one nibble each, as UG100).
pub fn format_stack_version(version: u16) -> String {
    format!(
        "{}.{}.{}.{}",
        (version >> 12) & 0xF,
        (version >> 8) & 0xF,
        (version >> 4) & 0xF,
        version & 0xF
    )
}

/// Builds a command frame. Legacy format: sequence, frame control, frame id (1 byte each).
/// Extended format: sequence, frame control (2 bytes), frame id (2 bytes), little-endian.
pub fn encode_command(seq: u8, frame_id: u16, params: &[u8], extended: bool) -> Vec<u8> {
    let mut frame = vec![seq, 0x00];
    if extended {
        frame.push(FC_HIGH_EXTENDED);
        frame.extend_from_slice(&frame_id.to_le_bytes());
    } else {
        // The legacy format only has room for a one-byte id.
        frame.push(frame_id.to_le_bytes()[0]);
    }
    frame.extend_from_slice(params);
    frame
}

/// An asynchronous callback frame: its frame id and its raw parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Callback {
    pub id: u16,
    pub params: Vec<u8>,
}

/// What an incoming EZSP frame is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// An asynchronous callback: not the answer to a command.
    Callback(Callback),
    /// The answer to the pending command, with its parameters.
    Response(Vec<u8>),
}

/// The parameters of a `version` response.
pub fn parse_version(params: &[u8]) -> Result<Version, EzspError> {
    if params.len() != 4 {
        return Err(EzspError::BadLength {
            expected: 4,
            got: params.len(),
        });
    }
    Ok(Version {
        protocol: params[0],
        stack_type: params[1],
        stack_version: u16::from_le_bytes([params[2], params[3]]),
    })
}

/// The parameters of a `networkState` response.
pub fn parse_network_state(params: &[u8]) -> Result<NetworkState, EzspError> {
    match params {
        [state] => Ok(NetworkState::from_u8(*state)),
        _ => Err(EzspError::BadLength {
            expected: 1,
            got: params.len(),
        }),
    }
}

/// The parameters of a `getEui64` response: the address, most significant byte first (the
/// radio sends it little-endian).
pub fn parse_eui64(params: &[u8]) -> Result<[u8; 8], EzspError> {
    let mut eui: [u8; 8] = params.try_into().map_err(|_| EzspError::BadLength {
        expected: 8,
        got: params.len(),
    })?;
    eui.reverse();
    Ok(eui)
}

/// The EZSP side of the conversation: sequence numbers, frame format, version negotiation.
#[derive(Debug, Default)]
pub struct Ezsp {
    seq: u8,
    extended: bool,
    /// Sequence and frame id of the last command, which the next response must match.
    pending: Option<(u8, u16)>,
    version: Option<u8>,
    /// Set when a frame came with the overflow flag (the radio is short of buffers).
    overflow: Cell<bool>,
}

impl Ezsp {
    pub fn new() -> Self {
        Self::default()
    }

    /// The protocol version agreed with the radio, once negotiated.
    pub fn protocol_version(&self) -> Option<u8> {
        self.version
    }

    /// Builds the next command frame.
    pub fn command(&mut self, frame_id: u16, params: &[u8]) -> Vec<u8> {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.pending = Some((seq, frame_id));
        encode_command(seq, frame_id, params, self.extended)
    }

    /// The first `version` command: legacy format, the newest version we know.
    pub fn version_command(&mut self) -> Vec<u8> {
        self.command(ID_VERSION, &[MAX_PROTOCOL_VERSION])
    }

    /// True once if a frame came with the overflow flag since the last call. The frame itself
    /// is used: the flag is a warning, not an error.
    pub fn take_overflow(&self) -> bool {
        self.overflow.replace(false)
    }

    /// Reads a frame's frame id and parameters. The format is read from the frame itself: in
    /// the extended format the byte after the frame control's low byte is its high byte, whose
    /// low bits are the format version.
    fn split(frame: &[u8]) -> Result<(u16, &[u8]), EzspError> {
        let extended = frame[2] & 0x03 == FC_HIGH_EXTENDED;
        if extended {
            if frame.len() < 5 {
                return Err(EzspError::TooShort);
            }
            Ok((u16::from_le_bytes([frame[3], frame[4]]), &frame[5..]))
        } else {
            Ok((u16::from(frame[2]), &frame[3..]))
        }
    }

    /// Checks a frame against the pending command.
    pub fn parse_response(&self, frame: &[u8]) -> Result<Parsed, EzspError> {
        if frame.len() < 3 {
            return Err(EzspError::TooShort);
        }
        let control = frame[1];
        if control & FC_OVERFLOW != 0 {
            self.overflow.set(true);
        }
        if control & FC_ASYNC_CALLBACK != 0 {
            let (id, params) = Self::split(frame)?;
            return Ok(Parsed::Callback(Callback {
                id,
                params: params.to_vec(),
            }));
        }
        if control & FC_RESPONSE == 0 {
            return Err(EzspError::NotAResponse);
        }
        if control & FC_TRUNCATED != 0 {
            return Err(EzspError::Truncated);
        }
        let (expected_seq, expected_id) = self.pending.ok_or(EzspError::NoCommandPending)?;
        if frame[0] != expected_seq {
            return Err(EzspError::WrongSequence {
                expected: expected_seq,
                got: frame[0],
            });
        }
        let (id, params) = Self::split(frame)?;
        if id != expected_id {
            return Err(EzspError::WrongFrameId {
                expected: expected_id,
                got: id,
            });
        }
        Ok(Parsed::Response(params.to_vec()))
    }

    /// Handles the answer to the first `version`: checks stack type and version range, switches
    /// to the extended format and returns the second `version` command.
    pub fn on_first_version(&mut self, params: &[u8]) -> Result<Vec<u8>, EzspError> {
        let version = parse_version(params)?;
        if version.stack_type != STACK_TYPE_MESH {
            return Err(EzspError::UnsupportedStackType(version.stack_type));
        }
        if !(MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION).contains(&version.protocol) {
            return Err(EzspError::UnsupportedVersion {
                found: version.protocol,
            });
        }
        self.extended = true;
        Ok(self.command(ID_VERSION, &[version.protocol]))
    }

    /// Handles the answer to the second `version`: the session's version is settled.
    pub fn on_second_version(&mut self, params: &[u8]) -> Result<Version, EzspError> {
        let version = parse_version(params)?;
        if !(MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION).contains(&version.protocol) {
            return Err(EzspError::UnsupportedVersion {
                found: version.protocol,
            });
        }
        self.version = Some(version.protocol);
        Ok(version)
    }
}

// ---------------------------------------------------------------------------------------------
// Commands and callbacks of the coordinator (spec 005). Layouts from UG100 and the ember adapter
// of zigbee-herdsman; the differences between EZSP 13 and 14+ are handled here, in one place.
// ---------------------------------------------------------------------------------------------

pub const ID_ADD_ENDPOINT: u16 = 0x0002;
pub const ID_GET_EXTENDED_VALUE: u16 = 0x0003;
pub const ID_SET_CONCENTRATOR: u16 = 0x0010;
pub const ID_NETWORK_INIT: u16 = 0x0017;
pub const ID_STACK_STATUS_HANDLER: u16 = 0x0019;
pub const ID_FORM_NETWORK: u16 = 0x001E;
pub const ID_PERMIT_JOINING: u16 = 0x0022;
pub const ID_TRUST_CENTER_JOIN_HANDLER: u16 = 0x0024;
pub const ID_GET_NETWORK_PARAMETERS: u16 = 0x0028;
pub const ID_SEND_UNICAST: u16 = 0x0034;
pub const ID_MESSAGE_SENT_HANDLER: u16 = 0x003F;
pub const ID_INCOMING_MESSAGE_HANDLER: u16 = 0x0045;
pub const ID_SET_CONFIGURATION_VALUE: u16 = 0x0053;
pub const ID_SET_POLICY: u16 = 0x0055;
pub const ID_SET_SOURCE_ROUTE_DISCOVERY_MODE: u16 = 0x005A;
pub const ID_LOOKUP_EUI64_BY_NODE_ID: u16 = 0x0061;
pub const ID_SET_INITIAL_SECURITY_STATE: u16 = 0x0068;
pub const ID_CLEAR_TRANSIENT_LINK_KEYS: u16 = 0x006B;
pub const ID_SET_VALUE: u16 = 0x00AB;
pub const ID_CLEAR_KEY_TABLE: u16 = 0x00B1;
pub const ID_IMPORT_TRANSIENT_KEY: u16 = 0x0111;

/// First EZSP protocol version with 32-bit statuses and 16-bit message tags.
const FIRST_WIDE_PROTOCOL: u8 = 14;

/// True when the protocol version uses the EZSP 14+ layouts.
pub fn is_wide(protocol: u8) -> bool {
    protocol >= FIRST_WIDE_PROTOCOL
}

// Statuses, as `SLStatus` (EZSP 14+). An EZSP 13 radio sends `EmberStatus` in one byte: it is
// converted to the same values on reading, so the rest of the code knows one set.
pub const STATUS_OK: u32 = 0;
pub const STATUS_BUSY: u32 = 0x0004;
pub const STATUS_NETWORK_UP: u32 = 0x0015;
pub const STATUS_NETWORK_DOWN: u32 = 0x0016;
pub const STATUS_NOT_JOINED: u32 = 0x0017;
pub const STATUS_ALLOCATION_FAILED: u32 = 0x0019;
pub const STATUS_DELIVERY_FAILED: u32 = 0x0C02;
pub const STATUS_MAX_MESSAGE_LIMIT_REACHED: u32 = 0x0C03;
/// An `EmberStatus` this code does not know is reported as `0x0001_0000` plus its value.
const STATUS_UNKNOWN_EMBER: u32 = 0x0001_0000;

fn ember_to_sl(status: u8) -> u32 {
    match status {
        0x00 => STATUS_OK,
        0x18 => STATUS_ALLOCATION_FAILED,
        0x66 => STATUS_DELIVERY_FAILED,
        0x72 => STATUS_MAX_MESSAGE_LIMIT_REACHED,
        0x90 => STATUS_NETWORK_UP,
        0x91 => STATUS_NETWORK_DOWN,
        0x93 => STATUS_NOT_JOINED,
        0x9C => 0x0C18, // network opened
        0x9D => 0x0C19, // network closed
        0xA1 => STATUS_BUSY,
        other => STATUS_UNKNOWN_EMBER | u32::from(other),
    }
}

/// True for the statuses of a radio that cannot take the message now: worth trying again.
pub fn status_is_busy(status: u32) -> bool {
    matches!(
        status,
        STATUS_BUSY | STATUS_ALLOCATION_FAILED | STATUS_MAX_MESSAGE_LIMIT_REACHED
    )
}

/// A cursor over the parameters of a frame. Reading past the end is `TooShort`; bytes left at
/// the end are fine (newer stacks append fields).
struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], EzspError> {
        if self.bytes.len() < n {
            return Err(EzspError::TooShort);
        }
        let (head, tail) = self.bytes.split_at(n);
        self.bytes = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, EzspError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, EzspError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, EzspError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A 64-bit address, sent little-endian, returned most significant byte first.
    fn eui64(&mut self) -> Result<[u8; 8], EzspError> {
        let mut eui = [0u8; 8];
        eui.copy_from_slice(self.take(8)?);
        eui.reverse();
        Ok(eui)
    }

    fn status(&mut self, wide: bool) -> Result<u32, EzspError> {
        if wide {
            self.u32()
        } else {
            Ok(ember_to_sl(self.u8()?))
        }
    }

    fn rest(self) -> &'a [u8] {
        self.bytes
    }
}

/// Reads the status at the start of a response (one byte on EZSP 13, four on 14+) and returns it
/// with the parameters that follow.
pub fn read_status(wide: bool, params: &[u8]) -> Result<(u32, &[u8]), EzspError> {
    let mut reader = Reader::new(params);
    let status = reader.status(wide)?;
    Ok((status, reader.rest()))
}

/// The parameters of `setConfigurationValue`.
pub fn set_configuration_value(id: u8, value: u16) -> Vec<u8> {
    let v = value.to_le_bytes();
    vec![id, v[0], v[1]]
}

/// The parameters of `setPolicy`.
pub fn set_policy(id: u8, decision: u8) -> Vec<u8> {
    vec![id, decision]
}

/// The parameters of `setValue`: value id, length, bytes.
pub fn set_value(id: u8, bytes: &[u8]) -> Vec<u8> {
    let mut params = vec![id, bytes.len() as u8];
    params.extend_from_slice(bytes);
    params
}

/// The parameters of `getExtendedValue(ENDPOINT_FLAGS)` for one endpoint.
pub fn endpoint_flags_query(endpoint: u8) -> Vec<u8> {
    let mut params = vec![0x00];
    params.extend_from_slice(&u32::from(endpoint).to_le_bytes());
    params
}

/// The parameters of `addEndpoint`.
pub fn add_endpoint(
    endpoint: u8,
    profile: u16,
    device_id: u16,
    inputs: &[u16],
    outputs: &[u16],
) -> Vec<u8> {
    let mut params = vec![endpoint];
    params.extend_from_slice(&profile.to_le_bytes());
    params.extend_from_slice(&device_id.to_le_bytes());
    params.push(1); // device version
    params.push(inputs.len() as u8);
    params.push(outputs.len() as u8);
    for cluster in inputs.iter().chain(outputs) {
        params.extend_from_slice(&cluster.to_le_bytes());
    }
    params
}

/// The parameters of `networkInit`: parent info in token, end devices rejoin on reboot.
pub fn network_init() -> Vec<u8> {
    0x0003u16.to_le_bytes().to_vec()
}

/// The parameters of `setInitialSecurityState`, 43 bytes: bitmask 0x0B84, the trust centre's
/// root key, the network key, key sequence 0, no preconfigured trust-centre address.
pub fn initial_security_state(trust_center_key: &[u8; 16], network_key: &[u8; 16]) -> Vec<u8> {
    let mut params = 0x0B84u16.to_le_bytes().to_vec();
    params.extend_from_slice(trust_center_key);
    params.extend_from_slice(network_key);
    params.push(0);
    params.extend_from_slice(&[0xFF; 8]);
    params
}

/// The network parameters of `formNetwork` and `getNetworkParameters` (20 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkParameters {
    /// As sent on the wire (little-endian).
    pub extended_pan_id: [u8; 8],
    pub pan_id: u16,
    pub radio_tx_power: u8,
    pub radio_channel: u8,
    pub join_method: u8,
    pub nwk_manager_id: u16,
    pub nwk_update_id: u8,
    pub channels: u32,
}

/// All the channels 11 to 26.
pub const ALL_CHANNELS: u32 = 0x07FF_F800;

impl NetworkParameters {
    /// The parameters of a new network: power 5 dBm, MAC association, manager 0, all channels.
    pub fn new(extended_pan_id: [u8; 8], pan_id: u16, channel: u8) -> Self {
        NetworkParameters {
            extended_pan_id,
            pan_id,
            radio_tx_power: 5,
            radio_channel: channel,
            join_method: 0,
            nwk_manager_id: 0,
            nwk_update_id: 0,
            channels: ALL_CHANNELS,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.extended_pan_id.to_vec();
        bytes.extend_from_slice(&self.pan_id.to_le_bytes());
        bytes.push(self.radio_tx_power);
        bytes.push(self.radio_channel);
        bytes.push(self.join_method);
        bytes.extend_from_slice(&self.nwk_manager_id.to_le_bytes());
        bytes.push(self.nwk_update_id);
        bytes.extend_from_slice(&self.channels.to_le_bytes());
        bytes
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, EzspError> {
        let mut extended_pan_id = [0u8; 8];
        extended_pan_id.copy_from_slice(reader.take(8)?);
        Ok(NetworkParameters {
            extended_pan_id,
            pan_id: reader.u16()?,
            radio_tx_power: reader.u8()?,
            radio_channel: reader.u8()?,
            join_method: reader.u8()?,
            nwk_manager_id: reader.u16()?,
            nwk_update_id: reader.u8()?,
            channels: reader.u32()?,
        })
    }
}

/// The node type of a radio that coordinates its network.
pub const NODE_TYPE_COORDINATOR: u8 = 1;

/// The answer to `getNetworkParameters`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkParametersResponse {
    pub status: u32,
    pub node_type: u8,
    pub parameters: NetworkParameters,
}

pub fn parse_network_parameters(
    wide: bool,
    params: &[u8],
) -> Result<NetworkParametersResponse, EzspError> {
    let mut reader = Reader::new(params);
    let status = reader.status(wide)?;
    let node_type = reader.u8()?;
    Ok(NetworkParametersResponse {
        status,
        node_type,
        parameters: NetworkParameters::read(&mut reader)?,
    })
}

/// The parameters of `setConcentrator`: on, high-RAM, 5 s to 60 s, error thresholds 3 and 1,
/// no hop limit.
pub fn set_concentrator() -> Vec<u8> {
    let mut params = vec![1];
    params.extend_from_slice(&0xFFF9u16.to_le_bytes());
    params.extend_from_slice(&5u16.to_le_bytes());
    params.extend_from_slice(&60u16.to_le_bytes());
    params.extend_from_slice(&[3, 1, 0]);
    params
}

/// The well-known Zigbee link key, "ZigBeeAlliance09".
pub const WELL_KNOWN_LINK_KEY: [u8; 16] = *b"ZigBeeAlliance09";

/// The parameters of `importTransientKey` for any device (address ff×8). EZSP 13 adds a flags
/// byte.
pub fn import_transient_key(wide: bool, key: &[u8; 16]) -> Vec<u8> {
    let mut params = vec![0xFF; 8];
    params.extend_from_slice(key);
    if !wide {
        params.push(0);
    }
    params
}

/// An APS frame (11 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApsFrame {
    pub profile: u16,
    pub cluster: u16,
    pub source_endpoint: u8,
    pub destination_endpoint: u8,
    pub options: u16,
    pub group_id: u16,
    pub sequence: u8,
}

/// Retry, route discovery and address discovery.
pub const APS_OPTIONS_DEFAULT: u16 = 0x1140;

impl ApsFrame {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.profile.to_le_bytes().to_vec();
        bytes.extend_from_slice(&self.cluster.to_le_bytes());
        bytes.push(self.source_endpoint);
        bytes.push(self.destination_endpoint);
        bytes.extend_from_slice(&self.options.to_le_bytes());
        bytes.extend_from_slice(&self.group_id.to_le_bytes());
        bytes.push(self.sequence);
        bytes
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, EzspError> {
        Ok(ApsFrame {
            profile: reader.u16()?,
            cluster: reader.u16()?,
            source_endpoint: reader.u8()?,
            destination_endpoint: reader.u8()?,
            options: reader.u16()?,
            group_id: reader.u16()?,
            sequence: reader.u8()?,
        })
    }
}

/// The parameters of `sendUnicast` to a node address (type "direct"): the message tag is one
/// byte on EZSP 13 and two on 14+.
pub fn send_unicast(
    wide: bool,
    destination: u16,
    aps: &ApsFrame,
    tag: u16,
    message: &[u8],
) -> Vec<u8> {
    let mut params = vec![0x00];
    params.extend_from_slice(&destination.to_le_bytes());
    params.extend_from_slice(&aps.encode());
    if wide {
        params.extend_from_slice(&tag.to_le_bytes());
    } else {
        params.push(tag as u8);
    }
    params.push(message.len() as u8);
    params.extend_from_slice(message);
    params
}

/// The answer to `sendUnicast`: status and the APS sequence number.
pub fn parse_send_unicast(wide: bool, params: &[u8]) -> Result<(u32, u8), EzspError> {
    let mut reader = Reader::new(params);
    let status = reader.status(wide)?;
    Ok((status, reader.u8()?))
}

/// What a `stackStatusHandler` callback says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackStatus {
    NetworkUp,
    NetworkDown,
    Other(u32),
}

pub fn parse_stack_status(wide: bool, params: &[u8]) -> Result<StackStatus, EzspError> {
    Ok(match Reader::new(params).status(wide)? {
        STATUS_NETWORK_UP => StackStatus::NetworkUp,
        STATUS_NETWORK_DOWN => StackStatus::NetworkDown,
        other => StackStatus::Other(other),
    })
}

/// A `trustCenterJoinHandler` callback: a node joined or rejoined (or left).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustCenterJoin {
    pub node_id: u16,
    /// Most significant byte first.
    pub eui64: [u8; 8],
    /// `EmberDeviceUpdate`: 1 unsecured join, 3 unsecured rejoin, 2 device left.
    pub status: u8,
    /// `EmberJoinDecision`: 2 is "deny".
    pub decision: u8,
}

pub const DEVICE_UPDATE_UNSECURED_JOIN: u8 = 1;
pub const DEVICE_UPDATE_UNSECURED_REJOIN: u8 = 3;
pub const JOIN_DECISION_DENY: u8 = 2;

pub fn parse_trust_center_join(params: &[u8]) -> Result<TrustCenterJoin, EzspError> {
    let mut reader = Reader::new(params);
    Ok(TrustCenterJoin {
        node_id: reader.u16()?,
        eui64: reader.eui64()?,
        status: reader.u8()?,
        decision: reader.u8()?,
    })
}

/// An `incomingMessageHandler` callback: a message from the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMessage {
    pub kind: u8,
    pub aps: ApsFrame,
    /// The sender's node address.
    pub sender: u16,
    /// The sender's 64-bit address, most significant byte first, when the radio gave it (EZSP
    /// 14+ only; all zeros means unknown).
    pub sender_eui64: Option<[u8; 8]>,
    pub payload: Vec<u8>,
}

/// Message kinds of `incomingMessageHandler` that are a unicast.
pub const INCOMING_UNICAST: u8 = 0x00;
pub const INCOMING_UNICAST_REPLY: u8 = 0x01;

impl IncomingMessage {
    /// True for a message sent to this node alone (not a broadcast, nor a multicast).
    pub fn is_unicast(&self) -> bool {
        matches!(self.kind, INCOMING_UNICAST | INCOMING_UNICAST_REPLY)
    }
}

/// Reads the callback in its EZSP 13 layout (fields one by one) or its 14+ layout (an 18-byte
/// packet info: sender, sender's 64-bit address, indexes, link quality, signal, timestamp).
pub fn parse_incoming_message(wide: bool, params: &[u8]) -> Result<IncomingMessage, EzspError> {
    let mut reader = Reader::new(params);
    let kind = reader.u8()?;
    let aps = ApsFrame::read(&mut reader)?;
    let (sender, sender_eui64) = if wide {
        let sender = reader.u16()?;
        let eui64 = reader.eui64()?;
        reader.take(1 + 1 + 1 + 1 + 4)?; // indexes, lqi, rssi, timestamp
        (sender, (eui64 != [0; 8]).then_some(eui64))
    } else {
        reader.take(2)?; // link quality, signal
        let sender = reader.u16()?;
        reader.take(2)?; // binding and address indexes
        (sender, None)
    };
    let length = usize::from(reader.u8()?);
    let payload = reader.take(length)?.to_vec();
    Ok(IncomingMessage {
        kind,
        aps,
        sender,
        sender_eui64,
        payload,
    })
}

/// The parameters of `lookupEui64ByNodeId`.
pub fn lookup_eui64_by_node_id(node_id: u16) -> Vec<u8> {
    node_id.to_le_bytes().to_vec()
}

/// The answer to `lookupEui64ByNodeId`: the address (most significant byte first), or `None`
/// when the radio's address table does not know the node.
pub fn parse_lookup_eui64(wide: bool, params: &[u8]) -> Result<Option<[u8; 8]>, EzspError> {
    let mut reader = Reader::new(params);
    if reader.status(wide)? != STATUS_OK {
        return Ok(None);
    }
    let eui64 = reader.eui64()?;
    Ok((eui64 != [0; 8]).then_some(eui64))
}

/// A `messageSentHandler` callback: what became of a message we sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageSent {
    pub status: u32,
    pub destination: u16,
    pub aps: ApsFrame,
    pub tag: u16,
}

pub fn parse_message_sent(wide: bool, params: &[u8]) -> Result<MessageSent, EzspError> {
    let mut reader = Reader::new(params);
    if wide {
        let status = reader.u32()?;
        reader.u8()?; // type
        let destination = reader.u16()?;
        let aps = ApsFrame::read(&mut reader)?;
        let tag = reader.u16()?;
        Ok(MessageSent {
            status,
            destination,
            aps,
            tag,
        })
    } else {
        reader.u8()?; // type
        let destination = reader.u16()?;
        let aps = ApsFrame::read(&mut reader)?;
        let tag = u16::from(reader.u8()?);
        let status = reader.status(false)?;
        Ok(MessageSent {
            status,
            destination,
            aps,
            tag,
        })
    }
}
