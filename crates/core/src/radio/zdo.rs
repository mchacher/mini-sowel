//! ZDO (Zigbee Device Objects): the few management messages the coordinator uses. Bytes in,
//! types out; no I/O. A ZDO message is one transaction byte followed by the cluster's payload.

use std::fmt;

/// The ZDO profile.
pub const PROFILE: u16 = 0x0000;
/// `Device_annce`: a device says it is on the network.
pub const CLUSTER_DEVICE_ANNOUNCE: u16 = 0x0013;
/// `Active_EP_req`.
pub const CLUSTER_ACTIVE_ENDPOINTS_REQUEST: u16 = 0x0005;
/// `Active_EP_rsp`.
pub const CLUSTER_ACTIVE_ENDPOINTS_RESPONSE: u16 = 0x8005;

/// `Simple_Desc_req`.
pub const CLUSTER_SIMPLE_DESCRIPTOR_REQUEST: u16 = 0x0004;
/// `Simple_Desc_rsp`.
pub const CLUSTER_SIMPLE_DESCRIPTOR_RESPONSE: u16 = 0x8004;
/// `Bind_req`.
pub const CLUSTER_BIND_REQUEST: u16 = 0x0021;
/// `Bind_rsp`.
pub const CLUSTER_BIND_RESPONSE: u16 = 0x8021;

/// Something wrong in a ZDO message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZdoError {
    TooShort,
    /// The device answered with a status other than success.
    Status(u8),
}

impl fmt::Display for ZdoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZdoError::TooShort => write!(f, "ZDO message too short"),
            ZdoError::Status(s) => write!(f, "ZDO status 0x{s:02X}"),
        }
    }
}

impl std::error::Error for ZdoError {}

/// A `Device_annce`: the device's node address and 64-bit address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAnnounce {
    pub nwk: u16,
    /// Most significant byte first.
    pub eui64: [u8; 8],
    /// The capability byte (bit 2: mains powered), if the message has it.
    pub capability: Option<u8>,
}

/// Reads a `Device_annce` message: transaction, node address, 64-bit address, capabilities.
pub fn parse_device_announce(message: &[u8]) -> Result<DeviceAnnounce, ZdoError> {
    if message.len() < 1 + 2 + 8 {
        return Err(ZdoError::TooShort);
    }
    let mut eui64 = [0u8; 8];
    eui64.copy_from_slice(&message[3..11]);
    eui64.reverse();
    Ok(DeviceAnnounce {
        nwk: u16::from_le_bytes([message[1], message[2]]),
        eui64,
        capability: message.get(11).copied(),
    })
}

/// An `Active_EP_req` message for the node `nwk`: `[tsn][nwk LE]`.
pub fn active_endpoints_request(tsn: u8, nwk: u16) -> Vec<u8> {
    let n = nwk.to_le_bytes();
    vec![tsn, n[0], n[1]]
}

/// An `Active_EP_rsp`: its transaction number and the endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveEndpoints {
    pub tsn: u8,
    pub nwk: u16,
    pub endpoints: Vec<u8>,
}

/// Reads an `Active_EP_rsp` message: transaction, status, node address, count, endpoints. A
/// status other than success is an error.
pub fn parse_active_endpoints_response(message: &[u8]) -> Result<ActiveEndpoints, ZdoError> {
    if message.len() < 2 {
        return Err(ZdoError::TooShort);
    }
    if message[1] != 0 {
        return Err(ZdoError::Status(message[1]));
    }
    if message.len() < 5 {
        return Err(ZdoError::TooShort);
    }
    let count = usize::from(message[4]);
    let endpoints = message.get(5..5 + count).ok_or(ZdoError::TooShort)?;
    Ok(ActiveEndpoints {
        tsn: message[0],
        nwk: u16::from_le_bytes([message[2], message[3]]),
        endpoints: endpoints.to_vec(),
    })
}

/// A `Simple_Desc_req` message: `[tsn][nwk LE][endpoint]`.
pub fn simple_descriptor_request(tsn: u8, nwk: u16, endpoint: u8) -> Vec<u8> {
    let n = nwk.to_le_bytes();
    vec![tsn, n[0], n[1], endpoint]
}

/// A `Simple_Desc_rsp`: what one endpoint of a device is and which clusters it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleDescriptor {
    pub tsn: u8,
    pub nwk: u16,
    pub endpoint: u8,
    pub profile: u16,
    pub device_id: u16,
    pub input_clusters: Vec<u16>,
    pub output_clusters: Vec<u16>,
}

/// Reads a `Simple_Desc_rsp`: transaction, status, node address, descriptor length, then the
/// descriptor (endpoint, profile, device id, version, input clusters, output clusters). A status
/// other than success is an error.
pub fn parse_simple_descriptor_response(message: &[u8]) -> Result<SimpleDescriptor, ZdoError> {
    if message.len() < 2 {
        return Err(ZdoError::TooShort);
    }
    if message[1] != 0 {
        return Err(ZdoError::Status(message[1]));
    }
    let mut reader = Cursor {
        bytes: &message[2..],
    };
    let nwk = reader.u16()?;
    let _length = reader.u8()?;
    let endpoint = reader.u8()?;
    let profile = reader.u16()?;
    let device_id = reader.u16()?;
    let _version = reader.u8()?;
    let input_clusters = reader.clusters()?;
    let output_clusters = reader.clusters()?;
    Ok(SimpleDescriptor {
        tsn: message[0],
        nwk,
        endpoint,
        profile,
        device_id,
        input_clusters,
        output_clusters,
    })
}

/// A cursor over a message: reading past the end is `TooShort`.
struct Cursor<'a> {
    bytes: &'a [u8],
}

impl Cursor<'_> {
    fn u8(&mut self) -> Result<u8, ZdoError> {
        let (first, rest) = self.bytes.split_first().ok_or(ZdoError::TooShort)?;
        self.bytes = rest;
        Ok(*first)
    }

    fn u16(&mut self) -> Result<u16, ZdoError> {
        Ok(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }

    /// A count, then that many cluster ids (at most 255 of them: the count is one byte).
    fn clusters(&mut self) -> Result<Vec<u16>, ZdoError> {
        let count = self.u8()?;
        (0..count).map(|_| self.u16()).collect()
    }
}

/// A `Bind_req` message: `[tsn][source EUI64 LE][source endpoint][cluster LE][destination
/// address mode 3][destination EUI64 LE][destination endpoint]`. The addresses are given most
/// significant byte first, as everywhere in the core.
pub fn bind_request(
    tsn: u8,
    source: [u8; 8],
    source_endpoint: u8,
    cluster: u16,
    destination: [u8; 8],
    destination_endpoint: u8,
) -> Vec<u8> {
    let mut message = vec![tsn];
    message.extend(source.iter().rev());
    message.push(source_endpoint);
    message.extend_from_slice(&cluster.to_le_bytes());
    message.push(0x03); // destination is a 64-bit address and an endpoint
    message.extend(destination.iter().rev());
    message.push(destination_endpoint);
    message
}

/// Reads a `Bind_rsp` message: transaction, status. Returns the transaction; a status other
/// than success is an error.
pub fn parse_bind_response(message: &[u8]) -> Result<u8, ZdoError> {
    if message.len() < 2 {
        return Err(ZdoError::TooShort);
    }
    if message[1] != 0 {
        return Err(ZdoError::Status(message[1]));
    }
    Ok(message[0])
}
