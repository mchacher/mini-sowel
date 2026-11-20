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
