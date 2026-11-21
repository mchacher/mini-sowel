//! A radio of the core wired to a simulated radio the test keeps hold of, with a fake clock.
//! When nothing is waiting, reading lets the radio's timed callbacks come due, then moves the
//! clock forward, so no test really waits.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use solite_core::radio::Radio;

use crate::clock::FakeClock;
use crate::ncp::Ncp;
use crate::transport::SimTransport;

/// A radio on a simulated radio, with the handles a test needs.
pub struct Harness {
    pub radio: Radio<SimTransport<FakeClock>, Box<dyn FnMut() -> u64>>,
    pub ncp: Rc<RefCell<Ncp>>,
    pub clock: Rc<Cell<u64>>,
}

pub fn harness(ncp: Ncp) -> Harness {
    let ncp = Rc::new(RefCell::new(ncp));
    let clock = FakeClock::default();
    let time = Rc::clone(&clock.time);
    let transport = SimTransport::new(Rc::clone(&ncp), Vec::new(), 1, clock);
    let reader = Rc::clone(&time);
    Harness {
        radio: Radio::new(transport, Box::new(move || reader.get())),
        ncp,
        clock: time,
    }
}
