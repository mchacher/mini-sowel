//! The radio as a Zigbee coordinator: start or resume its network, open pairing, follow the
//! devices that join and identify them (manufacturer and model, from the Basic cluster).
//!
//! No clock and no random source of its own: the time comes from the `Radio`'s clock, the random
//! bytes from the caller.

use std::fmt;

use super::{Radio, RadioError, RadioTransport};
use super::{ezsp, zcl, zdo};
use ezsp::{ApsFrame, Callback, IncomingMessage, NetworkParameters, StackStatus, TrustCenterJoin};

/// Wait for the stack to report the network up, in milliseconds.
const NETWORK_UP_TIMEOUT_MS: u64 = 10_000;
/// A device that joined and did not announce itself is identified after this wait.
const ANNOUNCE_WAIT_MS: u64 = 5_000;
/// Wait for a device's answer to a ZCL request, in milliseconds.
const ANSWER_TIMEOUT_MS: u64 = 10_000;
/// Wait for a device's answer to a ZDO request, in milliseconds (Zigbee2MQTT's value).
const ZDO_ANSWER_TIMEOUT_MS: u64 = 15_000;
/// A request answered "busy" is sent again after this long, in milliseconds.
const BUSY_RETRY_MS: u64 = 500;
/// A request answered "busy" this many times fails the identification.
const MAX_BUSY: u8 = 3;
/// The Basic read is sent once more if the device did not answer: this many sends in all.
const MAX_READS: u8 = 2;
/// Devices followed in one session; one past it is reported and ignored.
const MAX_DEVICES: usize = 32;
/// Devices reported as "too many", remembered to report each once; bounded like the devices.
const MAX_IGNORED: usize = 32;
/// The host's endpoint, Home Automation profile.
const HOST_ENDPOINT: u8 = 1;
/// The Zigbee channels.
const CHANNELS: std::ops::RangeInclusive<u8> = 11..=26;

/// Settings of the network to form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSettings {
    pub channel: u8,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings { channel: 15 }
    }
}

/// The keys of a network the radio formed. Never printed or logged: `Debug` hides them.
#[derive(Clone, PartialEq, Eq)]
pub struct NetworkKeys {
    pub network_key: [u8; 16],
    pub trust_center_key: [u8; 16],
}

impl fmt::Debug for NetworkKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NetworkKeys(hidden)")
    }
}

/// The network the radio runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub pan_id: u16,
    /// Most significant byte first.
    pub extended_pan_id: [u8; 8],
    pub channel: u8,
    /// True when this start formed the network, false when it resumed one the radio had.
    pub formed: bool,
    /// The keys, only for a network just formed (a resumed one keeps its keys in the radio).
    pub keys: Option<NetworkKeys>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// The `network:` line of the host. `foreign` is true for a resumed network the host has no
/// backup of.
pub fn network_line(network: &Network, foreign: bool) -> String {
    let state = if network.formed {
        "formed"
    } else if foreign {
        "resumed (made by another program)"
    } else {
        "resumed"
    };
    format!(
        "network: {state}, channel {}, PAN 0x{:04X}, extended PAN 0x{}",
        network.channel,
        network.pan_id,
        hex(&network.extended_pan_id)
    )
}

/// What happens while pairing is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingEvent {
    /// A device joined the network (once per device per session).
    Joined { eui64: [u8; 8], nwk: u16 },
    /// A device was identified; a string the device did not give is `None`.
    Identified {
        eui64: [u8; 8],
        manufacturer: Option<String>,
        model: Option<String>,
    },
    /// The radio stayed busy: the device could not be asked.
    NotIdentified { eui64: [u8; 8] },
    /// More devices than the session follows: this one is ignored.
    TooManyDevices { eui64: [u8; 8] },
}

impl PairingEvent {
    /// The line the host prints for this event.
    pub fn line(&self) -> String {
        match self {
            PairingEvent::Joined { eui64, nwk } => {
                format!("joined: 0x{} (0x{nwk:04X})", hex(eui64))
            }
            PairingEvent::Identified {
                eui64,
                manufacturer,
                model,
            } => format!(
                "device: 0x{} {} {}",
                hex(eui64),
                manufacturer.as_deref().unwrap_or("unknown"),
                model.as_deref().unwrap_or("unknown")
            ),
            PairingEvent::NotIdentified { eui64 } => {
                format!("device: 0x{} not identified (radio busy)", hex(eui64))
            }
            PairingEvent::TooManyDevices { eui64 } => {
                format!("pairing: too many devices, 0x{} ignored", hex(eui64))
            }
        }
    }
}

/// Where a device is in its identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Ask for the active endpoints at `due`.
    SendEndpoints,
    WaitEndpoints {
        tsn: u8,
        until: u64,
    },
    /// Read the Basic cluster at `due`.
    SendRead,
    WaitRead {
        tsn: u8,
        until: u64,
    },
    Done,
}

#[derive(Debug)]
struct Device {
    eui64: [u8; 8],
    nwk: u16,
    step: Step,
    due: u64,
    busy: u8,
    reads: u8,
    endpoint: u8,
}

/// What the coordinator follows while pairing.
#[derive(Debug, Default)]
pub struct Pairing {
    /// At most `MAX_DEVICES`.
    devices: Vec<Device>,
    /// At most `MAX_IGNORED`.
    ignored: Vec<[u8; 8]>,
    tsn: u8,
}

/// Configuration values set before the network starts: (id, value).
const CONFIGURATION: [(u8, u16); 9] = [
    (0x19, 2),    // trust-centre address cache size
    (0x12, 7680), // indirect transmission timeout (sleepy children)
    (0x10, 30),   // max hops
    (0x2D, 1),    // supported networks
    (0x0C, 2),    // stack profile: Zigbee PRO
    (0x0D, 5),    // security level
    (0x11, 32),   // max end device children
    (0x13, 8),    // end device poll timeout
    (0x36, 300),  // transient key timeout, seconds
];

/// Policies set before the network starts: (id, decision).
const POLICIES: [(u8, u8); 5] = [
    (1, 0x12), // binding modification: valid endpoint clusters only
    (4, 0x40), // message contents in callback: tag only
    (5, 0x51), // trust-centre key requests: allowed, current key sent
    (6, 0x60), // application key requests: denied
    (0, 0x02), // trust-centre policy: rejoins only (no new joins until pairing opens)
];
const POLICY_TRUST_CENTER: u8 = 0;
const TRUST_CENTER_ALLOW_JOINS: u8 = 0x03;
const TRUST_CENTER_REJOINS_ONLY: u8 = 0x02;

const ENDPOINT_PROFILE: u16 = zcl::PROFILE_HOME_AUTOMATION;
const ENDPOINT_DEVICE_ID: u16 = 0x0065;
const ENDPOINT_INPUTS: [u16; 7] = [0x0000, 0x0003, 0x0006, 0x0008, 0x000A, 0x0019, 0x0300];
const ENDPOINT_OUTPUTS: [u16; 18] = [
    0x0000, 0x0003, 0x0004, 0x0005, 0x0006, 0x0008, 0x0020, 0x0300, 0x0400, 0x0402, 0x0405, 0x0406,
    0x0500, 0x0702, 0x0B01, 0x0B03, 0x0B04, 0x1000,
];

impl<T: RadioTransport, C: FnMut() -> u64> Radio<T, C> {
    fn wide(&self) -> Result<bool, RadioError> {
        self.ezsp
            .protocol_version()
            .map(ezsp::is_wide)
            .ok_or(RadioError::NotIdentified)
    }

    fn call(&mut self, id: u16, params: &[u8]) -> Result<Vec<u8>, RadioError> {
        let command = self.ezsp.command(id, params);
        self.exchange(command)
    }

    fn call_status(&mut self, id: u16, params: &[u8], wide: bool) -> Result<u32, RadioError> {
        let response = self.call(id, params)?;
        Ok(ezsp::read_status(wide, &response)?.0)
    }

    /// Sends a command whose refusal ends the start.
    fn require_ok(
        &mut self,
        name: &'static str,
        id: u16,
        params: &[u8],
        wide: bool,
    ) -> Result<(), RadioError> {
        match self.call_status(id, params, wide)? {
            ezsp::STATUS_OK => Ok(()),
            status => Err(RadioError::Refused {
                command: name,
                status,
            }),
        }
    }

    /// Sends a command whose refusal is only logged (the radio's own value is then used).
    fn best_effort(
        &mut self,
        name: &'static str,
        id: u16,
        params: &[u8],
        wide: bool,
    ) -> Result<(), RadioError> {
        let status = self.call_status(id, params, wide)?;
        if status != ezsp::STATUS_OK {
            log::warn!("the radio refused {name} (status 0x{status:X}), its own value is used");
        }
        Ok(())
    }

    /// Starts the network: resumes the one the radio has, or forms a new one. `random` fills a
    /// buffer with random bytes (the core has no source of its own). `on_formed` is called with
    /// the new network and its keys right after the radio accepted `formNetwork`, before any
    /// later step that can fail, so the caller can keep the keys; its error ends the start.
    pub fn start_network(
        &mut self,
        settings: &NetworkSettings,
        random: &mut dyn FnMut(&mut [u8]) -> Result<(), String>,
        on_formed: &mut dyn FnMut(&Network) -> Result<(), String>,
    ) -> Result<Network, RadioError> {
        if !CHANNELS.contains(&settings.channel) {
            return Err(RadioError::InvalidArgument("channel must be 11 to 26"));
        }
        let wide = self.wide()?;
        self.configure(wide)?;

        let status = self.call_status(ezsp::ID_NETWORK_INIT, &ezsp::network_init(), wide)?;
        let keys = match status {
            ezsp::STATUS_OK => {
                self.wait_for_network_up()?;
                None
            }
            ezsp::STATUS_NOT_JOINED => Some(self.form(settings, random, on_formed, wide)?),
            status => {
                return Err(RadioError::Refused {
                    command: "networkInit",
                    status,
                });
            }
        };

        let response = self.call(ezsp::ID_GET_NETWORK_PARAMETERS, &[])?;
        let parameters = ezsp::parse_network_parameters(wide, &response)?;
        if parameters.status != ezsp::STATUS_OK {
            return Err(RadioError::Refused {
                command: "getNetworkParameters",
                status: parameters.status,
            });
        }
        // A radio that is only a member of a network is never left: that would destroy a
        // network the user may need.
        if parameters.node_type != ezsp::NODE_TYPE_COORDINATOR {
            return Err(RadioError::NotCoordinator);
        }

        self.require_ok(
            "setConcentrator",
            ezsp::ID_SET_CONCENTRATOR,
            &ezsp::set_concentrator(),
            wide,
        )?;
        // The answer is the time to the next route request, in milliseconds: not needed.
        self.call(ezsp::ID_SET_SOURCE_ROUTE_DISCOVERY_MODE, &[2])?;

        let mut extended_pan_id = parameters.parameters.extended_pan_id;
        extended_pan_id.reverse();
        Ok(Network {
            pan_id: parameters.parameters.pan_id,
            extended_pan_id,
            channel: parameters.parameters.radio_channel,
            formed: keys.is_some(),
            keys,
        })
    }

    /// Stack configuration, policies and the host's endpoint, all before the network starts.
    fn configure(&mut self, wide: bool) -> Result<(), RadioError> {
        for (id, value) in CONFIGURATION {
            let params = ezsp::set_configuration_value(id, value);
            self.best_effort(
                "setConfigurationValue",
                ezsp::ID_SET_CONFIGURATION_VALUE,
                &params,
                wide,
            )?;
        }
        for (id, decision) in POLICIES {
            let params = ezsp::set_policy(id, decision);
            self.best_effort("setPolicy", ezsp::ID_SET_POLICY, &params, wide)?;
        }
        // Transient device timeout: 10 s, in milliseconds.
        let params = ezsp::set_value(0x43, &10_000u16.to_le_bytes());
        self.best_effort("setValue", ezsp::ID_SET_VALUE, &params, wide)?;

        // The endpoint can only be added once: ask first whether it is there.
        let flags = self.call_status(
            ezsp::ID_GET_EXTENDED_VALUE,
            &ezsp::endpoint_flags_query(HOST_ENDPOINT),
            wide,
        )?;
        if flags != ezsp::STATUS_OK {
            let params = ezsp::add_endpoint(
                HOST_ENDPOINT,
                ENDPOINT_PROFILE,
                ENDPOINT_DEVICE_ID,
                &ENDPOINT_INPUTS,
                &ENDPOINT_OUTPUTS,
            );
            self.require_ok("addEndpoint", ezsp::ID_ADD_ENDPOINT, &params, wide)?;
        }
        Ok(())
    }

    /// Forms a new network with random keys and identifiers.
    fn form(
        &mut self,
        settings: &NetworkSettings,
        random: &mut dyn FnMut(&mut [u8]) -> Result<(), String>,
        on_formed: &mut dyn FnMut(&Network) -> Result<(), String>,
        wide: bool,
    ) -> Result<NetworkKeys, RadioError> {
        let mut fill = |buffer: &mut [u8]| random(buffer).map_err(RadioError::Random);
        let mut keys = NetworkKeys {
            network_key: [0; 16],
            trust_center_key: [0; 16],
        };
        fill(&mut keys.network_key)?;
        fill(&mut keys.trust_center_key)?;
        // An extended PAN of all 0x00 or all 0xFF is not valid: drawn again.
        let mut extended_pan_id = [0u8; 8];
        loop {
            fill(&mut extended_pan_id)?;
            if extended_pan_id != [0x00; 8] && extended_pan_id != [0xFF; 8] {
                break;
            }
        }
        // PAN 0x0000 and 0xFFFF are not valid either.
        let mut pan = [0u8; 2];
        let pan_id = loop {
            fill(&mut pan)?;
            let pan_id = u16::from_le_bytes(pan);
            if pan_id != 0x0000 && pan_id != 0xFFFF {
                break pan_id;
            }
        };

        let security = ezsp::initial_security_state(&keys.trust_center_key, &keys.network_key);
        self.require_ok(
            "setInitialSecurityState",
            ezsp::ID_SET_INITIAL_SECURITY_STATE,
            &security,
            wide,
        )?;
        // Extended security bitmask: joiners may use the global link key; no leave request.
        let bitmask = ezsp::set_value(0x0A, &0x0110u16.to_le_bytes());
        self.best_effort("setValue", ezsp::ID_SET_VALUE, &bitmask, wide)?;
        self.best_effort("clearKeyTable", ezsp::ID_CLEAR_KEY_TABLE, &[], wide)?;
        let parameters = NetworkParameters::new(extended_pan_id, pan_id, settings.channel);
        self.require_ok(
            "formNetwork",
            ezsp::ID_FORM_NETWORK,
            &parameters.encode(),
            wide,
        )?;
        let mut shown_pan = extended_pan_id;
        shown_pan.reverse();
        on_formed(&Network {
            pan_id,
            extended_pan_id: shown_pan,
            channel: settings.channel,
            formed: true,
            keys: Some(keys.clone()),
        })
        .map_err(RadioError::OnFormed)?;
        self.wait_for_network_up()?;
        // Stack token writing on: the radio keeps the network in its flash.
        let writing = ezsp::set_value(0x07, &[1]);
        self.best_effort("setValue", ezsp::ID_SET_VALUE, &writing, wide)?;
        Ok(keys)
    }

    /// Waits for the "network up" stack status, which may already be queued.
    fn wait_for_network_up(&mut self) -> Result<(), RadioError> {
        let wide = self.wide()?;
        let started = (self.now)();
        loop {
            self.drain_payloads();
            let mut up = false;
            self.callbacks.retain(|callback| {
                if callback.id != ezsp::ID_STACK_STATUS_HANDLER {
                    return true;
                }
                if let Ok(StackStatus::NetworkUp) = ezsp::parse_stack_status(wide, &callback.params)
                {
                    up = true;
                }
                false
            });
            if up {
                return Ok(());
            }
            if (self.now)().saturating_sub(started) >= NETWORK_UP_TIMEOUT_MS {
                return Err(RadioError::NetworkDidNotStart);
            }
            self.step()?;
        }
    }

    /// Opens joining for `seconds` (1 to 254) with the well-known link key.
    pub fn permit_join(&mut self, seconds: u8) -> Result<(), RadioError> {
        if !(1..=254).contains(&seconds) {
            return Err(RadioError::InvalidArgument(
                "pairing time must be 1 to 254 seconds",
            ));
        }
        let wide = self.wide()?;
        let key = ezsp::import_transient_key(wide, &ezsp::WELL_KNOWN_LINK_KEY);
        // This command answers with a 32-bit status on every version.
        let response = self.call(ezsp::ID_IMPORT_TRANSIENT_KEY, &key)?;
        match ezsp::read_status(true, &response)?.0 {
            ezsp::STATUS_OK => {}
            status => {
                return Err(RadioError::Refused {
                    command: "importTransientKey",
                    status,
                });
            }
        }
        self.require_ok(
            "setPolicy",
            ezsp::ID_SET_POLICY,
            &ezsp::set_policy(POLICY_TRUST_CENTER, TRUST_CENTER_ALLOW_JOINS),
            wide,
        )?;
        self.require_ok("permitJoining", ezsp::ID_PERMIT_JOINING, &[seconds], wide)
    }

    /// Closes joining: permit joining 0, transient keys cleared, policy back to rejoins only.
    /// Every step is tried; the first error is returned.
    pub fn close_joining(&mut self) -> Result<(), RadioError> {
        let wide = self.wide()?;
        let permit = self.require_ok("permitJoining", ezsp::ID_PERMIT_JOINING, &[0], wide);
        let keys = self
            .call(ezsp::ID_CLEAR_TRANSIENT_LINK_KEYS, &[])
            .map(|_| ());
        let policy = self.require_ok(
            "setPolicy",
            ezsp::ID_SET_POLICY,
            &ezsp::set_policy(POLICY_TRUST_CENTER, TRUST_CENTER_REJOINS_ONLY),
            wide,
        );
        permit.and(keys).and(policy)
    }

    /// True while a device that joined is still being identified.
    pub fn pairing_busy(&self) -> bool {
        self.pairing.devices.iter().any(|d| d.step != Step::Done)
    }

    /// Reads the radio for a moment, follows the devices and returns what happened.
    pub fn poll_pairing(&mut self) -> Result<Vec<PairingEvent>, RadioError> {
        let wide = self.wide()?;
        self.step()?;
        self.drain_payloads();
        let mut events = Vec::new();
        while let Some(callback) = self.callbacks.pop_front() {
            self.on_callback(wide, callback, &mut events);
        }
        self.drive_devices(wide, &mut events)?;
        Ok(events)
    }

    fn on_callback(&mut self, wide: bool, callback: Callback, events: &mut Vec<PairingEvent>) {
        match callback.id {
            ezsp::ID_STACK_STATUS_HANDLER => {
                if let Ok(StackStatus::NetworkDown) =
                    ezsp::parse_stack_status(wide, &callback.params)
                {
                    log::warn!("the radio's network went down");
                }
            }
            ezsp::ID_TRUST_CENTER_JOIN_HANDLER => {
                if let Ok(join) = ezsp::parse_trust_center_join(&callback.params) {
                    self.on_trust_center_join(join, events);
                }
            }
            ezsp::ID_INCOMING_MESSAGE_HANDLER => {
                if let Ok(message) = ezsp::parse_incoming_message(wide, &callback.params) {
                    self.on_incoming(message, events);
                }
            }
            // What became of a message is not needed: the answer, or its absence, tells.
            ezsp::ID_MESSAGE_SENT_HANDLER => {}
            _ => {}
        }
    }

    fn on_trust_center_join(&mut self, join: TrustCenterJoin, events: &mut Vec<PairingEvent>) {
        let joined = matches!(
            join.status,
            ezsp::DEVICE_UPDATE_UNSECURED_JOIN | ezsp::DEVICE_UPDATE_UNSECURED_REJOIN
        ) && join.decision != ezsp::JOIN_DECISION_DENY;
        if joined {
            self.register(join.eui64, join.node_id, false, events);
        }
    }

    fn on_incoming(&mut self, message: IncomingMessage, events: &mut Vec<PairingEvent>) {
        let now = (self.now)();
        match (message.aps.profile, message.aps.cluster) {
            (zdo::PROFILE, zdo::CLUSTER_DEVICE_ANNOUNCE) => {
                if let Ok(announce) = zdo::parse_device_announce(&message.payload) {
                    self.register(announce.eui64, announce.nwk, true, events);
                }
            }
            (zdo::PROFILE, zdo::CLUSTER_ACTIVE_ENDPOINTS_RESPONSE) => {
                let Some(device) = self
                    .pairing
                    .devices
                    .iter_mut()
                    .find(|d| d.nwk == message.sender)
                else {
                    return;
                };
                let Step::WaitEndpoints { tsn, .. } = device.step else {
                    return;
                };
                if message.payload.first() != Some(&tsn) {
                    return;
                }
                // The first endpoint the device lists, or endpoint 1 if it gives none.
                device.endpoint = zdo::parse_active_endpoints_response(&message.payload)
                    .ok()
                    .and_then(|r| r.endpoints.first().copied())
                    .unwrap_or(1);
                device.step = Step::SendRead;
                device.due = now;
            }
            (zcl::PROFILE_HOME_AUTOMATION, zcl::CLUSTER_BASIC) => {
                let Some(device) = self
                    .pairing
                    .devices
                    .iter_mut()
                    .find(|d| d.nwk == message.sender)
                else {
                    return;
                };
                let Step::WaitRead { tsn, .. } = device.step else {
                    return;
                };
                // A Default Response, or an answer with another transaction number, is not it.
                let Ok(response) = zcl::parse_read_attributes_response(&message.payload) else {
                    return;
                };
                if response.tsn != tsn {
                    return;
                }
                device.step = Step::Done;
                events.push(PairingEvent::Identified {
                    eui64: device.eui64,
                    manufacturer: response
                        .string(zcl::ATTRIBUTE_MANUFACTURER_NAME)
                        .map(str::to_string),
                    model: response
                        .string(zcl::ATTRIBUTE_MODEL_IDENTIFIER)
                        .map(str::to_string),
                });
            }
            _ => {}
        }
    }

    /// A device joined or announced itself: one `Joined` per device per session. An announce
    /// means the device has the network key: its identification starts now.
    fn register(
        &mut self,
        eui64: [u8; 8],
        nwk: u16,
        announced: bool,
        events: &mut Vec<PairingEvent>,
    ) {
        let now = (self.now)();
        if let Some(device) = self.pairing.devices.iter_mut().find(|d| d.eui64 == eui64) {
            device.nwk = nwk;
            if announced && device.step == Step::SendEndpoints {
                device.due = device.due.min(now);
            }
            return;
        }
        if self.pairing.devices.len() >= MAX_DEVICES {
            if !self.pairing.ignored.contains(&eui64) && self.pairing.ignored.len() < MAX_IGNORED {
                self.pairing.ignored.push(eui64);
                events.push(PairingEvent::TooManyDevices { eui64 });
            }
            return;
        }
        self.pairing.devices.push(Device {
            eui64,
            nwk,
            step: Step::SendEndpoints,
            due: if announced {
                now
            } else {
                now + ANNOUNCE_WAIT_MS
            },
            busy: 0,
            reads: 0,
            endpoint: 1,
        });
        events.push(PairingEvent::Joined { eui64, nwk });
    }

    fn next_tsn(&mut self) -> u8 {
        self.pairing.tsn = self.pairing.tsn.wrapping_add(1) & 0x7F;
        self.pairing.tsn
    }

    /// Sends one message to a device. `Ok(true)` when the radio took it, `Ok(false)` when it
    /// did not (busy or another refusal).
    fn send_to_device(
        &mut self,
        wide: bool,
        nwk: u16,
        aps: &ApsFrame,
        tsn: u8,
        message: &[u8],
    ) -> Result<bool, RadioError> {
        let params = ezsp::send_unicast(wide, nwk, aps, u16::from(tsn), message);
        let response = self.call(ezsp::ID_SEND_UNICAST, &params)?;
        let (status, _) = ezsp::parse_send_unicast(wide, &response)?;
        if status != ezsp::STATUS_OK && !ezsp::status_is_busy(status) {
            log::warn!("sendUnicast refused (status 0x{status:X}), counted as busy");
        }
        Ok(status == ezsp::STATUS_OK)
    }

    /// Does what each device is due to do: send a request, give up waiting, retry.
    fn drive_devices(
        &mut self,
        wide: bool,
        events: &mut Vec<PairingEvent>,
    ) -> Result<(), RadioError> {
        for index in 0..self.pairing.devices.len() {
            let now = (self.now)();
            let (step, due, nwk, eui64, endpoint) = {
                let d = &self.pairing.devices[index];
                (d.step, d.due, d.nwk, d.eui64, d.endpoint)
            };
            match step {
                Step::SendEndpoints | Step::SendRead if now >= due => {
                    let tsn = self.next_tsn();
                    let (aps, message) = if step == Step::SendEndpoints {
                        (
                            ApsFrame {
                                profile: zdo::PROFILE,
                                cluster: zdo::CLUSTER_ACTIVE_ENDPOINTS_REQUEST,
                                source_endpoint: 0,
                                destination_endpoint: 0,
                                options: ezsp::APS_OPTIONS_DEFAULT,
                                group_id: 0,
                                sequence: 0,
                            },
                            zdo::active_endpoints_request(tsn, nwk),
                        )
                    } else {
                        (
                            ApsFrame {
                                profile: zcl::PROFILE_HOME_AUTOMATION,
                                cluster: zcl::CLUSTER_BASIC,
                                source_endpoint: HOST_ENDPOINT,
                                destination_endpoint: endpoint,
                                options: ezsp::APS_OPTIONS_DEFAULT,
                                group_id: 0,
                                sequence: 0,
                            },
                            zcl::read_attributes_request(
                                tsn,
                                &[
                                    zcl::ATTRIBUTE_MANUFACTURER_NAME,
                                    zcl::ATTRIBUTE_MODEL_IDENTIFIER,
                                ],
                            ),
                        )
                    };
                    let taken = self.send_to_device(wide, nwk, &aps, tsn, &message)?;
                    let until = (self.now)()
                        + if step == Step::SendEndpoints {
                            ZDO_ANSWER_TIMEOUT_MS
                        } else {
                            ANSWER_TIMEOUT_MS
                        };
                    let device = &mut self.pairing.devices[index];
                    if taken {
                        device.busy = 0;
                        device.step = if step == Step::SendEndpoints {
                            Step::WaitEndpoints { tsn, until }
                        } else {
                            Step::WaitRead { tsn, until }
                        };
                    } else {
                        device.busy += 1;
                        if device.busy >= MAX_BUSY {
                            device.step = Step::Done;
                            events.push(PairingEvent::NotIdentified { eui64 });
                        } else {
                            device.due = now + BUSY_RETRY_MS;
                        }
                    }
                }
                Step::WaitEndpoints { until, .. } if now >= until => {
                    // No answer: endpoint 1.
                    let device = &mut self.pairing.devices[index];
                    device.endpoint = 1;
                    device.step = Step::SendRead;
                    device.due = now;
                }
                Step::WaitRead { until, .. } if now >= until => {
                    let device = &mut self.pairing.devices[index];
                    device.reads += 1;
                    if device.reads < MAX_READS {
                        device.step = Step::SendRead;
                        device.due = now;
                    } else {
                        device.step = Step::Done;
                        events.push(PairingEvent::Identified {
                            eui64,
                            manufacturer: None,
                            model: None,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}
