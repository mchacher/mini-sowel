//! The simulated radio of mini-Sowel: an EmberZNet NCP speaking real ASH and EZSP, simulated
//! Zigbee devices behind it, and scenario files that say what happens when. It runs on the Mac
//! only (never on the ESP32): the host program drives it through `SimTransport`, the tests of
//! the core and the host through `Ncp` and `harness`.

pub mod clock;
pub mod devices;
pub mod harness;
mod memory;
pub mod ncp;
pub mod scenario;
pub mod transport;

pub use clock::{Clock, FakeClock, RealClock, ScenarioClock};
pub use devices::{
    Attr, ConfigureAnswer, DeviceReport, Kind, SimDevice, battery_report, humidity_report,
    on_off_report, report_frame, report_frame_with_control, temperature_report, zone_notification,
};
pub use harness::{Harness, harness};
pub use ncp::{MAX_DEVICES, MAX_RECORDS, Mode, Ncp, SentMessage, StoredNetwork};
pub use scenario::{Action, Event, NetworkInit, Scenario};
pub use transport::SimTransport;
