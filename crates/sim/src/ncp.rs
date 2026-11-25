//! The simulated radio (NCP): bytes in, bytes out, speaking real ASH and EZSP, with simulated
//! devices behind it. The tests of the core and the host drive it directly (with fault
//! switches); the host runs it for `--sim`, through `SimTransport`.

use std::path::{Path, PathBuf};

use solite_core::radio::ash::{AshFrame, DecodeEvent, Decoder, encode};

use crate::devices::{
    AWAKE_MS, Attr, CieAnswer, ConfigureAnswer, DeviceReport, Kind, MOTION_CLEAR_MS, SimDevice,
    battery_report, color_raw, color_temp_report, color_xy_report, humidity_report, level_report,
    on_off_report, temperature_report, zone_notification,
};
use crate::memory::{self, Memory};
use crate::scenario::{NetworkInit, Scenario};

/// Most devices a radio holds (the scenario's bound too).
pub const MAX_DEVICES: usize = 64;
/// Each record the tests inspect (bytes received, commands, messages sent) holds at most this
/// many entries when recording is on; the entries past it are not kept.
pub const MAX_RECORDS: usize = 10_000;

/// How the simulated radio behaves after the reset.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    /// Never answers RST.
    Silent,
    /// Answers RST like Normal, but never answers a command (it still acknowledges it).
    MuteOnCommands,
    /// Answers RST, but never acknowledges nor answers a command.
    DeafToCommands,
    /// Answers RST with RSTACK, then an ERROR frame with this code.
    ErrorAfterRstAck(u8),
}

/// What the radio keeps in its flash: the network it formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredNetwork {
    /// As the radio receives it (little-endian).
    pub extended_pan_wire: [u8; 8],
    pub pan_id: u16,
    pub channel: u8,
    /// 1 coordinator, 2 router.
    pub node_type: u8,
}

/// A message the host sent with `sendUnicast`, as the radio read it.
#[derive(Clone, Debug)]
pub struct SentMessage {
    pub destination: u16,
    pub profile: u16,
    pub cluster: u16,
    pub source_endpoint: u8,
    pub destination_endpoint: u8,
    pub tag: u16,
    pub message: Vec<u8>,
}

pub struct Ncp {
    pub mode: Mode,
    pub protocol: u8,
    pub stack_type: u8,
    pub stack_version: u16,
    /// The address as the radio sends it (little-endian).
    pub eui64_wire: [u8; 8],
    pub network_state: u8,
    /// Send an asynchronous callback frame before every response.
    pub callback_first: bool,
    /// The three records below are kept only when this is on (off by default: a long run must
    /// not grow without bound).
    pub record: bool,
    /// Every byte received, in order (at most `MAX_RECORDS`).
    pub received: Vec<u8>,
    /// Every EZSP command received: frame id and parameters (at most `MAX_RECORDS`).
    pub commands: Vec<(u16, Vec<u8>)>,
    /// Every `sendUnicast` received (at most `MAX_RECORDS`).
    pub sent: Vec<SentMessage>,
    /// The radio's time, in milliseconds, when each message of `sent` was sent.
    pub sent_at: Vec<u64>,
    /// The network in the radio's flash. It outlives the connection: a second host run finds it.
    pub network: Option<StoredNetwork>,
    /// The keys received by `setInitialSecurityState`: network key then trust-centre key.
    pub keys: Option<([u8; 16], [u8; 16])>,
    /// The host's endpoint 1 is registered (kept across connections).
    pub endpoint_registered: bool,
    /// "Network up" is sent before the response of `networkInit` / `formNetwork`, not after.
    pub network_up_before_response: bool,
    /// "Network up" is never sent.
    pub no_network_up: bool,
    /// "Network up" comes this long after the command, in milliseconds.
    pub network_up_delay_ms: u64,
    /// `setConcentrator` is refused.
    pub refuse_concentrator: bool,
    /// `permitJoining` with a time (opening) is refused.
    pub refuse_permit_joining: bool,
    /// Every response is preceded by a late response to the previous sequence number.
    pub stale_response_first: bool,
    /// Callbacks of kinds the host does not use, sent when pairing opens.
    pub unused_burst: usize,
    /// Responses carry the overflow flag.
    pub overflow_flag: bool,
    /// The next `sendUnicast` commands answered "busy".
    pub busy_sends: u32,
    /// The simulated devices (at most `MAX_DEVICES`).
    pub devices: Vec<SimDevice>,
    /// What `lookupEui64ByNodeId` knows: node address and address as on the wire.
    pub known_addresses: Vec<(u16, [u8; 8])>,
    /// The sender's address the radio puts in the packet info of an incoming message (EZSP 14+).
    pub packet_eui_wire: Option<[u8; 8]>,
    /// The join callbacks of a pairing come before the response of `permitJoining`.
    pub join_before_response: bool,
    /// Devices that only join (trust-centre join, no answers), after the main devices.
    pub crowd: usize,
    /// Milliseconds between two joins of the crowd.
    pub crowd_spread_ms: u64,
    /// How much faster than real time the scenario runs: a motion sensor clears its alarm 60 s
    /// of scenario time after raising it.
    pub speed: u32,
    /// `getEui64` answers nothing usable (after the identification).
    pub eui64_fails: bool,
    decoder: Decoder,
    tx_frm: u8,
    rx_next: u8,
    now: u64,
    scheduled: Vec<(u64, Vec<u8>)>,
    /// When the pairing window closes, if one is open.
    pairing_until: Option<u64>,
    memory_file: Option<PathBuf>,
    dirty: bool,
}

/// What the radio sends for one command: callbacks before the response, the response, callbacks
/// after it (EZSP payloads).
#[derive(Default)]
struct Reply {
    before: Vec<Vec<u8>>,
    response: Option<Vec<u8>>,
    after: Vec<Vec<u8>>,
}

const SL_OK: u32 = 0;
const SL_BUSY: u32 = 4;
const SL_NOT_JOINED: u32 = 0x17;
const SL_FAIL: u32 = 1;

/// The first network address given to a device that has none.
const FIRST_NWK: u16 = 0x2000;

/// The clusters the host sets up reporting for.
const REPORTABLE: [u16; 9] = [
    0x0402, 0x0405, 0x0403, 0x0400, 0x0406, 0x0006, 0x0008, 0x0300, 0x0001,
];

/// An EZSP callback frame (extended format, asynchronous callback flag).
fn callback(id: u16, params: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x00, 0x90, 0x01];
    frame.extend_from_slice(&id.to_le_bytes());
    frame.extend_from_slice(params);
    frame
}

fn aps(profile: u16, cluster: u16, source: u8, destination: u8) -> Vec<u8> {
    let mut bytes = profile.to_le_bytes().to_vec();
    bytes.extend_from_slice(&cluster.to_le_bytes());
    bytes.push(source);
    bytes.push(destination);
    bytes.extend_from_slice(&0x1140u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.push(0);
    bytes
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

/// Keeps `item` unless the record is full.
fn keep<T>(record: &mut Vec<T>, item: T) {
    if record.len() < MAX_RECORDS {
        record.push(item);
    }
}

impl Default for Ncp {
    fn default() -> Self {
        Ncp::new()
    }
}

impl Ncp {
    /// A radio with nothing in its flash, no device, and the records off.
    pub fn new() -> Self {
        Ncp {
            mode: Mode::Normal,
            protocol: 13,
            stack_type: 2,
            stack_version: 0x7440,
            eui64_wire: [0x56, 0x34, 0x12, 0xFE, 0xFF, 0xB9, 0x6F, 0x8C],
            network_state: 0,
            callback_first: false,
            record: false,
            received: Vec::new(),
            commands: Vec::new(),
            sent: Vec::new(),
            sent_at: Vec::new(),
            network: None,
            keys: None,
            endpoint_registered: false,
            network_up_before_response: false,
            no_network_up: false,
            network_up_delay_ms: 0,
            refuse_concentrator: false,
            refuse_permit_joining: false,
            stale_response_first: false,
            unused_burst: 0,
            overflow_flag: false,
            busy_sends: 0,
            devices: Vec::new(),
            known_addresses: Vec::new(),
            packet_eui_wire: None,
            crowd: 0,
            crowd_spread_ms: 50,
            speed: 1,
            eui64_fails: false,
            join_before_response: false,
            decoder: Decoder::new(),
            tx_frm: 0,
            rx_next: 0,
            now: 0,
            scheduled: Vec::new(),
            pairing_until: None,
            memory_file: None,
            dirty: false,
        }
    }

    /// A radio that keeps the records the tests inspect.
    pub fn recording() -> Self {
        Ncp {
            record: true,
            ..Ncp::new()
        }
    }

    /// The radio and devices a scenario describes. The radio's memory is read from
    /// `memory_file` when it exists (the scenario's `network` then does not apply) and written
    /// there when it changes. An unreadable or unparsable file, or one that cannot be written,
    /// is an error naming it.
    pub fn from_scenario(scenario: &Scenario, memory_file: Option<&Path>) -> Result<Ncp, String> {
        let mut ncp = Ncp::new();
        ncp.protocol = scenario.radio.ezsp;
        ncp.stack_version = scenario.radio.stack_version;
        ncp.eui64_wire = crate::devices::wire(scenario.radio.eui64);
        ncp.devices = scenario
            .devices
            .iter()
            .take(MAX_DEVICES)
            .map(|spec| {
                SimDevice::of_kind(
                    spec.kind,
                    crate::devices::wire(spec.eui64),
                    spec.manufacturer.as_deref(),
                    spec.model.as_deref(),
                )
            })
            .collect();
        let Some(file) = memory_file else {
            ncp.apply_initial_network(scenario.radio.network);
            return Ok(ncp);
        };
        match memory::load(file)? {
            Some(stored) => {
                ncp.restore(stored);
                // Written back now too: a file that cannot be written is an error before
                // anything runs, even when listening.
                memory::save(file, &ncp.to_memory())?;
            }
            None => {
                ncp.apply_initial_network(scenario.radio.network);
                // The file is written now: a radio that cannot keep its memory is an error
                // before anything runs.
                memory::save(file, &ncp.to_memory())?;
            }
        }
        ncp.memory_file = Some(file.to_path_buf());
        Ok(ncp)
    }

    fn apply_initial_network(&mut self, init: NetworkInit) {
        if init == NetworkInit::Formed {
            self.network = Some(StoredNetwork {
                extended_pan_wire: [0x5A; 8],
                pan_id: 0x5A5A,
                channel: 15,
                node_type: 1,
            });
            self.keys = Some((
                [
                    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
                    0x0E, 0x0F, 0x10,
                ],
                [
                    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D,
                    0x1E, 0x1F, 0x20,
                ],
            ));
            self.network_state = 2;
        }
    }

    fn restore(&mut self, stored: Memory) {
        let (network, keys) = stored.network_and_keys();
        if network.is_some() {
            self.network_state = 2;
        }
        self.network = network;
        self.keys = keys;
        for saved in &stored.devices {
            if let Some(device) = self
                .devices
                .iter_mut()
                .find(|d| d.eui_wire == saved.eui_wire())
            {
                saved.apply_to(device);
            }
        }
    }

    fn to_memory(&self) -> Memory {
        Memory::new(self.network.as_ref(), self.keys.as_ref(), &self.devices)
    }

    /// Writes the memory file if something changed since it was last written. A failure is
    /// logged, never fatal: the simulation goes on.
    fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Some(file) = &self.memory_file
            && let Err(message) = memory::save(file, &self.to_memory())
        {
            log::error!("{message}");
        }
    }

    /// Stops writing the memory file: what was read from it stays, what changes from now on is
    /// kept in RAM only. For a caller whose storage is slow and not what it measures.
    pub fn keep_memory_in_ram(&mut self) {
        self.memory_file = None;
    }

    /// Sets the time the radio sees, in milliseconds (the caller's clock).
    pub fn set_now(&mut self, now: u64) {
        self.now = now;
    }

    /// When the next timed callback is due, if any.
    pub fn next_due(&self) -> Option<u64> {
        self.scheduled.iter().map(|(at, _)| *at).min()
    }

    /// True while a pairing window is open.
    pub fn pairing_open(&self) -> bool {
        self.pairing_until.is_some_and(|until| self.now < until)
    }

    /// The device joins now if pairing is open (or it already joined): `true`. `false`: the
    /// window is closed, the caller tries again later.
    pub fn join_device(&mut self, index: usize) -> bool {
        let Some(device) = self.devices.get(index) else {
            return true;
        };
        if device.joined {
            return true;
        }
        if !self.pairing_open() {
            return false;
        }
        let delay = self.devices.get(index).map_or(0, |d| d.join_delay_ms);
        for (extra, payload) in self.join_callbacks(index) {
            self.scheduled.push((self.now + delay + extra, payload));
        }
        self.flush();
        true
    }

    /// The device reports what the scenario says, as far as its state allows: it must be in the
    /// network, and the host must have set up the reporting of what it reports (a motion sensor
    /// must be enrolled). `false` when nothing was sent.
    pub fn device_report(&mut self, index: usize, report: &DeviceReport) -> bool {
        let now = self.now;
        let Some(device) = self.devices.get(index) else {
            return false;
        };
        if !device.joined {
            return false;
        }
        let nwk = device.nwk;
        let mut frames: Vec<(u16, Vec<u8>)> = Vec::new();
        let mut clear_alarm = false;
        if let Some(device) = self.devices.get_mut(index) {
            if let Some(value) = report.temperature
                && device.reports_attribute(0x0402, 0x0000)
            {
                let tsn = device.next_tsn();
                frames.push((
                    0x0402,
                    temperature_report(tsn, (value * 100.0).round() as i16),
                ));
            }
            if let Some(value) = report.humidity
                && device.reports_attribute(0x0405, 0x0000)
            {
                let tsn = device.next_tsn();
                frames.push((0x0405, humidity_report(tsn, (value * 100.0).round() as u16)));
            }
            if let Some(percent) = report.battery
                && device.reports_attribute(0x0001, 0x0021)
            {
                let tsn = device.next_tsn();
                frames.push((0x0001, battery_report(tsn, percent.min(100) * 2)));
            }
            if let Some(on) = report.light_state {
                device.relay_on = on;
                if device.reports_attribute(0x0006, 0x0000) {
                    let tsn = device.next_tsn();
                    frames.push((0x0006, on_off_report(tsn, on)));
                }
            }
            if let Some(mireds) = report.color_temp {
                device.color_temp = mireds;
                if device.reports_attribute(0x0300, 0x0007) {
                    let tsn = device.next_tsn();
                    frames.push((0x0300, color_temp_report(tsn, mireds)));
                }
            }
            if let Some((x, y)) = report.color {
                device.color_x = color_raw(x);
                device.color_y = color_raw(y);
                if device.reports_attribute(0x0300, 0x0003)
                    && device.reports_attribute(0x0300, 0x0004)
                {
                    let tsn = device.next_tsn();
                    frames.push((0x0300, color_xy_report(tsn, device.color_x, device.color_y)));
                }
            }
            if let Some(alarm) = report.motion
                && device.enrolled
            {
                let tsn = device.next_tsn();
                frames.push((0x0500, zone_notification(tsn, alarm)));
                clear_alarm = alarm;
            }
            if !frames.is_empty() && device.sleepy_ms.is_some() {
                device.awake_until = now + AWAKE_MS;
            }
        }
        let sent = !frames.is_empty();
        for (cluster, frame) in frames {
            let message = self.incoming(nwk, 0x0104, cluster, 1, &frame);
            self.scheduled.push((now, message));
        }
        if clear_alarm && let Some(device) = self.devices.get_mut(index) {
            // The sensor clears its alarm by itself.
            let tsn = device.next_tsn();
            let cleared = zone_notification(tsn, false);
            let message = self.incoming(nwk, 0x0104, 0x0500, 1, &cleared);
            let clear_after = MOTION_CLEAR_MS / u64::from(self.speed.max(1));
            self.scheduled.push((now + clear_after, message));
        }
        self.flush();
        sent
    }

    /// A message arrives from the network at the next tick: a unicast to the host's endpoint 1.
    pub fn inject(&mut self, sender: u16, profile: u16, cluster: u16, source: u8, payload: &[u8]) {
        self.inject_kind(0x00, sender, profile, cluster, source, payload);
    }

    /// The same with the message kind of `incomingMessageHandler` given (4 is a broadcast).
    pub fn inject_kind(
        &mut self,
        kind: u8,
        sender: u16,
        profile: u16,
        cluster: u16,
        source: u8,
        payload: &[u8],
    ) {
        let mut frame = self.incoming(sender, profile, cluster, source, payload);
        if let Some(byte) = frame.get_mut(5) {
            *byte = kind;
        }
        self.scheduled.push((self.now, frame));
    }

    /// Time passes: returns the callbacks that are due (ASH frames).
    pub fn on_tick(&mut self, now: u64) -> Vec<u8> {
        self.now = now;
        let mut out = Vec::new();
        let mut later = Vec::new();
        for (at, payload) in std::mem::take(&mut self.scheduled) {
            if at <= now {
                out.extend(self.data(payload));
            } else {
                later.push((at, payload));
            }
        }
        self.scheduled = later;
        self.flush();
        out
    }

    /// Feeds the bytes the host wrote; returns what the radio answers. Bytes that are not a
    /// valid frame are dropped, as a real radio's ASH layer does.
    pub fn on_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        if self.record {
            let room = MAX_RECORDS.saturating_sub(self.received.len());
            self.received
                .extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
        let mut out = Vec::new();
        for event in self.decoder.feed(bytes) {
            let DecodeEvent::Frame(frame) = event else {
                continue;
            };
            match frame {
                AshFrame::Rst => {
                    self.tx_frm = 0;
                    self.rx_next = 0;
                    match self.mode {
                        Mode::Silent => {}
                        Mode::Normal | Mode::MuteOnCommands | Mode::DeafToCommands => {
                            out.extend(rstack())
                        }
                        Mode::ErrorAfterRstAck(code) => {
                            out.extend(rstack());
                            out.extend(encode(&AshFrame::Error { version: 2, code }));
                        }
                    }
                }
                AshFrame::Data { frm, payload, .. }
                    if !matches!(self.mode, Mode::Silent | Mode::DeafToCommands) =>
                {
                    if frm != self.rx_next {
                        continue;
                    }
                    self.rx_next = (self.rx_next + 1) % 8;
                    out.extend(encode(&AshFrame::Ack {
                        ack: self.rx_next,
                        not_ready: false,
                    }));
                    if self.callback_first {
                        // Frame control 0x90: response and asynchronous callback.
                        out.extend(self.data(vec![0x77, 0x90, 0x01, 0x19, 0x00, 0x00]));
                    }
                    if self.mode == Mode::MuteOnCommands {
                        continue;
                    }
                    let reply = self.respond(&payload);
                    for frame in reply.before {
                        out.extend(self.data(frame));
                    }
                    if let Some(response) = reply.response {
                        out.extend(self.data(response));
                    }
                    for frame in reply.after {
                        out.extend(self.data(frame));
                    }
                }
                _ => {}
            }
        }
        self.flush();
        out
    }

    fn data(&mut self, payload: Vec<u8>) -> Vec<u8> {
        let frame = AshFrame::Data {
            frm: self.tx_frm,
            ack: self.rx_next,
            retransmit: false,
            payload,
        };
        self.tx_frm = (self.tx_frm + 1) % 8;
        encode(&frame)
    }

    fn wide(&self) -> bool {
        self.protocol >= 14
    }

    /// A status as the radio sends it: four bytes on EZSP 14+, one (EmberStatus) on 13.
    fn st(&self, sl: u32) -> Vec<u8> {
        if self.wide() {
            sl.to_le_bytes().to_vec()
        } else {
            vec![match sl {
                SL_OK => 0x00,
                SL_BUSY => 0xA1,
                SL_NOT_JOINED => 0x93,
                _ => 0x01,
            }]
        }
    }

    fn stack_status(&self, up: bool) -> Vec<u8> {
        let params = if self.wide() {
            (if up { 0x15u32 } else { 0x16 }).to_le_bytes().to_vec()
        } else {
            vec![if up { 0x90 } else { 0x91 }]
        };
        callback(0x0019, &params)
    }

    fn incoming(
        &self,
        sender: u16,
        profile: u16,
        cluster: u16,
        source: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut params = vec![0x00]; // unicast
        params.extend(aps(profile, cluster, source, 1));
        if self.wide() {
            params.extend_from_slice(&sender.to_le_bytes());
            // The sender's 64-bit address: all zeros when the radio does not know it.
            params.extend_from_slice(&self.packet_eui_wire.unwrap_or([0; 8]));
            params.extend_from_slice(&[0xFF, 0xFF, 0xC8, 0xD0]); // indexes, lqi, rssi
            params.extend_from_slice(&0u32.to_le_bytes()); // timestamp
        } else {
            params.extend_from_slice(&[0xC8, 0xD0]); // lqi, rssi
            params.extend_from_slice(&sender.to_le_bytes());
            params.extend_from_slice(&[0xFF, 0xFF]); // indexes
        }
        params.push(payload.len() as u8);
        params.extend_from_slice(payload);
        callback(0x0045, &params)
    }

    fn message_sent(&self, destination: u16, aps_bytes: &[u8], tag: u16, status: u32) -> Vec<u8> {
        let mut params = Vec::new();
        if self.wide() {
            params.extend_from_slice(&status.to_le_bytes());
            params.push(0x00);
            params.extend_from_slice(&destination.to_le_bytes());
            params.extend_from_slice(aps_bytes);
            params.extend_from_slice(&tag.to_le_bytes());
        } else {
            params.push(0x00);
            params.extend_from_slice(&destination.to_le_bytes());
            params.extend_from_slice(aps_bytes);
            params.push(tag as u8);
            params.extend(self.st(status));
            params.push(0); // length of the message contents, which a newer stack appends
        }
        callback(0x003F, &params)
    }

    fn trust_center_join(&self, nwk: u16, eui_wire: [u8; 8], status: u8, decision: u8) -> Vec<u8> {
        let mut params = nwk.to_le_bytes().to_vec();
        params.extend_from_slice(&eui_wire);
        params.push(status);
        params.push(decision);
        params.extend_from_slice(&0u16.to_le_bytes()); // parent
        callback(0x0024, &params)
    }

    /// Queues a callback: right after the response, or later when `delay` is not 0.
    fn emit(&mut self, reply: &mut Reply, delay: u64, payload: Vec<u8>) {
        if delay == 0 {
            reply.after.push(payload);
        } else {
            self.scheduled.push((self.now + delay, payload));
        }
    }

    fn network_up(&mut self, reply: &mut Reply) {
        if self.no_network_up {
            return;
        }
        let up = self.stack_status(true);
        if self.network_up_delay_ms > 0 {
            self.scheduled
                .push((self.now + self.network_up_delay_ms, up));
        } else if self.network_up_before_response {
            reply.before.push(up);
        } else {
            reply.after.push(up);
        }
    }

    /// The network is resumed: the devices that joined earlier send their reports.
    fn resume_reports(&mut self, reply: &mut Reply) {
        let devices: Vec<SimDevice> = self.devices.clone();
        for device in devices {
            if !device.joined || !device.reports_on_resume {
                continue;
            }
            for (cluster, frame) in &device.reports {
                let report = self.incoming(device.nwk, 0x0104, *cluster, 1, frame);
                self.emit(reply, device.answer_delay_ms, report);
            }
        }
    }

    /// A network address no device has.
    fn free_nwk(&self) -> u16 {
        let mut candidate = FIRST_NWK;
        while self.devices.iter().any(|d| d.nwk == candidate) {
            candidate += 1;
        }
        candidate
    }

    /// The device joins: marks it, gives it an address if it has none, and returns the callbacks
    /// of the join as `(extra delay, EZSP payload)`.
    fn join_callbacks(&mut self, index: usize) -> Vec<(u64, Vec<u8>)> {
        let nwk = self.free_nwk();
        let now = self.now;
        let Some(device) = self.devices.get_mut(index) else {
            return Vec::new();
        };
        device.joined = true;
        if device.nwk == 0 {
            device.nwk = nwk;
        }
        if device.sleepy_ms.is_some() {
            device.awake_until = now + device.join_delay_ms + AWAKE_MS;
        }
        let device = device.clone();
        self.dirty = true;
        let mut callbacks = vec![(
            0,
            self.trust_center_join(
                device.nwk,
                device.eui_wire,
                device.join_status,
                device.join_decision,
            ),
        )];
        if device.announce {
            let mut payload = vec![0x01];
            payload.extend_from_slice(&device.nwk.to_le_bytes());
            payload.extend_from_slice(&device.eui_wire);
            payload.push(device.capability);
            let announce = self.incoming(device.nwk, 0x0000, 0x0013, 0, &payload);
            callbacks.push((0, announce.clone()));
            if device.announce_twice {
                callbacks.push((100, announce));
            }
        }
        callbacks
    }

    /// Pairing opens: the devices that join by themselves (and the crowd) join.
    fn on_permit_joining(&mut self, reply: &mut Reply, seconds: u8) {
        self.pairing_until = Some(self.now + u64::from(seconds) * 1000);
        for i in 0..self.unused_burst {
            // childJoinHandler, then incomingRouteRecordHandler: callbacks the host ignores.
            let burst = if i % 2 == 0 {
                callback(0x0023, &[0, 1, 0x34, 0x12, 1, 2, 3, 4, 5, 6, 7, 8, 2])
            } else {
                callback(0x0059, &[0x34, 0x12, 1, 2, 3, 4, 5, 6, 7, 8, 0xC8, 0xD0, 0])
            };
            reply.after.push(burst);
        }
        let mut delay = 0;
        let mut any_joined = false;
        for index in 0..self.devices.len() {
            let Some(device) = self.devices.get(index) else {
                continue;
            };
            if device.joined || !device.auto_join {
                continue;
            }
            delay = device.join_delay_ms;
            any_joined = true;
            for (extra, payload) in self.join_callbacks(index) {
                self.emit(reply, delay + extra, payload);
            }
        }
        if any_joined {
            for i in 0..self.crowd {
                let mut eui = [0xA0, 0, 0, 0, 0, 0x4B, 0x12, 0x00];
                eui[1] = i as u8;
                let join = self.trust_center_join(0x1000 + i as u16, eui, 1, 0);
                // Spread over time, as a real burst of joins is; or all at once with a spread of
                // 0.
                self.emit(reply, delay + self.crowd_spread_ms * (i as u64 + 1), join);
            }
        }
    }

    /// The device answers what the host sent it.
    fn device_answers(
        &mut self,
        reply: &mut Reply,
        sent: &SentMessage,
        aps_bytes: &[u8],
        status: u32,
    ) {
        let Some(index) = self
            .devices
            .iter()
            .position(|d| d.joined && d.nwk == sent.destination)
        else {
            return;
        };
        let Some(device) = self.devices.get(index).cloned() else {
            return;
        };
        let delay = device.answer_delay_ms;
        let message_sent = self.message_sent(sent.destination, aps_bytes, sent.tag, status);
        if status != SL_OK {
            reply.after.push(message_sent);
            return;
        }
        // A sleepy device that is not listening does not answer.
        if device.sleepy_ms.is_some() && self.now >= device.awake_until {
            reply.after.push(message_sent);
            return;
        }
        let message = &sent.message;
        let control = message.first().copied().unwrap_or(0);
        let mut answers = Vec::new();
        if sent.profile == 0x0000 && sent.cluster == 0x0005 {
            let Some(endpoints) = device.endpoints.clone() else {
                reply.after.push(message_sent);
                return;
            };
            let Some(tsn) = message.first().copied() else {
                reply.after.push(message_sent);
                return;
            };
            let response = |tsn: u8, endpoints: &[u8], status: u8| {
                let mut payload = vec![tsn, status];
                payload.extend_from_slice(&device.nwk.to_le_bytes());
                payload.push(endpoints.len() as u8);
                payload.extend_from_slice(endpoints);
                payload
            };
            if device.decoys {
                // Another device with the right transaction number, then the right device with
                // another one: both say endpoint 9.
                let other = self.incoming(device.nwk ^ 1, 0, 0x8005, 0, &response(tsn, &[9], 0));
                answers.push(other);
                let wrong_tsn =
                    self.incoming(device.nwk, 0, 0x8005, 0, &response(tsn ^ 0x55, &[9], 0));
                answers.push(wrong_tsn);
            }
            let list: Vec<u8> = if device.endpoint_status == 0 {
                endpoints
            } else {
                Vec::new()
            };
            let right = self.incoming(
                device.nwk,
                0,
                0x8005,
                0,
                &response(tsn, &list, device.endpoint_status),
            );
            answers.push(right);
        } else if sent.profile == 0x0104 && sent.cluster == 0x0000 && message.get(2) == Some(&0x00)
        {
            if let Some(device) = self.devices.get_mut(index)
                && device.ignore_reads > 0
            {
                if device.ignore_reads != u32::MAX {
                    device.ignore_reads -= 1;
                }
                reply.after.push(message_sent);
                return;
            }
            let Some(tsn) = message.get(1).copied() else {
                reply.after.push(message_sent);
                return;
            };
            let source = sent.destination_endpoint;
            let records = |manufacturer: &Attr, model: &Attr| {
                let mut bytes = Vec::new();
                for (id, attr) in [(0x0004u16, manufacturer), (0x0005, model)] {
                    bytes.extend_from_slice(&id.to_le_bytes());
                    match attr {
                        Attr::Text(text) => {
                            bytes.push(0x00);
                            bytes.push(0x42);
                            bytes.push(text.len() as u8);
                            bytes.extend_from_slice(text);
                        }
                        Attr::Unsupported => bytes.push(0x86),
                    }
                }
                bytes
            };
            let frame = |tsn: u8, body: &[u8]| {
                let mut f = vec![0x18, tsn, 0x01];
                f.extend_from_slice(body);
                f
            };
            if device.decoys {
                let wrong = records(
                    &Attr::Text(b"WRONG".to_vec()),
                    &Attr::Text(b"WRONG".to_vec()),
                );
                let other = self.incoming(device.nwk ^ 1, 0x0104, 0, source, &frame(tsn, &wrong));
                answers.push(other);
                let wrong_tsn = self.incoming(
                    device.nwk,
                    0x0104,
                    0,
                    source,
                    &frame(tsn.wrapping_add(1), &wrong),
                );
                answers.push(wrong_tsn);
                // A Default Response with the right transaction number.
                let default = self.incoming(
                    device.nwk,
                    0x0104,
                    0,
                    source,
                    &[0x18, tsn, 0x0B, 0x00, 0x00],
                );
                answers.push(default);
            }
            let right = self.incoming(
                device.nwk,
                0x0104,
                0,
                source,
                &frame(tsn, &records(&device.manufacturer, &device.model)),
            );
            answers.push(right);
        } else if sent.profile == 0x0000 && sent.cluster == 0x0004 {
            if let Some(device) = self.devices.get_mut(index)
                && device.ignore_descriptors > 0
            {
                if device.ignore_descriptors != u32::MAX {
                    device.ignore_descriptors -= 1;
                }
                reply.after.push(message_sent);
                return;
            }
            let (Some(tsn), Some(endpoint)) = (message.first().copied(), message.get(3).copied())
            else {
                reply.after.push(message_sent);
                return;
            };
            let mut descriptor = vec![endpoint];
            descriptor.extend_from_slice(&0x0104u16.to_le_bytes());
            descriptor.extend_from_slice(&device.kind.device_id().to_le_bytes());
            descriptor.push(0); // device version
            descriptor.push(device.input_clusters.len() as u8);
            for cluster in &device.input_clusters {
                descriptor.extend_from_slice(&cluster.to_le_bytes());
            }
            descriptor.push(1);
            descriptor.extend_from_slice(&0x0019u16.to_le_bytes());
            let mut payload = vec![tsn, 0x00];
            payload.extend_from_slice(&device.nwk.to_le_bytes());
            payload.push(descriptor.len() as u8);
            payload.extend(descriptor);
            answers.push(self.incoming(device.nwk, 0, 0x8004, 0, &payload));
        } else if sent.profile == 0x0000 && sent.cluster == 0x0021 {
            // Bind_req: [tsn][source EUI64][source endpoint][cluster][mode][destination...]
            let (Some(tsn), Some(cluster)) = (message.first().copied(), u16_at(message, 10)) else {
                reply.after.push(message_sent);
                return;
            };
            let status = if device.bind_refused.contains(&cluster) {
                0x84
            } else {
                0x00
            };
            answers.push(self.incoming(device.nwk, 0, 0x8021, 0, &[tsn, status]));
            let first = self.devices.get_mut(index).is_some_and(|d| {
                let first = !d.bound;
                d.bound = true;
                first
            });
            self.dirty = true;
            if first {
                for (cluster, frame) in &device.reports_during_setup {
                    answers.push(self.incoming(device.nwk, 0x0104, *cluster, 1, frame));
                }
            }
        } else if sent.profile == 0x0104
            && sent.cluster == 0x0500
            && device.kind == Kind::Motion
            && device.input_clusters.contains(&0x0500)
        {
            // IAS zone: it tells its state, type and CIE address; writing the CIE address, then
            // the Zone Enroll Response, enrolls it.
            let Some(tsn) = message.get(1).copied() else {
                reply.after.push(message_sent);
                return;
            };
            let command = message.get(2).copied();
            let cluster_specific = control & 0x03 == 0x01;
            if !cluster_specific && command == Some(0x00) {
                let Some(live) = self.devices.get_mut(index) else {
                    reply.after.push(message_sent);
                    return;
                };
                if live.ignore_zone_reads > 0 {
                    if live.ignore_zone_reads != u32::MAX {
                        live.ignore_zone_reads -= 1;
                    }
                    reply.after.push(message_sent);
                    return;
                }
                if live.zone_reads_answered == 0 {
                    reply.after.push(message_sent);
                    return;
                }
                if live.zone_reads_answered != u32::MAX {
                    live.zone_reads_answered -= 1;
                }
                let mut frame = vec![0x18, tsn, 0x01];
                for pair in message.get(3..).unwrap_or(&[]).as_chunks::<2>().0 {
                    let attribute = u16::from_le_bytes([pair[0], pair[1]]);
                    frame.extend_from_slice(&attribute.to_le_bytes());
                    match (attribute, live.zone_type) {
                        (0x0000, _) => {
                            frame.extend_from_slice(&[0x00, 0x30, u8::from(live.enrolled)]);
                        }
                        (0x0001, Some(zone_type)) => {
                            frame.extend_from_slice(&[0x00, 0x31]);
                            frame.extend_from_slice(&zone_type.to_le_bytes());
                        }
                        (0x0010, _) => {
                            frame.extend_from_slice(&[0x00, 0xF0]);
                            frame.extend_from_slice(&live.cie);
                        }
                        _ => frame.push(0x86),
                    }
                }
                answers.push(self.incoming(device.nwk, 0x0104, 0x0500, 1, &frame));
            } else if !cluster_specific && command == Some(0x02) {
                let Some(live) = self.devices.get_mut(index) else {
                    reply.after.push(message_sent);
                    return;
                };
                let is_cie = u16_at(message, 3) == Some(0x0010);
                let answer = match live.cie_answer {
                    CieAnswer::Silent => None,
                    CieAnswer::Refuse(status) => Some(vec![0x18, tsn, 0x04, status, 0x10, 0x00]),
                    CieAnswer::Accept => {
                        if is_cie && let Some(bytes) = message.get(6..14) {
                            live.cie.copy_from_slice(bytes);
                            live.cie_written = true;
                            self.dirty = true;
                        }
                        Some(vec![0x18, tsn, 0x04, 0x00])
                    }
                };
                let Some(answer) = answer else {
                    reply.after.push(message_sent);
                    return;
                };
                answers.push(self.incoming(device.nwk, 0x0104, 0x0500, 1, &answer));
            } else if cluster_specific
                && command == Some(0x00)
                && let Some(device) = self.devices.get_mut(index)
                && device.cie_written
                && !device.zone_stays_unenrolled
            {
                device.enrolled = true;
                self.dirty = true;
            }
        } else if sent.profile == 0x0104
            && matches!(device.kind, Kind::Relay | Kind::Dimmer | Kind::ColorLight)
            && device.input_clusters.contains(&sent.cluster)
            && matches!(sent.cluster, 0x0006 | 0x0008 | 0x0300)
            && match control & 0x03 {
                // On/Off: Off (0), On (1), Toggle (2); Level Control: Move to Level with On/Off;
                // Color Control: Move to Hue and Saturation (6), Move to Color (7), Move to Color
                // Temperature (0x0A).
                0x01 => matches!(
                    (sent.cluster, message.get(2)),
                    (0x0006, Some(0x00..=0x02))
                        | (0x0008, Some(0x04))
                        | (0x0300, Some(0x06 | 0x07 | 0x0A))
                ),
                // Read Attributes.
                0x00 => message.get(2) == Some(&0x00),
                _ => false,
            }
        {
            let (Some(tsn), Some(command)) = (message.get(1).copied(), message.get(2).copied())
            else {
                reply.after.push(message_sent);
                return;
            };
            if device.silent {
                reply.after.push(message_sent);
                return;
            }
            if control & 0x03 == 0x00 {
                let mut frame = vec![0x18, tsn, 0x01];
                for pair in message.get(3..).unwrap_or(&[]).as_chunks::<2>().0 {
                    let attribute = u16::from_le_bytes(*pair);
                    frame.extend_from_slice(&attribute.to_le_bytes());
                    match (sent.cluster, attribute) {
                        (0x0006, 0x0000) => {
                            frame.extend_from_slice(&[0x00, 0x10, u8::from(device.relay_on)]);
                        }
                        (0x0008, 0x0000) => frame.extend_from_slice(&[0x00, 0x20, device.level]),
                        (0x0300, 0x0007) => {
                            frame.extend_from_slice(&[0x00, 0x21]);
                            frame.extend_from_slice(&device.color_temp.to_le_bytes());
                        }
                        (0x0300, 0x0003) => {
                            frame.extend_from_slice(&[0x00, 0x21]);
                            frame.extend_from_slice(&device.color_x.to_le_bytes());
                        }
                        (0x0300, 0x0004) => {
                            frame.extend_from_slice(&[0x00, 0x21]);
                            frame.extend_from_slice(&device.color_y.to_le_bytes());
                        }
                        (0x0300, 0x0000) => {
                            frame.extend_from_slice(&[0x00, 0x20, device.hue]);
                        }
                        (0x0300, 0x0001) => {
                            frame.extend_from_slice(&[0x00, 0x20, device.saturation]);
                        }
                        _ => frame.push(0x86),
                    }
                }
                answers.push(self.incoming(device.nwk, 0x0104, sent.cluster, 1, &frame));
            } else if let Some(status) = device.refuse_commands {
                answers.push(self.incoming(
                    device.nwk,
                    0x0104,
                    sent.cluster,
                    1,
                    &[0x18, tsn, 0x0B, command, status],
                ));
            } else {
                let (on, level) = match (sent.cluster, command) {
                    (0x0300, _) => (device.relay_on, device.level),
                    (0x0006, 0x00) => (false, device.level),
                    (0x0006, 0x01) => (true, device.level),
                    (0x0006, _) => (!device.relay_on, device.level),
                    _ => {
                        let level = message.get(3).copied().unwrap_or(0);
                        (level > 0, level)
                    }
                };
                let (reports_on_off, reports_level) =
                    self.devices.get_mut(index).map_or((false, false), |d| {
                        d.relay_on = on;
                        d.level = level;
                        (
                            d.reports_attribute(0x0006, 0x0000),
                            matches!(d.kind, Kind::Dimmer | Kind::ColorLight)
                                && d.reports_attribute(0x0008, 0x0000),
                        )
                    });
                // The colour a colour command sets, applied whether the light is on or off.
                let mut color_report = None;
                if sent.cluster == 0x0300
                    && let Some(d) = self.devices.get_mut(index)
                {
                    let byte = |at: usize| message.get(at).copied().unwrap_or(0);
                    let word = |at: usize| u16::from_le_bytes([byte(at), byte(at + 1)]);
                    match command {
                        0x0A => {
                            d.color_temp = word(3);
                            if d.reports_attribute(0x0300, 0x0007) {
                                color_report = Some(color_temp_report(d.next_tsn(), d.color_temp));
                            }
                        }
                        0x07 => {
                            d.color_x = word(3);
                            d.color_y = word(5);
                            if d.reports_attribute(0x0300, 0x0003)
                                && d.reports_attribute(0x0300, 0x0004)
                            {
                                color_report =
                                    Some(color_xy_report(d.next_tsn(), d.color_x, d.color_y));
                            }
                        }
                        _ => {
                            d.hue = byte(3);
                            d.saturation = byte(4);
                        }
                    }
                }
                self.dirty = true;
                // The command asks for a Default Response unless it says "disable default
                // response".
                if control & 0x10 == 0 {
                    answers.push(self.incoming(
                        device.nwk,
                        0x0104,
                        sent.cluster,
                        1,
                        &[0x18, tsn, 0x0B, command, 0x00],
                    ));
                }
                if reports_on_off && sent.cluster != 0x0300 {
                    let report_tsn = self.devices.get_mut(index).map_or(0, SimDevice::next_tsn);
                    answers.push(self.incoming(
                        device.nwk,
                        0x0104,
                        0x0006,
                        1,
                        &on_off_report(report_tsn, on),
                    ));
                }
                if let Some(report) = color_report {
                    answers.push(self.incoming(device.nwk, 0x0104, 0x0300, 1, &report));
                }
                if reports_level && sent.cluster == 0x0008 {
                    let report_tsn = self.devices.get_mut(index).map_or(0, SimDevice::next_tsn);
                    answers.push(self.incoming(
                        device.nwk,
                        0x0104,
                        0x0008,
                        1,
                        &level_report(report_tsn, level),
                    ));
                }
            }
        } else if sent.profile == 0x0104
            && device.input_clusters.contains(&sent.cluster)
            && message.get(2) == Some(&0x06)
        {
            if let Some(device) = self.devices.get_mut(index)
                && device.ignore_configures > 0
            {
                if device.ignore_configures != u32::MAX {
                    device.ignore_configures -= 1;
                }
                reply.after.push(message_sent);
                return;
            }
            let Some(tsn) = message.get(1).copied() else {
                reply.after.push(message_sent);
                return;
            };
            let attributes = configured_attributes(message.get(3..).unwrap_or(&[]));
            let mode = device
                .configure_overrides
                .iter()
                .find(|(cluster, _)| *cluster == sent.cluster)
                .map_or(device.configure, |(_, mode)| *mode);
            let response = |body: &[u8]| {
                let mut frame = vec![0x18, tsn];
                frame.extend_from_slice(body);
                frame
            };
            let frame = match mode {
                ConfigureAnswer::Silent => None,
                ConfigureAnswer::Single => Some(response(&[0x07, 0x00])),
                ConfigureAnswer::SingleStatus(status) => Some(response(&[0x07, status])),
                ConfigureAnswer::RefuseAttribute(refused, status) => {
                    let mut body = vec![0x07];
                    for attribute in &attributes {
                        let status = if *attribute == refused { status } else { 0x00 };
                        body.extend_from_slice(&[status, 0x00]);
                        body.extend_from_slice(&attribute.to_le_bytes());
                    }
                    Some(response(&body))
                }
                ConfigureAnswer::PerAttribute | ConfigureAnswer::Refuse(_) => {
                    let status = match mode {
                        ConfigureAnswer::Refuse(status) => status,
                        _ => 0x00,
                    };
                    let mut body = vec![0x07];
                    for attribute in &attributes {
                        body.extend_from_slice(&[status, 0x00]);
                        body.extend_from_slice(&attribute.to_le_bytes());
                    }
                    Some(response(&body))
                }
                ConfigureAnswer::Default(status) => Some(response(&[0x0B, 0x06, status])),
            };
            if let Some(frame) = frame {
                answers.push(self.incoming(device.nwk, 0x0104, sent.cluster, 1, &frame));
                // The attributes the device accepted are the ones it will report.
                let accepted: Vec<u16> = match mode {
                    ConfigureAnswer::Refuse(_)
                    | ConfigureAnswer::Default(_)
                    | ConfigureAnswer::SingleStatus(1..=255) => Vec::new(),
                    ConfigureAnswer::RefuseAttribute(refused, _) => attributes
                        .iter()
                        .copied()
                        .filter(|a| *a != refused)
                        .collect(),
                    _ => attributes.clone(),
                };
                let all_done = self.devices.get_mut(index).is_some_and(|d| {
                    // At most one entry per cluster, so at most the device's clusters.
                    if !d.configured.contains(&sent.cluster) {
                        d.configured.push(sent.cluster);
                    }
                    for attribute in accepted {
                        if !d.configured_attributes.contains(&(sent.cluster, attribute)) {
                            d.configured_attributes.push((sent.cluster, attribute));
                        }
                    }
                    let wanted = d.input_clusters.iter().filter(|c| REPORTABLE.contains(c));
                    let done = wanted.clone().all(|c| d.configured.contains(c));
                    let first = done && !d.reports_sent;
                    d.reports_sent |= done;
                    first
                });
                self.dirty = true;
                if all_done {
                    for (cluster, report) in &device.reports {
                        answers.push(self.incoming(device.nwk, 0x0104, *cluster, 1, report));
                    }
                }
            }
        } else {
            reply.after.push(message_sent);
            return;
        }
        if !device.message_sent_last {
            self.emit(reply, delay, message_sent.clone());
        }
        for answer in answers {
            self.emit(reply, delay, answer);
        }
        if device.message_sent_last {
            self.emit(reply, delay, message_sent);
        }
    }

    fn respond(&mut self, command: &[u8]) -> Reply {
        let mut reply = Reply::default();
        if command.len() < 3 {
            return reply;
        }
        let seq = command[0];
        let extended = command.len() >= 5 && command[2] == 0x01;
        let id = if extended {
            u16::from_le_bytes([command[3], command[4]])
        } else {
            u16::from(command[2])
        };
        let args: &[u8] = if extended {
            &command[5..]
        } else {
            &command[3..]
        };
        if id != 0x0000 && self.record {
            keep(&mut self.commands, (id, args.to_vec()));
        }
        let ok = self.st(SL_OK);
        let params: Vec<u8> = match id {
            0x0000 => {
                let v = self.stack_version.to_le_bytes();
                vec![self.protocol, self.stack_type, v[0], v[1]]
            }
            0x0018 => vec![self.network_state],
            0x0026 if self.eui64_fails => Vec::new(),
            0x0026 => self.eui64_wire.to_vec(),
            // setConfigurationValue, setPolicy, setValue, clearKeyTable, setConcentrator
            0x10 if self.refuse_concentrator => self.st(SL_FAIL),
            0x53 | 0x55 | 0xAB | 0xB1 | 0x10 => ok,
            0x03 => {
                // getExtendedValue(endpoint flags): not registered, or registered (2 flag bytes).
                if self.endpoint_registered {
                    let mut p = ok;
                    p.extend_from_slice(&[2, 0, 0]);
                    p
                } else {
                    self.st(SL_FAIL)
                }
            }
            0x02 => {
                self.endpoint_registered = true;
                ok
            }
            0x17 => {
                if self.network.is_some() {
                    self.network_up(&mut reply);
                    self.resume_reports(&mut reply);
                    ok
                } else {
                    self.st(SL_NOT_JOINED)
                }
            }
            // leaveNetwork: the radio forgets its network and says "network down".
            0x20 => {
                self.network = None;
                self.network_state = 0;
                self.dirty = true;
                let down = self.stack_status(false);
                reply.after.push(down);
                ok
            }
            0x68 => {
                if args.len() == 43 {
                    let mut tc = [0u8; 16];
                    let mut nk = [0u8; 16];
                    tc.copy_from_slice(&args[2..18]);
                    nk.copy_from_slice(&args[18..34]);
                    self.keys = Some((nk, tc));
                    self.dirty = true;
                }
                ok
            }
            0x1E => {
                if args.len() == 20 {
                    let mut ext = [0u8; 8];
                    ext.copy_from_slice(&args[..8]);
                    self.network = Some(StoredNetwork {
                        extended_pan_wire: ext,
                        pan_id: u16::from_le_bytes([args[8], args[9]]),
                        channel: args[11],
                        node_type: 1,
                    });
                    self.dirty = true;
                }
                self.network_up(&mut reply);
                ok
            }
            0x28 => match self.network.clone() {
                Some(net) => {
                    let mut p = ok;
                    p.push(net.node_type);
                    p.extend_from_slice(&net.extended_pan_wire);
                    p.extend_from_slice(&net.pan_id.to_le_bytes());
                    p.extend_from_slice(&[5, net.channel, 0, 0, 0, 0]);
                    p.extend_from_slice(&0x07FF_F800u32.to_le_bytes());
                    p
                }
                None => self.st(SL_NOT_JOINED),
            },
            // lookupNodeIdByEui64: the node address, or 0xFFFF; no status.
            0x60 => {
                let Some(wire) = args.get(..8) else {
                    return reply;
                };
                let hidden = self
                    .devices
                    .iter()
                    .any(|d| d.lookup_unknown && d.eui_wire[..] == *wire);
                let joined = self
                    .devices
                    .iter()
                    .filter(|d| !hidden && d.joined && d.eui_wire[..] == *wire)
                    .map(|d| d.nwk);
                let known = self
                    .known_addresses
                    .iter()
                    .filter(|(_, eui)| !hidden && eui[..] == *wire)
                    .map(|(n, _)| *n);
                known
                    .chain(joined)
                    .next()
                    .unwrap_or(0xFFFF)
                    .to_le_bytes()
                    .to_vec()
            }
            // lookupEui64ByNodeId
            0x61 => {
                let Some(nwk) = u16_at(args, 0) else {
                    return reply;
                };
                // The radio's address table has what was added, and the devices that joined it.
                let joined = self
                    .devices
                    .iter()
                    .filter(|d| d.joined && d.nwk == nwk)
                    .map(|d| (d.nwk, d.eui_wire))
                    .collect::<Vec<_>>();
                let known = self.known_addresses.iter().copied().chain(joined);
                match known.into_iter().find(|(n, _)| *n == nwk) {
                    Some((_, eui)) => {
                        let mut p = ok;
                        p.extend_from_slice(&eui);
                        p
                    }
                    None => self.st(SL_FAIL),
                }
            }
            0x5A => 0u32.to_le_bytes().to_vec(),
            // importTransientKey answers a 32-bit status on every version.
            0x111 => 0u32.to_le_bytes().to_vec(),
            0x6B => Vec::new(),
            0x22 if self.refuse_permit_joining && args.first().is_some_and(|s| *s > 0) => {
                self.st(SL_FAIL)
            }
            0x22 => {
                match args.first() {
                    Some(seconds) if *seconds > 0 => {
                        self.on_permit_joining(&mut reply, *seconds);
                        if self.join_before_response {
                            reply.before = std::mem::take(&mut reply.after);
                        }
                    }
                    _ => self.pairing_until = None,
                }
                ok
            }
            0x34 => {
                let wide = self.wide();
                let tag_len = if wide { 2 } else { 1 };
                if args.len() < 14 + tag_len + 1 {
                    return reply;
                }
                let tag = if wide {
                    u16::from_le_bytes([args[14], args[15]])
                } else {
                    u16::from(args[14])
                };
                let length = usize::from(args[14 + tag_len]);
                let Some(message) = args.get(15 + tag_len..15 + tag_len + length) else {
                    return reply;
                };
                let sent = SentMessage {
                    destination: u16::from_le_bytes([args[1], args[2]]),
                    profile: u16::from_le_bytes([args[3], args[4]]),
                    cluster: u16::from_le_bytes([args[5], args[6]]),
                    source_endpoint: args[7],
                    destination_endpoint: args[8],
                    tag,
                    message: message.to_vec(),
                };
                if self.record {
                    if self.sent.len() < MAX_RECORDS {
                        self.sent_at.push(self.now);
                    }
                    keep(&mut self.sent, sent.clone());
                }
                let status = if self.busy_sends > 0 {
                    self.busy_sends -= 1;
                    SL_BUSY
                } else {
                    SL_OK
                };
                let mut p = self.st(status);
                p.push(1); // APS sequence
                let aps_bytes = args[3..14].to_vec();
                self.device_answers(&mut reply, &sent, &aps_bytes, status);
                p
            }
            _ => return reply,
        };
        if self.stale_response_first && extended {
            let mut stale = vec![seq.wrapping_sub(1), 0x80, 0x01];
            stale.extend_from_slice(&id.to_le_bytes());
            stale.extend(self.st(SL_OK));
            reply.before.push(stale);
        }
        let mut frame = vec![seq, if self.overflow_flag { 0x81 } else { 0x80 }];
        if extended {
            frame.push(0x01);
            frame.extend_from_slice(&id.to_le_bytes());
        } else {
            frame.push(id as u8);
        }
        frame.extend(params);
        reply.response = Some(frame);
        reply
    }
}

/// The attribute ids of the records of a Configure Reporting request (after its header).
fn configured_attributes(mut records: &[u8]) -> Vec<u16> {
    let mut attributes = Vec::new();
    // At most a handful of records fit in a frame: the loop is bounded by the bytes.
    while records.len() >= 6 {
        attributes.push(u16::from_le_bytes([records[1], records[2]]));
        let change = match records[3] {
            0x20 => 1,
            0x21 | 0x29 => 2,
            _ => 0,
        };
        records = records.get(8 + change..).unwrap_or(&[]);
    }
    attributes
}

fn rstack() -> Vec<u8> {
    encode(&AshFrame::RstAck {
        version: 2,
        code: 0x0B, // software reset: the answer to our RST
    })
}

#[cfg(test)]
mod tests;
