//! Shared by the tests of the simulator: a radio of the core on a simulated radio running a
//! scenario, with a fake clock, and a scratch folder.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mini_sowel_core::radio::{
    MeasureEvent, Network, NetworkSettings, PairingEvent, Radio, RadioError,
};
use mini_sowel_sim::{FakeClock, Ncp, Scenario, SimTransport};

pub type SimRadio = Radio<SimTransport<FakeClock>, Box<dyn FnMut() -> u64>>;

pub struct Sim {
    pub radio: SimRadio,
    pub ncp: Rc<RefCell<Ncp>>,
    pub clock: Rc<Cell<u64>>,
}

/// A scenario given as text.
pub fn scenario(json: &str) -> Scenario {
    Scenario::parse(json, "test.json").expect("the scenario parses")
}

/// A radio on the scenario, with the radio's memory in `memory` if given.
pub fn sim(json: &str, memory: Option<&Path>, speed: u32) -> Sim {
    let scenario = scenario(json);
    let ncp = Rc::new(RefCell::new(
        Ncp::from_scenario(&scenario, memory).expect("the simulated radio opens"),
    ));
    ncp.borrow_mut().record = true;
    let clock = FakeClock::default();
    let time = Rc::clone(&clock.time);
    let transport = SimTransport::new(Rc::clone(&ncp), scenario.events.clone(), speed, clock);
    let reader = Rc::clone(&time);
    Sim {
        radio: Radio::new(transport, Box::new(move || reader.get())),
        ncp,
        clock: time,
    }
}

/// Fills buffers with values that differ from call to call.
pub fn random() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut calls = 0u8;
    move |buffer: &mut [u8]| {
        calls = calls.wrapping_add(1);
        for (i, byte) in buffer.iter_mut().enumerate() {
            *byte = calls.wrapping_mul(31).wrapping_add(i as u8).wrapping_add(1);
        }
        Ok(())
    }
}

impl Sim {
    /// Identifies the radio and starts its network.
    pub fn start(&mut self) -> Result<Network, RadioError> {
        self.radio.identify()?;
        self.radio
            .start_network(&NetworkSettings::default(), &mut random(), &mut |_| Ok(()))
    }

    /// Resumes the network without forming one.
    pub fn listen(&mut self) -> Result<Network, RadioError> {
        self.radio.identify()?;
        self.radio.listen()
    }

    /// Polls for `ms` of fake time and returns the events.
    pub fn poll_for(&mut self, ms: u64) -> Vec<PairingEvent> {
        let end = self.clock.get() + ms;
        let mut events = Vec::new();
        while self.clock.get() < end {
            events.extend(self.radio.poll_pairing().expect("the radio answers"));
        }
        events
    }

    /// Opens pairing for `seconds`, polls for that long, closes it, and lets the setups in
    /// progress finish.
    pub fn pair(&mut self, seconds: u8) -> Vec<PairingEvent> {
        self.radio.permit_join(seconds).expect("pairing opens");
        let mut events = self.poll_for(u64::from(seconds) * 1000);
        self.radio.close_joining().expect("pairing closes");
        let end = self.clock.get() + 60_000;
        while self.radio.pairing_busy() && self.clock.get() < end {
            events.extend(self.radio.poll_pairing().expect("the radio answers"));
        }
        events
    }
}

pub fn measures(events: &[PairingEvent]) -> Vec<MeasureEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::Measures(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// The lines of the measures, as the host prints them.
pub fn measure_lines(events: &[PairingEvent]) -> Vec<String> {
    measures(events)
        .iter()
        .flat_map(MeasureEvent::lines)
        .collect()
}

pub fn joined(events: &[PairingEvent]) -> Vec<(String, u16)> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::Joined { eui64, nwk } => {
                Some((eui64.iter().map(|b| format!("{b:02X}")).collect(), *nwk))
            }
            _ => None,
        })
        .collect()
}

pub fn identified(events: &[PairingEvent]) -> Vec<(Option<String>, Option<String>)> {
    events
        .iter()
        .filter_map(|e| match e {
            PairingEvent::Identified {
                manufacturer,
                model,
                ..
            } => Some((manufacturer.clone(), model.clone())),
            _ => None,
        })
        .collect()
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh scratch folder name, not created.
pub fn scratch() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mini-sowel-sim-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

/// One SNZB-02 that joins at 2 s and reports at 40 s (the example of the scenario format).
pub const ONE_SENSOR: &str = r#"{
  "radio": { "ezsp": 13, "stack": "7.4.4.0", "eui64": "8C6FB9FFFE123456", "network": "none" },
  "devices": [
    { "id": "th", "kind": "temperature-humidity", "eui64": "00124B0012345678" }
  ],
  "events": [
    { "at": "2s", "device": "th", "do": "join" },
    { "at": "40s", "device": "th", "do": "report", "temperature": 19.52, "humidity": 54.1 },
    { "at": "1m30s", "device": "th", "do": "report", "battery": 100 }
  ]
}"#;
