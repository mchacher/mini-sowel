//! Commands to devices (spec 011): an On/Off, Level Control or Color Control command sent to a
//! device, its Default Response awaited without blocking the reception of other frames, one retry,
//! and the state read back once the device took it. The routes (network address and endpoint) the messages tell are
//! kept for the host to hand to the devices registry.
//!
//! A command is sent by `Radio::send_command` and its outcome comes back with
//! `Radio::take_command_results`: both move with the polls (`poll_pairing`, `poll`).

use std::collections::VecDeque;
use std::fmt;

use super::coordinator::HOST_ENDPOINT;
use super::{Radio, RadioError, RadioTransport};
use super::{ezsp, zcl};
use ezsp::{ApsFrame, IncomingMessage};

/// Commands in flight at once; one more is refused.
pub const MAX_COMMANDS: usize = 8;
/// Each attempt waits this long for the Default Response, in milliseconds.
///
/// Rule: neither a first attempt nor a retry is sent unless the time left before the command's
/// deadline is at least `ANSWER_TIMEOUT_MS - SEND_SLACK_MS`, so that a light is never switched
/// by a frame whose answer cannot be waited for (the client would be told "failed" for a light
/// that then switches). Otherwise the command ends with its last failure, or "device did not
/// answer", without sending.
const ANSWER_TIMEOUT_MS: u64 = 5_000;
/// The slack of that rule: the retry of a command sent a few milliseconds after it was queued
/// (5 s + 2 s + 5 s = 12 s) would otherwise always be refused.
const SEND_SLACK_MS: u64 = 500;
/// A command that got no answer, or found the radio busy, is sent again this long after, in
/// milliseconds (Sowel's `dispatchWithRetry`: one retry after 2 s).
const RETRY_MS: u64 = 2_000;
/// A command is sent this many times in all (one retry).
const MAX_ATTEMPTS: u8 = 2;
/// Outcomes kept until the host takes them; the oldest is dropped past this.
const MAX_RESULTS: usize = 16;
/// State reads waiting to be sent; the oldest is dropped past this.
const MAX_READS: usize = 16;
/// Route updates kept until the host takes them (one per device); the oldest is dropped past
/// this.
const MAX_ROUTES: usize = 64;

/// What a command is, for the host to tell its outcome apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandHandle(u32);

/// The device a command goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandTarget {
    /// Most significant byte first.
    pub eui64: [u8; 8],
    /// The node address the registry last knew; `None` when it knows none.
    pub nwk: Option<u16>,
    /// The endpoint of the device the command goes to.
    pub endpoint: u8,
}

/// Why a command did not get through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandFailure {
    /// No Default Response in time (twice).
    NoAnswer,
    /// The device answered with an error status.
    Refused(u8),
    /// The radio stayed busy (twice).
    Busy,
    /// Neither the registry, the radio's cache nor the radio's address table knows the device.
    UnknownAddress,
    /// The radio failed while looking the device up or sending to it (the text is the error's).
    Radio(String),
}

impl fmt::Display for CommandFailure {
    /// The text the order route answers with.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandFailure::NoAnswer => write!(f, "device did not answer"),
            CommandFailure::Refused(status) => {
                write!(f, "device refused the command (status 0x{status:02X})")
            }
            CommandFailure::Busy => write!(f, "radio busy"),
            CommandFailure::UnknownAddress => write!(f, "device address unknown"),
            CommandFailure::Radio(reason) => write!(f, "radio error: {reason}"),
        }
    }
}

/// What a device's messages told of how to reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteUpdate {
    pub eui64: [u8; 8],
    pub nwk: u16,
    /// The endpoint its On/Off, Level Control or Color Control frames come from, when the
    /// message was one.
    pub endpoint: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
enum Stage {
    Send { due: u64 },
    Wait { until: u64 },
}

#[derive(Debug)]
struct InFlight {
    handle: CommandHandle,
    target: CommandTarget,
    command: zcl::Command,
    /// The coordinator's clock: nothing is sent, and nothing waited for, after it.
    deadline: u64,
    /// Times it was sent.
    attempts: u8,
    /// The transaction number and node address of each attempt (at most `MAX_ATTEMPTS`): the
    /// Default Response to any of them, even a late one, answers the command.
    sent: Vec<(u8, u16)>,
    last: Option<CommandFailure>,
    stage: Stage,
}

/// The attributes of one cluster to read once a command succeeded: up to four in one request
/// (a fixed array, no allocation), so the colour comes back in one frame.
#[derive(Debug, Clone, Copy)]
struct StateRead {
    nwk: u16,
    endpoint: u8,
    cluster: u16,
    attributes: [u16; MAX_READ_ATTRIBUTES],
    /// How many of `attributes` are used.
    count: usize,
}

/// The most attributes one state read asks for.
const MAX_READ_ATTRIBUTES: usize = 4;

/// What the radio keeps for commands and routes.
#[derive(Debug, Default)]
pub struct Commands {
    /// At most `MAX_COMMANDS`.
    in_flight: Vec<InFlight>,
    /// At most `MAX_RESULTS`.
    results: VecDeque<(CommandHandle, Result<(), CommandFailure>)>,
    /// At most `MAX_READS`.
    reads: VecDeque<StateRead>,
    /// At most `MAX_ROUTES`, one per device.
    routes: Vec<RouteUpdate>,
    next_handle: u32,
}

impl Commands {
    /// Remembers how to reach a device: one entry per device, the endpoint kept once it is known.
    pub(super) fn note_route(&mut self, eui64: [u8; 8], nwk: u16, endpoint: Option<u8>) {
        if let Some(known) = self.routes.iter_mut().find(|r| r.eui64 == eui64) {
            known.nwk = nwk;
            known.endpoint = endpoint.or(known.endpoint);
            return;
        }
        if self.routes.len() >= MAX_ROUTES {
            self.routes.remove(0);
        }
        self.routes.push(RouteUpdate {
            eui64,
            nwk,
            endpoint,
        });
    }

    fn finish(&mut self, index: usize, result: Result<(), CommandFailure>) {
        let done = self.in_flight.remove(index);
        if self.results.len() >= MAX_RESULTS {
            self.results.pop_front();
            log::warn!("too many command outcomes not taken: the oldest was dropped");
        }
        self.results.push_back((done.handle, result));
    }

    /// Queues the read of the scale attributes of an electrical cluster (spec 048): sent by the
    /// next poll, its answer is a Read Attributes Response the coordinator takes the scale from.
    pub(super) fn queue_scale_read(&mut self, nwk: u16, endpoint: u8, cluster: u16) {
        let Some(wanted) = super::measures::scale_attributes(cluster) else {
            return;
        };
        let mut attributes = [0; MAX_READ_ATTRIBUTES];
        let count = wanted.len().min(MAX_READ_ATTRIBUTES);
        attributes[..count].copy_from_slice(&wanted[..count]);
        self.push_read(StateRead {
            nwk,
            endpoint,
            cluster,
            attributes,
            count,
        });
    }

    fn push_read(&mut self, read: StateRead) {
        if self.reads.len() >= MAX_READS {
            self.reads.pop_front();
        }
        self.reads.push_back(read);
    }
}

impl<T: RadioTransport, C: FnMut() -> u64> Radio<T, C> {
    /// Queues a command for a device; it is sent by the next polls. `deadline_in_ms` bounds the
    /// whole command: nothing is sent or waited for past it. At most `MAX_COMMANDS` at once.
    pub fn send_command(
        &mut self,
        target: CommandTarget,
        command: zcl::Command,
        deadline_in_ms: u64,
    ) -> Result<CommandHandle, RadioError> {
        if self.commands.in_flight.len() >= MAX_COMMANDS {
            return Err(RadioError::InvalidArgument("too many commands in flight"));
        }
        let now = (self.now)();
        let handle = CommandHandle(self.commands.next_handle);
        self.commands.next_handle = self.commands.next_handle.wrapping_add(1);
        self.commands.in_flight.push(InFlight {
            handle,
            target,
            command,
            deadline: now.saturating_add(deadline_in_ms),
            attempts: 0,
            sent: Vec::new(),
            last: None,
            stage: Stage::Send { due: now },
        });
        Ok(handle)
    }

    /// Pings a device (Zigbee2MQTT's availability): reads its ZCL version (Basic cluster), queued
    /// with the state reads and sent by the next poll. Any answer is a message from it, so the
    /// registry hears it; none, and it goes offline at its time. False when no address is known.
    pub fn ping(&mut self, target: CommandTarget) -> Result<bool, RadioError> {
        let Some(nwk) = self.command_address(&target)? else {
            return Ok(false);
        };
        // The answer is told apart by its sender's node address, which the registry gave.
        self.pairing.remember_address(nwk, target.eui64);
        let mut attributes = [0; MAX_READ_ATTRIBUTES];
        attributes[0] = zcl::ATTRIBUTE_ZCL_VERSION;
        self.commands.push_read(StateRead {
            nwk,
            endpoint: target.endpoint,
            cluster: zcl::CLUSTER_BASIC,
            attributes,
            count: 1,
        });
        Ok(true)
    }

    /// Commands sent and not finished.
    pub fn commands_in_flight(&self) -> usize {
        self.commands.in_flight.len()
    }

    /// The commands that finished since the last call, oldest first.
    pub fn take_command_results(&mut self) -> Vec<(CommandHandle, Result<(), CommandFailure>)> {
        self.commands.results.drain(..).collect()
    }

    /// How the devices' messages said they are reached since the last call.
    pub fn take_routes(&mut self) -> Vec<RouteUpdate> {
        std::mem::take(&mut self.commands.routes)
    }

    /// A message from a device whose address is known: what it tells of its route.
    pub(super) fn note_route_of(&mut self, message: &IncomingMessage, eui64: [u8; 8]) {
        let light_cluster = matches!(
            message.aps.cluster,
            zcl::CLUSTER_ON_OFF
                | zcl::CLUSTER_LEVEL_CONTROL
                | zcl::CLUSTER_COLOR_CONTROL
                | zcl::CLUSTER_WINDOW_COVERING
                | zcl::CLUSTER_THERMOSTAT
        );
        let endpoint = light_cluster.then_some(message.aps.source_endpoint);
        self.commands.note_route(eui64, message.sender, endpoint);
    }

    /// A Default Response that answers a command in flight: sender, cluster, the transaction
    /// number of any of its attempts and the command id it carries must all match. A command
    /// answered is not sent again, whatever its stage.
    pub(super) fn on_command_answer(&mut self, message: &IncomingMessage) {
        if self.commands.in_flight.is_empty() {
            return;
        }
        // A write is answered by a Write Attributes Response (spec 051), any other command by a
        // Default Response; a device may answer a write with either.
        let response = match zcl::parse_default_response(&message.payload) {
            Ok(response) => response,
            Err(_) => {
                let Ok(written) = zcl::parse_write_attributes_response(&message.payload) else {
                    return;
                };
                zcl::DefaultResponse {
                    tsn: written.tsn,
                    command: 0x02,
                    // The device's own refusal code, logged and reported as it is.
                    status: if written.written {
                        0
                    } else {
                        written.status.max(0x01)
                    },
                }
            }
        };
        let Some(index) = self.commands.in_flight.iter().position(|c| {
            c.sent.contains(&(response.tsn, message.sender))
                && c.command.cluster() == message.aps.cluster
                && c.command.id() == response.command
        }) else {
            return;
        };
        let command = self.commands.in_flight[index].command;
        let endpoint = self.commands.in_flight[index].target.endpoint;
        if response.status != 0 {
            log::warn!(
                "the device refused a command (status 0x{:02X})",
                response.status
            );
            self.commands
                .finish(index, Err(CommandFailure::Refused(response.status)));
            return;
        }
        self.commands.finish(index, Ok(()));
        // The state is read back once: the answer goes through the usual path of measures.
        let read = |cluster, attributes: &[u16]| {
            let mut array = [0; MAX_READ_ATTRIBUTES];
            let count = attributes.len().min(MAX_READ_ATTRIBUTES);
            array[..count].copy_from_slice(&attributes[..count]);
            StateRead {
                nwk: message.sender,
                endpoint,
                cluster,
                attributes: array,
                count,
            }
        };
        // A shutter has no On/Off and is still moving: its reports give the position (spec 050).
        // A thermostat has no On/Off either: its report gives the setpoint (spec 051).
        if !matches!(
            command.cluster(),
            zcl::CLUSTER_WINDOW_COVERING | zcl::CLUSTER_THERMOSTAT
        ) {
            self.commands
                .push_read(read(zcl::CLUSTER_ON_OFF, &[zcl::ATTRIBUTE_ON_OFF]));
        }
        let colour: &[u16] = match command {
            zcl::Command::MoveToColorTemperature(_) => &[zcl::ATTRIBUTE_COLOR_TEMPERATURE],
            zcl::Command::MoveToColor { .. } => {
                &[zcl::ATTRIBUTE_CURRENT_X, zcl::ATTRIBUTE_CURRENT_Y]
            }
            zcl::Command::MoveToHueSaturation { .. } => &[
                zcl::ATTRIBUTE_CURRENT_HUE,
                zcl::ATTRIBUTE_CURRENT_SATURATION,
            ],
            _ => &[],
        };
        if !colour.is_empty() {
            self.commands
                .push_read(read(zcl::CLUSTER_COLOR_CONTROL, colour));
        }
        if let zcl::Command::MoveToLevel(_) = command {
            self.commands.push_read(read(
                zcl::CLUSTER_LEVEL_CONTROL,
                &[zcl::ATTRIBUTE_CURRENT_LEVEL],
            ));
        }
    }

    /// The node address to send to: the registry's, else the coordinator's cache, else the
    /// radio's address table; `None` when none knows it.
    fn command_address(&mut self, target: &CommandTarget) -> Result<Option<u16>, RadioError> {
        let known = target.nwk.or_else(|| self.pairing.nwk_of(&target.eui64));
        if known.is_some() {
            return Ok(known);
        }
        let response = self.call(
            ezsp::ID_LOOKUP_NODE_ID_BY_EUI64,
            &ezsp::lookup_node_id_by_eui64(&target.eui64),
        )?;
        let found = ezsp::parse_lookup_node_id(&response).unwrap_or(None);
        if let Some(nwk) = found {
            self.pairing.remember_address(nwk, target.eui64);
        }
        Ok(found)
    }

    /// Sends what is due, gives up on what waited long enough, and sends the state reads. Called
    /// by every poll.
    pub(super) fn drive_commands(&mut self, wide: bool) -> Result<(), RadioError> {
        let mut index = 0;
        while index < self.commands.in_flight.len() {
            let now = (self.now)();
            let command = &self.commands.in_flight[index];
            let (stage, deadline) = (command.stage, command.deadline);
            match stage {
                Stage::Send { .. }
                    if deadline.saturating_sub(now)
                        < ANSWER_TIMEOUT_MS.saturating_sub(SEND_SLACK_MS) =>
                {
                    let failure = command.last.clone().unwrap_or(CommandFailure::NoAnswer);
                    self.commands.finish(index, Err(failure));
                }
                Stage::Send { due } if now >= due => {
                    let (target, frame_command) = (command.target, command.command);
                    let nwk = match self.command_address(&target) {
                        Ok(Some(nwk)) => nwk,
                        Ok(None) => {
                            self.commands
                                .finish(index, Err(CommandFailure::UnknownAddress));
                            continue;
                        }
                        Err(e) => {
                            self.commands
                                .finish(index, Err(CommandFailure::Radio(e.to_string())));
                            continue;
                        }
                    };
                    let tsn = self.next_tsn();
                    let aps = ApsFrame {
                        profile: zcl::PROFILE_HOME_AUTOMATION,
                        cluster: frame_command.cluster(),
                        source_endpoint: HOST_ENDPOINT,
                        destination_endpoint: target.endpoint,
                        options: ezsp::APS_OPTIONS_DEFAULT,
                        group_id: 0,
                        sequence: 0,
                    };
                    let frame = frame_command.frame(tsn);
                    let taken = match self.send_to_device(wide, nwk, &aps, tsn, &frame) {
                        Ok(taken) => taken,
                        Err(e) => {
                            self.commands
                                .finish(index, Err(CommandFailure::Radio(e.to_string())));
                            continue;
                        }
                    };
                    let now = (self.now)();
                    let command = &mut self.commands.in_flight[index];
                    command.attempts += 1;
                    if taken {
                        command.sent.push((tsn, nwk));
                        command.stage = Stage::Wait {
                            until: now.saturating_add(ANSWER_TIMEOUT_MS).min(deadline),
                        };
                    } else {
                        command.last = Some(CommandFailure::Busy);
                        if command.attempts >= MAX_ATTEMPTS {
                            self.commands.finish(index, Err(CommandFailure::Busy));
                            continue;
                        }
                        command.stage = Stage::Send {
                            due: now.saturating_add(RETRY_MS),
                        };
                    }
                    index += 1;
                }
                Stage::Wait { until, .. } if now >= until => {
                    let command = &mut self.commands.in_flight[index];
                    command.last = Some(CommandFailure::NoAnswer);
                    if command.attempts >= MAX_ATTEMPTS {
                        self.commands.finish(index, Err(CommandFailure::NoAnswer));
                        continue;
                    }
                    command.stage = Stage::Send {
                        due: now.saturating_add(RETRY_MS),
                    };
                    index += 1;
                }
                _ => index += 1,
            }
        }
        while let Some(read) = self.commands.reads.pop_front() {
            let tsn = self.next_tsn();
            let aps = ApsFrame {
                profile: zcl::PROFILE_HOME_AUTOMATION,
                cluster: read.cluster,
                source_endpoint: HOST_ENDPOINT,
                destination_endpoint: read.endpoint,
                options: ezsp::APS_OPTIONS_DEFAULT,
                group_id: 0,
                sequence: 0,
            };
            let request = zcl::read_attributes_request(tsn, &read.attributes[..read.count]);
            // Best effort: a read the radio refuses is dropped, the device reports on its own
            // or the next order reads again.
            self.send_to_device(wide, read.nwk, &aps, tsn, &request)?;
        }
        Ok(())
    }
}
