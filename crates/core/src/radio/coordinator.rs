//! The radio as a Zigbee coordinator: start or resume its network, open pairing, follow the
//! devices that join and identify them (manufacturer and model, from the Basic cluster).
//!
//! No clock and no random source of its own: the time comes from the `Radio`'s clock, the random
//! bytes from the caller.

use std::collections::VecDeque;
use std::fmt;

use super::measures::{self, Measure};
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
/// A request of the reporting setup is sent this many times in all (one retry) before the setup
/// gives up.
const MAX_ATTEMPTS: u8 = 2;
/// The Configure Reporting command of ZCL, to recognise the Default Response it gets.
const COMMAND_CONFIGURE_REPORTING: u8 = 0x06;
/// The Write Attributes command of ZCL, to recognise the Default Response it gets.
const COMMAND_WRITE_ATTRIBUTES: u8 = 0x02;
/// Pairs (sender, transaction number) remembered as already given a Default Response; the oldest
/// is dropped past this.
const MAX_ANSWERED: usize = 32;
/// Node addresses whose 64-bit address is remembered; the oldest is dropped past this.
const MAX_ADDRESSES: usize = 64;
/// Wait between the steps of an enrollment (before the Zone Enroll Response, between two reads
/// of the zone state), in milliseconds (Zigbee2MQTT's value).
const ENROLL_WAIT_MS: u64 = 500;
/// The zone state is read at most this many times after the Zone Enroll Response; the bound of
/// the enrollment's work per device.
const MAX_ZONE_POLLS: u8 = 20;
/// The host's endpoint, Home Automation profile.
pub(super) const HOST_ENDPOINT: u8 = 1;
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

/// The line the host prints for a device whose setup the time cap cut short.
pub fn cut_short_line(eui64: &[u8; 8]) -> String {
    format!("reporting: 0x{} cut short", hex(eui64))
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

/// How the enrollment of an IAS zone device ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentOutcome {
    /// The device reads as enrolled with this coordinator.
    Enrolled,
    /// The Zone Enroll Response was sent, but the device never read as enrolled.
    Unconfirmed,
    /// The device did not answer, or refused the coordinator's address.
    Failed,
}

/// The enrollment of an IAS zone device: its zone type (when known) and how it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enrollment {
    /// `None` when the device gave none; always `None` for a failed enrollment.
    pub zone_type: Option<u16>,
    pub outcome: EnrollmentOutcome,
}

/// The name of a zone type for the `reporting:` line; the number for one that is not known.
fn zone_type_name(zone_type: u16) -> String {
    match zone_type {
        0x000D => "motion".to_string(),
        0x0015 => "contact".to_string(),
        0x002A => "water leak".to_string(),
        0x0028 => "fire".to_string(),
        other => format!("zone 0x{other:04X}"),
    }
}

impl Enrollment {
    /// The text of the `reporting:` line: `enrolled (motion)`, `enrollment unconfirmed (motion)`,
    /// `enrollment failed`.
    fn text(&self) -> String {
        let zone = self
            .zone_type
            .map(|z| format!(" ({})", zone_type_name(z)))
            .unwrap_or_default();
        match self.outcome {
            EnrollmentOutcome::Enrolled => format!("enrolled{zone}"),
            EnrollmentOutcome::Unconfirmed => format!("enrollment unconfirmed{zone}"),
            EnrollmentOutcome::Failed => "enrollment failed".to_string(),
        }
    }
}

/// What happens while pairing is open.
#[derive(Debug, Clone, PartialEq)]
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
    /// The reporting of a device's standard clusters was set up (or tried): the clusters set
    /// up, the clusters that failed, whether the device never gave its descriptor (nothing
    /// was tried then), and the enrollment of an IAS zone device (`None` for any other).
    ReportingConfigured {
        eui64: [u8; 8],
        ok: Vec<u16>,
        failed: Vec<u16>,
        no_descriptor: bool,
        enrollment: Option<Enrollment>,
    },
    /// A device sent measures.
    Measures(MeasureEvent),
}

/// Who sent a message: its 64-bit address, or its network address when the radio does not know
/// the 64-bit one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sender {
    Eui64([u8; 8]),
    Nwk(u16),
}

impl fmt::Display for Sender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sender::Eui64(eui64) => write!(f, "0x{}", hex(eui64)),
            Sender::Nwk(nwk) => write!(f, "0x{nwk:04X}"),
        }
    }
}

/// What a device did, for the device registry (spec 008): the host takes it after each poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// The device announced itself: its capability byte (bit 2: mains powered).
    Announced { eui64: [u8; 8], capability: u8 },
    /// The device sent a message (of any kind).
    Message { sender: Sender },
}

/// Activities kept until the host takes them; the oldest is dropped past this.
const MAX_ACTIVITY: usize = 64;

/// The measures of one message from a device.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureEvent {
    pub sender: Sender,
    /// The endpoint of the device that sent them.
    pub endpoint: u8,
    /// At most `measures::MAX_MEASURES_PER_FRAME`.
    pub measures: Vec<Measure>,
}

impl MeasureEvent {
    /// The lines the host prints: `0x00124B0012345678 temperature 19.52 °C`, one per measure
    /// (`battery_low` is data of the device registry only, not shown).
    pub fn lines(&self) -> Vec<String> {
        self.measures
            .iter()
            .filter(|m| m.kind != "battery_low")
            .map(|m| format!("{} {m}", self.sender))
            .collect()
    }
}

impl PairingEvent {
    /// The line the host prints for this event (one line per measure for `Measures`, joined by
    /// newlines).
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
            PairingEvent::ReportingConfigured {
                eui64,
                ok,
                failed,
                no_descriptor,
                enrollment,
            } => {
                let names = |clusters: &[u16]| {
                    clusters
                        .iter()
                        .map(|c| measures::cluster_name(*c))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let mut line = format!("reporting: 0x{}", hex(eui64));
                if *no_descriptor {
                    line.push_str(" failed: no descriptor");
                } else if ok.is_empty() && failed.is_empty() {
                    line.push_str(" none");
                } else {
                    if !ok.is_empty() {
                        line.push(' ');
                        line.push_str(&names(ok));
                    }
                    if !failed.is_empty() {
                        line.push_str(" failed: ");
                        line.push_str(&names(failed));
                    }
                }
                if let (false, Some(enrollment)) = (*no_descriptor, enrollment) {
                    line.push_str(", ");
                    line.push_str(&enrollment.text());
                }
                line
            }
            PairingEvent::Measures(event) => event.lines().join("\n"),
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

/// Where the reporting setup of a device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    SendDescriptor,
    WaitDescriptor {
        tsn: u8,
        until: u64,
    },
    SendBind,
    WaitBind {
        tsn: u8,
        until: u64,
    },
    SendConfigure,
    WaitConfigure {
        tsn: u8,
        until: u64,
    },
    /// Enrollment of an IAS zone device (spec 009): read the zone state, zone type and CIE
    /// address.
    EnrollSendRead,
    EnrollWaitRead {
        tsn: u8,
        until: u64,
    },
    /// Write the CIE address.
    EnrollSendWrite,
    EnrollWaitWrite {
        tsn: u8,
        until: u64,
    },
    /// Send the Zone Enroll Response (not waited for).
    EnrollSendResponse,
    /// Read the zone state again.
    EnrollSendPoll,
    EnrollWaitPoll {
        tsn: u8,
        until: u64,
    },
    /// Nothing left to do: the result is reported.
    Finish,
}

impl Stage {
    fn is_enrollment(self) -> bool {
        matches!(
            self,
            Stage::EnrollSendRead
                | Stage::EnrollWaitRead { .. }
                | Stage::EnrollSendWrite
                | Stage::EnrollWaitWrite { .. }
                | Stage::EnrollSendResponse
                | Stage::EnrollSendPoll
                | Stage::EnrollWaitPoll { .. }
        )
    }
}

/// The reporting setup of one device: a descriptor request, the enrollment of an IAS zone
/// device, then for each cluster a bind and a Configure Reporting, one request at a time.
#[derive(Debug)]
struct Setup {
    stage: Stage,
    due: u64,
    /// Clusters still to do, the one in progress first; at most the table's size.
    todo: Vec<u16>,
    /// At most the table's size, like `todo`.
    ok: Vec<u16>,
    failed: Vec<u16>,
    /// Times the request in progress was sent.
    attempts: u8,
    /// Times the radio refused the request in progress.
    busy: u8,
    /// The device never gave its descriptor.
    no_descriptor: bool,
    /// This run tries again the clusters that failed in the first one.
    retry: bool,
    /// How the enrollment ended, once it did.
    enrollment: Option<Enrollment>,
    /// The zone type the device gave when read.
    zone_type: Option<u16>,
    /// The zone type of a Zone Enroll Request that came before the outcome.
    request_zone: Option<u16>,
    /// Reads of the zone state sent after the Zone Enroll Response; at most `MAX_ZONE_POLLS`.
    polls: u8,
}

impl Setup {
    fn new(now: u64) -> Self {
        Setup {
            stage: Stage::SendDescriptor,
            due: now,
            todo: Vec::new(),
            ok: Vec::new(),
            failed: Vec::new(),
            attempts: 0,
            busy: 0,
            no_descriptor: false,
            retry: false,
            enrollment: None,
            zone_type: None,
            request_zone: None,
            polls: 0,
        }
    }

    /// A second try at what failed: the enrollment if `enroll`, then `clusters`.
    fn retrying(clusters: Vec<u16>, enroll: bool, now: u64) -> Self {
        Setup {
            stage: if enroll {
                Stage::EnrollSendRead
            } else {
                Stage::SendBind
            },
            todo: clusters,
            retry: true,
            ..Setup::new(now)
        }
    }

    /// The enrollment is over: on to the binds, or to the end if no cluster is left.
    fn enrollment_done(&mut self, now: u64, outcome: EnrollmentOutcome) {
        let zone_type = match outcome {
            EnrollmentOutcome::Failed => None,
            _ => self.zone_type.or(self.request_zone),
        };
        self.enrollment = Some(Enrollment { zone_type, outcome });
        self.attempts = 0;
        self.busy = 0;
        self.due = now;
        self.stage = if self.todo.is_empty() {
            Stage::Finish
        } else {
            Stage::SendBind
        };
    }

    /// The cluster in progress is done, well or not: on to the next.
    fn cluster_done(&mut self, now: u64, succeeded: bool) {
        if !self.todo.is_empty() {
            let cluster = self.todo.remove(0);
            if succeeded {
                self.ok.push(cluster);
            } else {
                self.failed.push(cluster);
            }
        }
        self.attempts = 0;
        self.busy = 0;
        self.due = now;
        self.stage = if self.todo.is_empty() {
            Stage::Finish
        } else {
            Stage::SendBind
        };
    }

    /// The device does not answer: the clusters not done yet are failed.
    fn give_up(&mut self) {
        self.failed.append(&mut self.todo);
        self.stage = Stage::Finish;
    }
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
    setup: Option<Setup>,
    /// Clusters that failed, to try once more at the device's next message; at most the table's
    /// size.
    retry: Vec<u16>,
    /// The enrollment failed or was unconfirmed: tried once more at the next message too.
    retry_enroll: bool,
}

/// What the coordinator follows while pairing, and what it remembers of the network.
#[derive(Debug, Default)]
pub struct Pairing {
    /// At most `MAX_DEVICES`.
    devices: Vec<Device>,
    /// At most `MAX_IGNORED`.
    ignored: Vec<[u8; 8]>,
    tsn: u8,
    /// The radio's own address, read when it was identified.
    own_eui64: Option<[u8; 8]>,
    /// (sender, transaction number) given a Default Response; at most `MAX_ANSWERED`.
    answered: VecDeque<(u16, u8)>,
    /// Node address to 64-bit address; at most `MAX_ADDRESSES`, the oldest dropped.
    addresses: VecDeque<(u16, [u8; 8])>,
    /// The radio only listens: devices are not followed.
    listening: bool,
    /// What devices did since the host last took it; at most `MAX_ACTIVITY`.
    activity: VecDeque<Activity>,
}

impl Pairing {
    pub(super) fn set_own_eui64(&mut self, eui64: [u8; 8]) {
        self.own_eui64 = Some(eui64);
    }

    /// The node address remembered for this device: the cache, or a device being followed.
    pub(super) fn nwk_of(&self, eui64: &[u8; 8]) -> Option<u16> {
        self.addresses
            .iter()
            .find(|(_, e)| e == eui64)
            .map(|(n, _)| *n)
            .or_else(|| {
                self.devices
                    .iter()
                    .find(|d| d.eui64 == *eui64)
                    .map(|d| d.nwk)
            })
    }

    fn push_activity(&mut self, activity: Activity) {
        if self.activity.len() >= MAX_ACTIVITY {
            self.activity.pop_front();
        }
        self.activity.push_back(activity);
    }

    pub(super) fn remember_address(&mut self, nwk: u16, eui64: [u8; 8]) {
        self.forget_address(nwk, eui64);
        if self.addresses.len() >= MAX_ADDRESSES {
            self.addresses.pop_front();
        }
        self.addresses.push_back((nwk, eui64));
    }

    /// Drops what is remembered of this node address and of this device: one of the two changed.
    fn forget_address(&mut self, nwk: u16, eui64: [u8; 8]) {
        self.addresses.retain(|(n, e)| *n != nwk && *e != eui64);
    }

    /// True the first time a Default Response is due for this transaction of this sender.
    fn first_default_response(&mut self, nwk: u16, tsn: u8) -> bool {
        if self.answered.contains(&(nwk, tsn)) {
            return false;
        }
        if self.answered.len() >= MAX_ANSWERED {
            self.answered.pop_front();
        }
        self.answered.push_back((nwk, tsn));
        true
    }
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
    pub(super) fn wide(&self) -> Result<bool, RadioError> {
        self.ezsp
            .protocol_version()
            .map(ezsp::is_wide)
            .ok_or(RadioError::NotIdentified)
    }

    pub(super) fn call(&mut self, id: u16, params: &[u8]) -> Result<Vec<u8>, RadioError> {
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

        self.running_network(wide, keys)
    }

    /// Resumes the network the radio has, without ever forming one, and returns it. A radio with
    /// no network is `RadioError::NoNetwork`. Joining stays closed; the devices are not
    /// followed, only their measures are read (`poll`).
    pub fn listen(&mut self) -> Result<Network, RadioError> {
        let wide = self.wide()?;
        self.configure(wide)?;
        match self.call_status(ezsp::ID_NETWORK_INIT, &ezsp::network_init(), wide)? {
            ezsp::STATUS_OK => self.wait_for_network_up()?,
            ezsp::STATUS_NOT_JOINED => return Err(RadioError::NoNetwork),
            status => {
                return Err(RadioError::Refused {
                    command: "networkInit",
                    status,
                });
            }
        }
        self.pairing.listening = true;
        self.running_network(wide, None)
    }

    /// The network once it is up: its parameters, checked, and the concentrator set.
    fn running_network(
        &mut self,
        wide: bool,
        keys: Option<NetworkKeys>,
    ) -> Result<Network, RadioError> {
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

    /// Forgets the radio's own address (tests: it is asked again when needed).
    #[doc(hidden)]
    pub fn forget_own_eui64(&mut self) {
        self.pairing.own_eui64 = None;
    }

    /// The devices whose reporting setup or enrollment is still going on.
    pub fn setups_in_progress(&self) -> Vec<[u8; 8]> {
        self.pairing
            .devices
            .iter()
            .filter(|d| d.setup.is_some())
            .map(|d| d.eui64)
            .collect()
    }

    /// True while a device that joined is still being identified.
    pub fn pairing_busy(&self) -> bool {
        self.pairing
            .devices
            .iter()
            .any(|d| d.step != Step::Done || d.setup.is_some())
    }

    /// Reads the radio for a moment, follows the devices and returns what happened.
    pub fn poll_pairing(&mut self) -> Result<Vec<PairingEvent>, RadioError> {
        let wide = self.wide()?;
        self.step()?;
        self.drain_payloads();
        let mut events = Vec::new();
        while let Some(callback) = self.callbacks.pop_front() {
            self.on_callback(wide, callback, &mut events)?;
        }
        self.drive_devices(wide, &mut events)?;
        self.drive_commands(wide)?;
        Ok(events)
    }

    /// What devices did since the last call: announces and messages, oldest first.
    pub fn take_activity(&mut self) -> Vec<Activity> {
        self.pairing.activity.drain(..).collect()
    }

    /// Reads the radio for a moment and returns the measures that arrived (after `listen`).
    pub fn poll(&mut self) -> Result<Vec<MeasureEvent>, RadioError> {
        Ok(self
            .poll_pairing()?
            .into_iter()
            .filter_map(|event| match event {
                PairingEvent::Measures(measures) => Some(measures),
                _ => None,
            })
            .collect())
    }

    fn on_callback(
        &mut self,
        wide: bool,
        callback: Callback,
        events: &mut Vec<PairingEvent>,
    ) -> Result<(), RadioError> {
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
                    self.on_incoming(wide, message, events)?;
                }
            }
            // What became of a message is not needed: the answer, or its absence, tells.
            ezsp::ID_MESSAGE_SENT_HANDLER => {}
            _ => {}
        }
        Ok(())
    }

    fn on_trust_center_join(&mut self, join: TrustCenterJoin, events: &mut Vec<PairingEvent>) {
        let joined = matches!(
            join.status,
            ezsp::DEVICE_UPDATE_UNSECURED_JOIN | ezsp::DEVICE_UPDATE_UNSECURED_REJOIN
        ) && join.decision != ezsp::JOIN_DECISION_DENY;
        if joined {
            self.commands.note_route(join.eui64, join.node_id, None);
            self.register(join.eui64, join.node_id, false, events);
        }
    }

    fn on_incoming(
        &mut self,
        wide: bool,
        message: IncomingMessage,
        events: &mut Vec<PairingEvent>,
    ) -> Result<(), RadioError> {
        let now = (self.now)();
        self.wake_for_retry(message.sender, now);
        match (message.aps.profile, message.aps.cluster) {
            (zdo::PROFILE, zdo::CLUSTER_DEVICE_ANNOUNCE) => {
                if let Ok(announce) = zdo::parse_device_announce(&message.payload) {
                    if let Some(capability) = announce.capability {
                        self.pairing.push_activity(Activity::Announced {
                            eui64: announce.eui64,
                            capability,
                        });
                    }
                    self.commands.note_route(announce.eui64, announce.nwk, None);
                    self.register(announce.eui64, announce.nwk, true, events);
                }
            }
            (zdo::PROFILE, zdo::CLUSTER_ACTIVE_ENDPOINTS_RESPONSE) => {
                self.on_active_endpoints(&message, now);
            }
            (zdo::PROFILE, zdo::CLUSTER_SIMPLE_DESCRIPTOR_RESPONSE) => {
                self.on_descriptor(&message, now);
                self.report_finished_at(message.sender, events);
            }
            (zdo::PROFILE, zdo::CLUSTER_BIND_RESPONSE) => {
                self.on_bind(&message, now);
                self.report_finished_at(message.sender, events);
            }
            (zcl::PROFILE_HOME_AUTOMATION, _) => self.on_zcl(wide, &message, now, events)?,
            _ => {}
        }
        Ok(())
    }

    fn device_at(&mut self, nwk: u16) -> Option<&mut Device> {
        self.pairing.devices.iter_mut().find(|d| d.nwk == nwk)
    }

    fn on_active_endpoints(&mut self, message: &IncomingMessage, now: u64) {
        let Some(device) = self.device_at(message.sender) else {
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

    /// A device that failed some clusters sends a message: it is awake, they are tried once more.
    fn wake_for_retry(&mut self, nwk: u16, now: u64) {
        let Some(device) = self.device_at(nwk) else {
            return;
        };
        if device.step == Step::Done
            && device.setup.is_none()
            && (!device.retry.is_empty() || device.retry_enroll)
        {
            let clusters = std::mem::take(&mut device.retry);
            let enroll = std::mem::take(&mut device.retry_enroll);
            device.setup = Some(Setup::retrying(clusters, enroll, now));
        }
    }

    /// The `Simple_Desc_rsp` of the setup: the clusters to set up, or "no descriptor".
    fn on_descriptor(&mut self, message: &IncomingMessage, now: u64) {
        let Some(setup) = self
            .device_at(message.sender)
            .and_then(|d| d.setup.as_mut())
        else {
            return;
        };
        let Stage::WaitDescriptor { tsn, .. } = setup.stage else {
            return;
        };
        if message.payload.first() != Some(&tsn) {
            return;
        }
        match zdo::parse_simple_descriptor_response(&message.payload) {
            Ok(descriptor) => {
                setup.todo = measures::clusters_to_set_up(&descriptor.input_clusters);
                setup.attempts = 0;
                setup.busy = 0;
                setup.due = now;
                setup.stage = if descriptor.input_clusters.contains(&zcl::CLUSTER_IAS_ZONE) {
                    Stage::EnrollSendRead
                } else if setup.todo.is_empty() {
                    Stage::Finish
                } else {
                    Stage::SendBind
                };
            }
            Err(_) => {
                setup.no_descriptor = true;
                setup.stage = Stage::Finish;
            }
        }
    }

    /// The `Bind_rsp` of the setup: on to the Configure Reporting, or the cluster failed.
    fn on_bind(&mut self, message: &IncomingMessage, now: u64) {
        let Some(setup) = self
            .device_at(message.sender)
            .and_then(|d| d.setup.as_mut())
        else {
            return;
        };
        let Stage::WaitBind { tsn, .. } = setup.stage else {
            return;
        };
        if message.payload.first() != Some(&tsn) {
            return;
        }
        if zdo::parse_bind_response(&message.payload).is_ok() {
            setup.attempts = 0;
            setup.busy = 0;
            setup.due = now;
            setup.stage = Stage::SendConfigure;
        } else {
            setup.cluster_done(now, false);
        }
    }

    /// A ZCL frame: the Basic answer of the identification, the answer to a Configure Reporting,
    /// measures, and the Default Response the frame may ask for.
    fn on_zcl(
        &mut self,
        wide: bool,
        message: &IncomingMessage,
        now: u64,
        events: &mut Vec<PairingEvent>,
    ) -> Result<(), RadioError> {
        let cluster = message.aps.cluster;
        let frame = &message.payload;
        self.on_command_answer(message);
        if cluster == zcl::CLUSTER_BASIC {
            self.on_basic_answer(message, now, events);
        }
        self.on_configure_answer(message, now);
        if cluster == zcl::CLUSTER_IAS_ZONE {
            self.on_enroll_request(wide, message)?;
            self.on_enroll_answer(message, now)?;
        }
        self.report_finished_at(message.sender, events);

        let measured = if cluster == zcl::CLUSTER_IAS_ZONE {
            match zcl::parse_zone_status_change(frame) {
                Ok(status) => measures::from_zone_status(status),
                Err(_) => zcl::parse_attribute_report(frame)
                    .map(|report| measures::from_records(cluster, &report.records))
                    .unwrap_or_default(),
            }
        } else {
            zcl::parse_attribute_report(frame)
                .map(|report| measures::from_records(cluster, &report.records))
                .unwrap_or_default()
        };
        // A frame without measures never costs a lookup (and never a failure that would skip
        // the Default Response): its activity is kept only if the sender is already known.
        if measured.is_empty() {
            if let Some(sender) = self.known_sender(message) {
                self.pairing.push_activity(Activity::Message { sender });
                if let Sender::Eui64(eui64) = sender {
                    self.note_route_of(message, eui64);
                }
            }
        } else {
            // The Default Response is sent whatever the lookup gives.
            let sender = self.sender_address(wide, message);
            let answered = self.answer_with_default_response(wide, message);
            let sender = sender?;
            self.pairing.push_activity(Activity::Message { sender });
            if let Sender::Eui64(eui64) = sender {
                self.note_route_of(message, eui64);
            }
            events.push(PairingEvent::Measures(MeasureEvent {
                sender,
                endpoint: message.aps.source_endpoint,
                measures: measured,
            }));
            return answered;
        }
        self.answer_with_default_response(wide, message)
    }

    /// The Read Attributes Response of the identification.
    fn on_basic_answer(
        &mut self,
        message: &IncomingMessage,
        now: u64,
        events: &mut Vec<PairingEvent>,
    ) {
        let Some(device) = self.device_at(message.sender) else {
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
        // The device is awake: its reporting is set up right now.
        device.setup = Some(Setup::new(now));
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

    /// The answer to the Configure Reporting in progress: a Configure Reporting Response or a
    /// Default Response, matched by sender, cluster, command and transaction number.
    fn on_configure_answer(&mut self, message: &IncomingMessage, now: u64) {
        let cluster = message.aps.cluster;
        let Some(setup) = self
            .device_at(message.sender)
            .and_then(|d| d.setup.as_mut())
        else {
            return;
        };
        let Stage::WaitConfigure { tsn, .. } = setup.stage else {
            return;
        };
        if setup.todo.first() != Some(&cluster) {
            return;
        }
        let frame = &message.payload;
        let succeeded = if let Ok(response) = zcl::parse_configure_reporting_response(frame) {
            if response.tsn != tsn {
                return;
            }
            // Set up when the device accepted at least one attribute of the cluster.
            let wanted = measures::reporting_cluster(cluster).map_or(0, |c| c.attributes.len());
            response.failed.is_none() && response.refused.len() < wanted
        } else if let Ok(response) = zcl::parse_default_response(frame) {
            if response.tsn != tsn || response.command != COMMAND_CONFIGURE_REPORTING {
                return;
            }
            response.status == 0
        } else {
            return;
        };
        setup.cluster_done(now, succeeded);
    }

    /// A Zone Enroll Request: answered with a Zone Enroll Response carrying its transaction
    /// number, whatever the mode. Its zone type is kept if the enrollment has no outcome yet.
    fn on_enroll_request(
        &mut self,
        wide: bool,
        message: &IncomingMessage,
    ) -> Result<(), RadioError> {
        let Ok(request) = zcl::parse_zone_enroll_request(&message.payload) else {
            return Ok(());
        };
        if let Some(setup) = self
            .device_at(message.sender)
            .and_then(|d| d.setup.as_mut())
            && setup.enrollment.is_none()
        {
            setup.request_zone.get_or_insert(request.zone_type);
        }
        if !message.is_unicast() {
            return Ok(());
        }
        let aps = ApsFrame {
            profile: zcl::PROFILE_HOME_AUTOMATION,
            cluster: zcl::CLUSTER_IAS_ZONE,
            source_endpoint: message.aps.destination_endpoint,
            destination_endpoint: message.aps.source_endpoint,
            options: ezsp::APS_OPTIONS_DEFAULT,
            group_id: 0,
            sequence: 0,
        };
        // Nobody waits for it: the device asks again if it needs to.
        self.send_to_device(
            wide,
            message.sender,
            &aps,
            request.tsn,
            &zcl::zone_enroll_response(request.tsn),
        )?;
        Ok(())
    }

    /// The answer to the enrollment step in progress: the read of the zone state (first or
    /// polled) or the write of the CIE address.
    fn on_enroll_answer(&mut self, message: &IncomingMessage, now: u64) -> Result<(), RadioError> {
        let Some(stage) = self
            .device_at(message.sender)
            .and_then(|d| d.setup.as_ref())
            .map(|s| s.stage)
        else {
            return Ok(());
        };
        let frame = &message.payload;
        match stage {
            Stage::EnrollWaitRead { tsn, .. } | Stage::EnrollWaitPoll { tsn, .. } => {
                let Ok(report) = zcl::parse_attribute_report(frame) else {
                    return Ok(());
                };
                if report.tsn != tsn {
                    return Ok(());
                }
                let record = |attribute: u16| {
                    report
                        .records
                        .iter()
                        .find(|r| r.attribute == attribute)
                        .map(|r| r.value.as_slice())
                };
                let state = record(zcl::ATTRIBUTE_ZONE_STATE).and_then(|v| v.first().copied());
                let zone_type = record(zcl::ATTRIBUTE_ZONE_TYPE)
                    .and_then(|v| <[u8; 2]>::try_from(v).ok())
                    .map(u16::from_le_bytes);
                let cie = record(zcl::ATTRIBUTE_CIE_ADDRESS).and_then(|v| {
                    let mut eui64 = <[u8; 8]>::try_from(v).ok()?;
                    eui64.reverse();
                    Some(eui64)
                });
                let first = matches!(stage, Stage::EnrollWaitRead { .. });
                // Only the first read tells the address: the polls ask for the state only.
                let enrolled_with_us = if first && state == Some(1) {
                    // An address nobody can tell is "not enrolled with us": on to the write.
                    let own = self.own_eui64().ok();
                    cie.is_some() && cie == own
                } else {
                    false
                };
                let Some(setup) = self
                    .device_at(message.sender)
                    .and_then(|d| d.setup.as_mut())
                else {
                    return Ok(());
                };
                if first {
                    setup.zone_type = zone_type;
                    if enrolled_with_us {
                        setup.enrollment_done(now, EnrollmentOutcome::Enrolled);
                    } else {
                        setup.attempts = 0;
                        setup.busy = 0;
                        setup.due = now;
                        setup.stage = Stage::EnrollSendWrite;
                    }
                } else if state == Some(1) {
                    setup.enrollment_done(now, EnrollmentOutcome::Enrolled);
                } else {
                    setup.polls += 1;
                    if setup.polls >= MAX_ZONE_POLLS {
                        setup.enrollment_done(now, EnrollmentOutcome::Unconfirmed);
                    } else {
                        setup.due = now + ENROLL_WAIT_MS;
                        setup.stage = Stage::EnrollSendPoll;
                    }
                }
            }
            Stage::EnrollWaitWrite { tsn, .. } => {
                let written = if let Ok(response) = zcl::parse_write_attributes_response(frame) {
                    (response.tsn == tsn).then_some(response.written)
                } else if let Ok(response) = zcl::parse_default_response(frame) {
                    (response.tsn == tsn && response.command == COMMAND_WRITE_ATTRIBUTES)
                        .then_some(response.status == 0)
                } else {
                    None
                };
                let Some(written) = written else {
                    return Ok(());
                };
                let Some(setup) = self
                    .device_at(message.sender)
                    .and_then(|d| d.setup.as_mut())
                else {
                    return Ok(());
                };
                if written {
                    setup.attempts = 0;
                    setup.busy = 0;
                    setup.due = now + ENROLL_WAIT_MS;
                    setup.stage = Stage::EnrollSendResponse;
                } else {
                    setup.enrollment_done(now, EnrollmentOutcome::Failed);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The 64-bit address of the sender of a message: the device followed in this session, what
    /// is remembered, what the packet carried, then what the radio's address table says; its node
    /// address if nobody knows.
    /// The sender's 64-bit address if it is known without asking the radio: a device being
    /// followed, the cache, or the packet itself.
    fn known_sender(&mut self, message: &IncomingMessage) -> Option<Sender> {
        let nwk = message.sender;
        if let Some(device) = self.device_at(nwk) {
            return Some(Sender::Eui64(device.eui64));
        }
        if let Some((_, eui64)) = self.pairing.addresses.iter().find(|(n, _)| *n == nwk) {
            return Some(Sender::Eui64(*eui64));
        }
        message.sender_eui64.map(Sender::Eui64)
    }

    fn sender_address(
        &mut self,
        wide: bool,
        message: &IncomingMessage,
    ) -> Result<Sender, RadioError> {
        let nwk = message.sender;
        if let Some(device) = self.device_at(nwk) {
            return Ok(Sender::Eui64(device.eui64));
        }
        if let Some((_, eui64)) = self.pairing.addresses.iter().find(|(n, _)| *n == nwk) {
            return Ok(Sender::Eui64(*eui64));
        }
        let known = match message.sender_eui64 {
            Some(eui64) => Some(eui64),
            None => {
                let response = self.call(
                    ezsp::ID_LOOKUP_EUI64_BY_NODE_ID,
                    &ezsp::lookup_eui64_by_node_id(nwk),
                )?;
                ezsp::parse_lookup_eui64(wide, &response).unwrap_or(None)
            }
        };
        Ok(match known {
            Some(eui64) => {
                self.pairing.remember_address(nwk, eui64);
                Sender::Eui64(eui64)
            }
            None => Sender::Nwk(nwk),
        })
    }

    /// Sends the Default Response a unicast frame asks for, once per transaction and sender. It
    /// is sent once and not waited for: it takes no request slot of the setup.
    fn answer_with_default_response(
        &mut self,
        wide: bool,
        message: &IncomingMessage,
    ) -> Result<(), RadioError> {
        if !message.is_unicast() {
            return Ok(());
        }
        let Some(response) = zcl::default_response(message.aps.cluster, &message.payload) else {
            return Ok(());
        };
        let Ok((header, _)) = zcl::parse_header(&message.payload) else {
            return Ok(());
        };
        if !self
            .pairing
            .first_default_response(message.sender, header.tsn)
        {
            return Ok(());
        }
        // From the endpoint the frame was addressed to, to the endpoint that sent it.
        let aps = ApsFrame {
            profile: zcl::PROFILE_HOME_AUTOMATION,
            cluster: message.aps.cluster,
            source_endpoint: message.aps.destination_endpoint,
            destination_endpoint: message.aps.source_endpoint,
            options: ezsp::APS_OPTIONS_DEFAULT,
            group_id: 0,
            sequence: 0,
        };
        // Refused or lost: nobody waits for it, the device asks again if it needs it.
        self.send_to_device(wide, message.sender, &aps, header.tsn, &response)?;
        Ok(())
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
        if self.pairing.listening {
            return;
        }
        let now = (self.now)();
        self.pairing.forget_address(nwk, eui64);
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
            setup: None,
            retry: Vec::new(),
            retry_enroll: false,
        });
        events.push(PairingEvent::Joined { eui64, nwk });
    }

    pub(super) fn next_tsn(&mut self) -> u8 {
        self.pairing.tsn = self.pairing.tsn.wrapping_add(1) & 0x7F;
        self.pairing.tsn
    }

    /// Sends one message to a device. `Ok(true)` when the radio took it, `Ok(false)` when it
    /// did not (busy or another refusal).
    pub(super) fn send_to_device(
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
            self.drive_setup(wide, index, events)?;
        }
        Ok(())
    }

    fn report_finished_at(&mut self, nwk: u16, events: &mut Vec<PairingEvent>) {
        if let Some(index) = self.pairing.devices.iter().position(|d| d.nwk == nwk) {
            self.report_finished(index, events);
        }
    }

    /// Reports the result of a setup that is at its end, and arms the retry of what failed.
    fn report_finished(&mut self, index: usize, events: &mut Vec<PairingEvent>) {
        let device = &mut self.pairing.devices[index];
        if device.setup.as_ref().map(|s| s.stage) != Some(Stage::Finish) {
            return;
        }
        let Some(setup) = device.setup.take() else {
            return;
        };
        if !setup.retry && !setup.no_descriptor {
            device.retry = setup.failed.clone();
            device.retry_enroll = setup.enrollment.is_some_and(|e| {
                matches!(
                    e.outcome,
                    EnrollmentOutcome::Failed | EnrollmentOutcome::Unconfirmed
                )
            });
        }
        events.push(PairingEvent::ReportingConfigured {
            eui64: device.eui64,
            ok: setup.ok,
            failed: setup.failed,
            no_descriptor: setup.no_descriptor,
            enrollment: setup.enrollment,
        });
    }

    /// The radio's own address: read when it was identified, asked again if it was not.
    fn own_eui64(&mut self) -> Result<[u8; 8], RadioError> {
        if let Some(eui64) = self.pairing.own_eui64 {
            return Ok(eui64);
        }
        let response = self.call(ezsp::ID_GET_EUI64, &[])?;
        let eui64 = ezsp::parse_eui64(&response)?;
        self.pairing.set_own_eui64(eui64);
        Ok(eui64)
    }

    /// Does what the enrollment of an IAS zone device is due to do: send its next request, send
    /// it again, give up (the enrollment failed, or stays unconfirmed once the response is
    /// sent).
    fn drive_enrollment(&mut self, wide: bool, index: usize) -> Result<(), RadioError> {
        let now = (self.now)();
        let device = &mut self.pairing.devices[index];
        let (nwk, endpoint) = (device.nwk, device.endpoint);
        let Some(setup) = device.setup.as_mut() else {
            return Ok(());
        };
        let stage = setup.stage;
        match stage {
            Stage::EnrollWaitRead { until, .. } | Stage::EnrollWaitWrite { until, .. }
                if now >= until =>
            {
                // No answer: the same request again, then the end of the enrollment.
                setup.attempts += 1;
                if setup.attempts < MAX_ATTEMPTS {
                    setup.due = now;
                    setup.stage = if matches!(stage, Stage::EnrollWaitRead { .. }) {
                        Stage::EnrollSendRead
                    } else {
                        Stage::EnrollSendWrite
                    };
                } else {
                    setup.enrollment_done(now, EnrollmentOutcome::Failed);
                }
            }
            // A poll without an answer ends the polling at once.
            Stage::EnrollWaitPoll { until, .. } if now >= until => {
                setup.enrollment_done(now, EnrollmentOutcome::Unconfirmed);
            }
            Stage::EnrollSendRead
            | Stage::EnrollSendWrite
            | Stage::EnrollSendResponse
            | Stage::EnrollSendPoll
                if now >= setup.due =>
            {
                let tsn = self.next_tsn();
                let message = match stage {
                    Stage::EnrollSendRead => zcl::read_attributes_request(
                        tsn,
                        &[
                            zcl::ATTRIBUTE_ZONE_STATE,
                            zcl::ATTRIBUTE_ZONE_TYPE,
                            zcl::ATTRIBUTE_CIE_ADDRESS,
                        ],
                    ),
                    Stage::EnrollSendWrite => match self.own_eui64() {
                        Ok(own) => zcl::write_cie_address_request(tsn, &own),
                        Err(_) => {
                            // No address to write: the enrollment fails, the run goes on.
                            if let Some(setup) = self.pairing.devices[index].setup.as_mut() {
                                setup.enrollment_done(now, EnrollmentOutcome::Failed);
                            }
                            return Ok(());
                        }
                    },
                    Stage::EnrollSendResponse => zcl::zone_enroll_response(tsn),
                    _ => zcl::read_attributes_request(tsn, &[zcl::ATTRIBUTE_ZONE_STATE]),
                };
                let aps = ApsFrame {
                    profile: zcl::PROFILE_HOME_AUTOMATION,
                    cluster: zcl::CLUSTER_IAS_ZONE,
                    source_endpoint: HOST_ENDPOINT,
                    destination_endpoint: endpoint,
                    options: ezsp::APS_OPTIONS_DEFAULT,
                    group_id: 0,
                    sequence: 0,
                };
                let taken = self.send_to_device(wide, nwk, &aps, tsn, &message)?;
                let until = (self.now)() + ANSWER_TIMEOUT_MS;
                let Some(setup) = self.pairing.devices[index].setup.as_mut() else {
                    return Ok(());
                };
                if taken {
                    setup.busy = 0;
                    setup.stage = match stage {
                        Stage::EnrollSendRead => Stage::EnrollWaitRead { tsn, until },
                        Stage::EnrollSendWrite => Stage::EnrollWaitWrite { tsn, until },
                        Stage::EnrollSendResponse => {
                            setup.polls = 0;
                            setup.due = now + ENROLL_WAIT_MS;
                            Stage::EnrollSendPoll
                        }
                        _ => Stage::EnrollWaitPoll { tsn, until },
                    };
                } else {
                    setup.busy += 1;
                    if setup.busy < MAX_BUSY {
                        setup.due = now + BUSY_RETRY_MS;
                    } else if matches!(stage, Stage::EnrollSendResponse) {
                        // The response is lost: the state is read all the same.
                        setup.busy = 0;
                        setup.polls = 0;
                        setup.due = now + ENROLL_WAIT_MS;
                        setup.stage = Stage::EnrollSendPoll;
                    } else if matches!(stage, Stage::EnrollSendPoll) {
                        setup.enrollment_done(now, EnrollmentOutcome::Unconfirmed);
                    } else {
                        setup.enrollment_done(now, EnrollmentOutcome::Failed);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Does what the reporting setup of one device is due to do: send its next request, send
    /// it again, give up, or report the result.
    fn drive_setup(
        &mut self,
        wide: bool,
        index: usize,
        events: &mut Vec<PairingEvent>,
    ) -> Result<(), RadioError> {
        let now = (self.now)();
        let device = &mut self.pairing.devices[index];
        let (nwk, eui64, endpoint) = (device.nwk, device.eui64, device.endpoint);
        let Some(setup) = device.setup.as_mut() else {
            return Ok(());
        };
        if setup.stage.is_enrollment() {
            return self.drive_enrollment(wide, index);
        }
        let cluster = setup.todo.first().copied();
        match setup.stage {
            Stage::Finish => self.report_finished(index, events),
            Stage::WaitDescriptor { until, .. }
            | Stage::WaitBind { until, .. }
            | Stage::WaitConfigure { until, .. }
                if now >= until =>
            {
                // No answer: the same request again, then the end of the setup.
                setup.attempts += 1;
                if setup.attempts < MAX_ATTEMPTS {
                    setup.due = now;
                    setup.stage = match setup.stage {
                        Stage::WaitDescriptor { .. } => Stage::SendDescriptor,
                        Stage::WaitBind { .. } => Stage::SendBind,
                        _ => Stage::SendConfigure,
                    };
                } else {
                    setup.no_descriptor = matches!(setup.stage, Stage::WaitDescriptor { .. });
                    setup.give_up();
                }
            }
            stage @ (Stage::SendDescriptor | Stage::SendBind | Stage::SendConfigure)
                if now >= setup.due =>
            {
                let tsn = self.next_tsn();
                let zdo_aps = |cluster: u16| ApsFrame {
                    profile: zdo::PROFILE,
                    cluster,
                    source_endpoint: 0,
                    destination_endpoint: 0,
                    options: ezsp::APS_OPTIONS_DEFAULT,
                    group_id: 0,
                    sequence: 0,
                };
                let (aps, message, timeout) = match (stage, cluster) {
                    (Stage::SendDescriptor, _) => (
                        zdo_aps(zdo::CLUSTER_SIMPLE_DESCRIPTOR_REQUEST),
                        zdo::simple_descriptor_request(tsn, nwk, endpoint),
                        ZDO_ANSWER_TIMEOUT_MS,
                    ),
                    (Stage::SendBind, Some(cluster)) => {
                        let Ok(own) = self.own_eui64() else {
                            // No address to bind to: the cluster fails, the run goes on.
                            if let Some(setup) = self.pairing.devices[index].setup.as_mut() {
                                setup.cluster_done(now, false);
                            }
                            return Ok(());
                        };
                        (
                            zdo_aps(zdo::CLUSTER_BIND_REQUEST),
                            zdo::bind_request(tsn, eui64, endpoint, cluster, own, HOST_ENDPOINT),
                            ZDO_ANSWER_TIMEOUT_MS,
                        )
                    }
                    (Stage::SendConfigure, Some(cluster)) => {
                        let attributes =
                            measures::reporting_cluster(cluster).map_or(&[][..], |c| c.attributes);
                        (
                            ApsFrame {
                                profile: zcl::PROFILE_HOME_AUTOMATION,
                                cluster,
                                source_endpoint: HOST_ENDPOINT,
                                destination_endpoint: endpoint,
                                options: ezsp::APS_OPTIONS_DEFAULT,
                                group_id: 0,
                                sequence: 0,
                            },
                            zcl::configure_reporting_request(tsn, attributes),
                            ANSWER_TIMEOUT_MS,
                        )
                    }
                    // A cluster step without a cluster: nothing to send, report the result.
                    _ => {
                        if let Some(setup) = self.pairing.devices[index].setup.as_mut() {
                            setup.stage = Stage::Finish;
                        }
                        return Ok(());
                    }
                };
                let taken = self.send_to_device(wide, nwk, &aps, tsn, &message)?;
                let until = (self.now)() + timeout;
                let Some(setup) = self.pairing.devices[index].setup.as_mut() else {
                    return Ok(());
                };
                if taken {
                    setup.busy = 0;
                    setup.stage = match stage {
                        Stage::SendDescriptor => Stage::WaitDescriptor { tsn, until },
                        Stage::SendBind => Stage::WaitBind { tsn, until },
                        _ => Stage::WaitConfigure { tsn, until },
                    };
                } else {
                    setup.busy += 1;
                    if setup.busy >= MAX_BUSY {
                        setup.no_descriptor = stage == Stage::SendDescriptor;
                        setup.give_up();
                    } else {
                        setup.due = now + BUSY_RETRY_MS;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
