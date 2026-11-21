//! The devices of mini-Sowel, in Sowel's format (spec 008): the model with Sowel's names, the
//! catalog that says which data and orders a cluster gives, the registry that keeps them up to
//! date from the radio's events, and the rules for writing them to storage.

mod catalog;
mod model;
mod persist;
mod registry;
mod time;

pub use catalog::{DataSpec, OrderSpec, data_for_cluster, key_for_measure, orders_for_cluster};
pub use model::{
    Device, DeviceData, DeviceOrder, DeviceStatus, DeviceWithDetails, INTEGRATION_ID, Id,
    PowerSource, ValueType,
};
pub use persist::{DeviceStore, VALUE_WRITE_INTERVAL_MS, WritePolicy, lock, parse_list, to_json};
pub use registry::{Devices, MAX_DATA, MAX_DEVICES, MAX_NAME_CHARS, MAX_ORDERS, Random};
pub use time::{format_iso, parse_iso};
