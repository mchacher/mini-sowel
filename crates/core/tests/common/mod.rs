//! A scripted radio (NCP) for the tests: bytes in, bytes out, speaking real ASH and EZSP.
//! Shared by the core's and the host's tests.

#![allow(dead_code)]

use mini_sowel_core::radio::ash::{AshFrame, DecodeEvent, Decoder, encode};

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
    decoder: Decoder,
    tx_frm: u8,
    rx_next: u8,
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
            decoder: Decoder::new(),
            tx_frm: 0,
            rx_next: 0,
        }
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
                    if let Some(response) = self.respond(&payload) {
                        out.extend(self.data(response));
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

    fn respond(&self, command: &[u8]) -> Option<Vec<u8>> {
        if command.len() < 3 {
            return None;
        }
        let seq = command[0];
        let extended = command.len() >= 5 && command[2] == 0x01;
        let id = if extended {
            u16::from_le_bytes([command[3], command[4]])
        } else {
            u16::from(command[2])
        };
        let params: Vec<u8> = match id {
            0x0000 => {
                let v = self.stack_version.to_le_bytes();
                vec![self.protocol, self.stack_type, v[0], v[1]]
            }
            0x0018 => vec![self.network_state],
            0x0026 => self.eui64_wire.to_vec(),
            _ => return None,
        };
        let mut frame = vec![seq, 0x80];
        if extended {
            frame.push(0x01);
            frame.extend_from_slice(&id.to_le_bytes());
        } else {
            frame.push(id as u8);
        }
        frame.extend(params);
        Some(frame)
    }
}

fn rstack() -> Vec<u8> {
    encode(&AshFrame::RstAck {
        version: 2,
        code: 0x0B, // software reset: the answer to our RST
    })
}
