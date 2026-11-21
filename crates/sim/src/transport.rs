//! The simulated radio as the core's byte pipe: what the host reads and writes instead of a
//! TCP connection or a serial port.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;

use solite_core::radio::{RadioTransport, TransportError};

use crate::clock::{Clock, ScenarioClock};
use crate::ncp::Ncp;
use crate::scenario::{Action, Event, Scenario};

/// The simulated radio and, optionally, the scenario that plays on it.
pub struct SimTransport<C: Clock> {
    ncp: Rc<RefCell<Ncp>>,
    clock: C,
    time: ScenarioClock,
    events: Vec<Event>,
    next_event: usize,
    /// Devices whose `join` came while pairing was closed: they join when it opens.
    waiting_joins: Vec<usize>,
    /// The clock time the scenario started at: when the host connected.
    start_ms: u64,
    to_read: VecDeque<u8>,
}

fn busy() -> TransportError {
    TransportError::Io("the simulated radio is in use elsewhere".to_string())
}

impl<C: Clock> SimTransport<C> {
    /// A transport on a radio the caller keeps a handle on, playing `events` (sorted by time) at
    /// `speed` times the clock's speed.
    pub fn new(ncp: Rc<RefCell<Ncp>>, events: Vec<Event>, speed: u32, clock: C) -> Self {
        let start_ms = clock.now_ms();
        if let Ok(mut radio) = ncp.try_borrow_mut() {
            radio.speed = speed.max(1);
        }
        SimTransport {
            ncp,
            clock,
            time: ScenarioClock {
                speed: speed.max(1),
            },
            events,
            next_event: 0,
            waiting_joins: Vec::new(),
            start_ms,
            to_read: VecDeque::new(),
        }
    }

    /// The radio and the events of a scenario, with the radio's memory in `memory_file`.
    pub fn for_scenario(
        scenario: &Scenario,
        memory_file: Option<&Path>,
        speed: u32,
        clock: C,
    ) -> Result<Self, String> {
        let ncp = Ncp::from_scenario(scenario, memory_file)?;
        Ok(SimTransport::new(
            Rc::new(RefCell::new(ncp)),
            scenario.events.clone(),
            speed,
            clock,
        ))
    }

    /// Plays what is due: the scenario's events, the joins that waited for pairing, the radio's
    /// timed callbacks. The answers go to the bytes waiting to be read.
    fn tick(&mut self) -> Result<(), TransportError> {
        let now = self.clock.now_ms();
        let scenario_ms = self.time.scenario_ms(now.saturating_sub(self.start_ms));
        let mut ncp = self.ncp.try_borrow_mut().map_err(|_| busy())?;
        ncp.set_now(now);
        while let Some(event) = self.events.get(self.next_event)
            && event.at_ms <= scenario_ms
        {
            self.next_event += 1;
            match &event.action {
                Action::Join => {
                    if !ncp.join_device(event.device) && !self.waiting_joins.contains(&event.device)
                    {
                        self.waiting_joins.push(event.device);
                    }
                }
                Action::Report(report) => {
                    ncp.device_report(event.device, report);
                }
            }
        }
        self.waiting_joins
            .retain(|device| !ncp.join_device(*device));
        self.to_read.extend(ncp.on_tick(now));
        Ok(())
    }

    /// Clock milliseconds until the next scenario event or timed callback is due.
    fn until_next_due(&self) -> Option<u64> {
        let now = self.clock.now_ms();
        let callback = self
            .ncp
            .try_borrow()
            .ok()
            .and_then(|ncp| ncp.next_due())
            .map(|at| at.saturating_sub(now));
        let event = self
            .events
            .get(self.next_event)
            .map(|event| (self.start_ms + self.time.clock_ms(event.at_ms)).saturating_sub(now));
        match (callback, event) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

impl<C: Clock> RadioTransport for SimTransport<C> {
    /// Waits for the shorter of `timeout_ms` and the time to the next due event, then returns
    /// what the radio sent (`Ok(0)` when nothing).
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        if self.to_read.is_empty() {
            self.tick()?;
        }
        if self.to_read.is_empty() {
            let wait = self
                .until_next_due()
                .map_or(timeout_ms, |due| due.min(timeout_ms));
            self.clock.wait_ms(wait.max(1));
            self.tick()?;
        }
        let n = buf.len().min(self.to_read.len());
        for (slot, byte) in buf.iter_mut().zip(self.to_read.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let answer = self
            .ncp
            .try_borrow_mut()
            .map_err(|_| busy())?
            .on_bytes(bytes);
        self.to_read.extend(answer);
        Ok(())
    }
}
