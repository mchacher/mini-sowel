//! Orders (spec 011): an equipment's order alias turned into commands for its Zigbee devices,
//! the queue that carries them from whoever asks (the API, or the core itself) to the radio
//! loop, and the runner that sends them there. No I/O of its own: the radio loop gives the radio,
//! the time comes from the caller.

mod queue;
mod resolve;
mod runner;

pub use queue::{
    Busy, Clock, DEADLINE_MS, MAX_ORDERS, MAX_UNATTENDED, OrderQueue, OrderReply, OrderResult,
    OrderWait, Queued,
};
pub use resolve::{commands_for, prepare};
pub use runner::OrderRunner;

use crate::devices::Id;
use crate::radio::zcl;

/// One command for one device of an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCommand {
    /// The device, most significant byte first.
    pub eui64: [u8; 8],
    /// The node address its record has; `None` when the record has none.
    pub nwk: Option<u16>,
    /// The endpoint the command goes to.
    pub endpoint: u8,
    pub command: zcl::Command,
}

/// An order, validated and resolved, ready for the radio: the commands of the devices bound under
/// its alias (at most the bindings of an equipment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRequest {
    pub equipment: Id,
    pub alias: String,
    pub commands: Vec<DeviceCommand>,
}
