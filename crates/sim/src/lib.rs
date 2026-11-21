//! The simulated radio of Solite: an EmberZNet NCP speaking real ASH and EZSP, simulated
//! Zigbee devices behind it, and scenario files that say what happens when. It runs on the Mac
//! and, for the image QEMU runs, in the ESP32 firmware (feature `sim`; decision D26): the host
//! program and the firmware drive it through `SimTransport`, the tests of
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
    Attr, CieAnswer, ConfigureAnswer, DeviceReport, Kind, SimDevice, battery_report,
    humidity_report, level_report, on_off_report, report_frame, report_frame_with_control,
    temperature_report, zone_notification, zone_status_notification,
};
pub use harness::{Harness, harness};
pub use ncp::{MAX_DEVICES, MAX_RECORDS, Mode, Ncp, SentMessage, StoredNetwork};
pub use scenario::{Action, Event, NetworkInit, Scenario};
pub use transport::SimTransport;
