//! A scripted radio (NCP) for the tests: bytes in, bytes out, speaking real ASH and EZSP.
//! Shared by the core's and the host's tests.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mini_sowel_core::radio::ash::{AshFrame, DecodeEvent, Decoder, encode};
use mini_sowel_core::radio::{Radio, RadioTransport, TransportError};

/// How the fake radio behaves after the reset.
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

/// A character attribute of the simulated device.
#[derive(Clone, Debug)]
pub enum Attr {
    /// The bytes of the string as the device sends them.
    Text(Vec<u8>),
    /// The device answers "unsupported attribute" (status 0x86).
    Unsupported,
}

/// A simulated Zigbee device: it joins when pairing opens, answers the endpoint request and the
/// Basic read.
#[derive(Clone, Debug)]
pub struct FakeDevice {
    /// As on the wire (little-endian).
    pub eui_wire: [u8; 8],
    pub nwk: u16,
    /// Time between the opening of pairing and the join, in milliseconds.
    pub join_delay_ms: u64,
    /// `EmberDeviceUpdate` of the trust-centre join, and its `EmberJoinDecision`.
    pub join_status: u8,
    pub join_decision: u8,
    /// Sends a `Device_annce` just after the join; twice if `announce_twice`.
    pub announce: bool,
    pub announce_twice: bool,
    /// The endpoints it lists; `None`: it never answers the endpoint request.
    pub endpoints: Option<Vec<u8>>,
    /// Status of its `Active_EP_rsp`.
    pub endpoint_status: u8,
    pub manufacturer: Attr,
    pub model: Attr,
    /// Number of Read Attributes requests it ignores before it answers; `u32::MAX`: never.
    pub ignore_reads: u32,
    /// Sends wrong answers (other device, other transaction number, Default Response) before
    /// each right one.
    pub decoys: bool,
    /// Time between a request and its answer, in milliseconds.
    pub answer_delay_ms: u64,
    /// Sends `messageSentHandler` after the answer instead of before.
    pub message_sent_last: bool,
    joined: bool,
}

impl FakeDevice {
    /// An original SNZB-02: `eWeLink` / `TH01`, on endpoint 1, announcing itself.
    pub fn sensor() -> Self {
        FakeDevice {
            eui_wire: [0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00],
            nwk: 0x5C3D,
            join_delay_ms: 0,
            join_status: 1,
            join_decision: 0,
            announce: true,
            announce_twice: false,
            endpoints: Some(vec![1]),
            endpoint_status: 0,
            manufacturer: Attr::Text(b"eWeLink".to_vec()),
            model: Attr::Text(b"TH01".to_vec()),
            ignore_reads: 0,
            decoys: false,
            answer_delay_ms: 0,
            message_sent_last: false,
            joined: false,
        }
    }
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

pub struct FakeNcp {
    pub mode: Mode,
    pub protocol: u8,
    pub stack_type: u8,
    pub stack_version: u16,
    /// The address as the radio sends it (little-endian).
    pub eui64_wire: [u8; 8],
    pub network_state: u8,
    /// Send an asynchronous callback frame before every response.
    pub callback_first: bool,
    /// Every byte received, in order.
    pub received: Vec<u8>,
    /// Every EZSP command received: frame id and parameters.
    pub commands: Vec<(u16, Vec<u8>)>,
    /// Every `sendUnicast` received.
    pub sent: Vec<SentMessage>,
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
    /// Every response is preceded by a late response to the previous sequence number.
    pub stale_response_first: bool,
    /// Callbacks of kinds the host does not use, sent when pairing opens.
    pub unused_burst: usize,
    /// Responses carry the overflow flag.
    pub overflow_flag: bool,
    /// The next `sendUnicast` commands answered "busy".
    pub busy_sends: u32,
    /// The simulated device.
    pub device: Option<FakeDevice>,
    /// The join callbacks of a pairing come before the response of `permitJoining`.
    pub join_before_response: bool,
    /// Devices that only join (trust-centre join, no answers), after the main device.
    pub crowd: usize,
    /// Milliseconds between two joins of the crowd.
    pub crowd_spread_ms: u64,
    decoder: Decoder,
    tx_frm: u8,
    rx_next: u8,
    now: u64,
    scheduled: Vec<(u64, Vec<u8>)>,
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

impl FakeNcp {
    pub fn new() -> Self {
        FakeNcp {
            mode: Mode::Normal,
            protocol: 13,
            stack_type: 2,
            stack_version: 0x7440,
            eui64_wire: [0x56, 0x34, 0x12, 0xFE, 0xFF, 0xB9, 0x6F, 0x8C],
            network_state: 0,
            callback_first: false,
            received: Vec::new(),
            commands: Vec::new(),
            sent: Vec::new(),
            network: None,
            keys: None,
            endpoint_registered: false,
            network_up_before_response: false,
            no_network_up: false,
            network_up_delay_ms: 0,
            refuse_concentrator: false,
            stale_response_first: false,
            unused_burst: 0,
            overflow_flag: false,
            busy_sends: 0,
            device: None,
            crowd: 0,
            crowd_spread_ms: 50,
            join_before_response: false,
            decoder: Decoder::new(),
            tx_frm: 0,
            rx_next: 0,
            now: 0,
            scheduled: Vec::new(),
        }
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
        out
    }

    /// Feeds the bytes the host wrote; returns what the radio answers.
    pub fn on_bytes(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.received.extend_from_slice(bytes);
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
            params.extend_from_slice(&[0; 8]); // long id unknown
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

    /// Pairing opens: the device (and the crowd) join.
    fn on_permit_joining(&mut self, reply: &mut Reply) {
        for i in 0..self.unused_burst {
            // childJoinHandler, then incomingRouteRecordHandler: callbacks the host ignores.
            let burst = if i % 2 == 0 {
                callback(0x0023, &[0, 1, 0x34, 0x12, 1, 2, 3, 4, 5, 6, 7, 8, 2])
            } else {
                callback(0x0059, &[0x34, 0x12, 1, 2, 3, 4, 5, 6, 7, 8, 0xC8, 0xD0, 0])
            };
            reply.after.push(burst);
        }
        let Some(device) = self.device.as_mut() else {
            return;
        };
        if device.joined {
            return;
        }
        device.joined = true;
        let device = device.clone();
        let delay = device.join_delay_ms;
        let join = self.trust_center_join(
            device.nwk,
            device.eui_wire,
            device.join_status,
            device.join_decision,
        );
        self.emit(reply, delay, join);
        if device.announce {
            let mut payload = vec![0x01];
            payload.extend_from_slice(&device.nwk.to_le_bytes());
            payload.extend_from_slice(&device.eui_wire);
            payload.push(0x80);
            let announce = self.incoming(device.nwk, 0x0000, 0x0013, 0, &payload);
            self.emit(reply, delay, announce.clone());
            if device.announce_twice {
                self.emit(reply, delay + 100, announce);
            }
        }
        for i in 0..self.crowd {
            let mut eui = [0xA0, 0, 0, 0, 0, 0x4B, 0x12, 0x00];
            eui[1] = i as u8;
            let join = self.trust_center_join(0x1000 + i as u16, eui, 1, 0);
            // Spread over time, as a real burst of joins is; or all at once with a spread of 0.
            self.emit(reply, delay + self.crowd_spread_ms * (i as u64 + 1), join);
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
        let Some(device) = self.device.clone() else {
            return;
        };
        if !device.joined || sent.destination != device.nwk {
            return;
        }
        let delay = device.answer_delay_ms;
        let message_sent = self.message_sent(sent.destination, aps_bytes, sent.tag, status);
        if status != SL_OK {
            reply.after.push(message_sent);
            return;
        }
        let mut answers = Vec::new();
        if sent.profile == 0x0000 && sent.cluster == 0x0005 {
            let Some(endpoints) = device.endpoints.clone() else {
                reply.after.push(message_sent);
                return;
            };
            let tsn = sent.message[0];
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
        } else if sent.profile == 0x0104
            && sent.cluster == 0x0000
            && sent.message.get(2) == Some(&0x00)
        {
            if let Some(device) = self.device.as_mut()
                && device.ignore_reads > 0
            {
                if device.ignore_reads != u32::MAX {
                    device.ignore_reads -= 1;
                }
                reply.after.push(message_sent);
                return;
            }
            let tsn = sent.message[1];
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
        if id != 0x0000 {
            self.commands.push((id, args.to_vec()));
        }
        let ok = self.st(SL_OK);
        let params: Vec<u8> = match id {
            0x0000 => {
                let v = self.stack_version.to_le_bytes();
                vec![self.protocol, self.stack_type, v[0], v[1]]
            }
            0x0018 => vec![self.network_state],
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
                    ok
                } else {
                    self.st(SL_NOT_JOINED)
                }
            }
            0x68 => {
                if args.len() == 43 {
                    let mut tc = [0u8; 16];
                    let mut nk = [0u8; 16];
                    tc.copy_from_slice(&args[2..18]);
                    nk.copy_from_slice(&args[18..34]);
                    self.keys = Some((nk, tc));
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
            0x5A => 0u32.to_le_bytes().to_vec(),
            // importTransientKey answers a 32-bit status on every version.
            0x111 => 0u32.to_le_bytes().to_vec(),
            0x6B => Vec::new(),
            0x22 => {
                if args.first().is_some_and(|seconds| *seconds > 0) {
                    self.on_permit_joining(&mut reply);
                    if self.join_before_response {
                        reply.before = std::mem::take(&mut reply.after);
                    }
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
                let message = args[15 + tag_len..15 + tag_len + length].to_vec();
                let sent = SentMessage {
                    destination: u16::from_le_bytes([args[1], args[2]]),
                    profile: u16::from_le_bytes([args[3], args[4]]),
                    cluster: u16::from_le_bytes([args[5], args[6]]),
                    source_endpoint: args[7],
                    destination_endpoint: args[8],
                    tag,
                    message,
                };
                self.sent.push(sent.clone());
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

fn rstack() -> Vec<u8> {
    encode(&AshFrame::RstAck {
        version: 2,
        code: 0x0B, // software reset: the answer to our RST
    })
}

/// A transport wired to a fake radio the test keeps hold of. When nothing is waiting, reading
/// lets the fake radio's timed callbacks come due, then advances the fake clock by the timeout,
/// so no test really waits.
pub struct SharedTransport {
    ncp: Rc<RefCell<FakeNcp>>,
    to_read: Vec<u8>,
    clock: Rc<Cell<u64>>,
}

impl RadioTransport for SharedTransport {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        if self.to_read.is_empty() {
            let due = self.ncp.borrow_mut().on_tick(self.clock.get());
            if due.is_empty() {
                self.clock.set(self.clock.get() + timeout_ms);
                return Ok(0);
            }
            self.to_read.extend(due);
        }
        let n = buf.len().min(self.to_read.len());
        buf[..n].copy_from_slice(&self.to_read[..n]);
        self.to_read.drain(..n);
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let answer = self.ncp.borrow_mut().on_bytes(bytes);
        self.to_read.extend(answer);
        Ok(())
    }
}

/// A radio on a fake radio, with the handles a test needs.
pub struct Harness {
    pub radio: Radio<SharedTransport, Box<dyn FnMut() -> u64>>,
    pub ncp: Rc<RefCell<FakeNcp>>,
    pub clock: Rc<Cell<u64>>,
}

pub fn harness(ncp: FakeNcp) -> Harness {
    let ncp = Rc::new(RefCell::new(ncp));
    let clock = Rc::new(Cell::new(0u64));
    let transport = SharedTransport {
        ncp: Rc::clone(&ncp),
        to_read: Vec::new(),
        clock: Rc::clone(&clock),
    };
    let reader = Rc::clone(&clock);
    Harness {
        radio: Radio::new(transport, Box::new(move || reader.get())),
        ncp,
        clock,
    }
}
