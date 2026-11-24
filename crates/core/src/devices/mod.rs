//! The devices of Solite, in Sowel's format (spec 008): the model with Sowel's names, the
//! catalog that says which data and orders a cluster gives, the registry that keeps them up to
//! date from the radio's events, and the rules for writing them to storage.

mod catalog;
mod model;
mod persist;
mod registry;
mod time;

pub use catalog::{
    DataSpec, OrderSpec, all_data, all_orders, data_for_cluster, key_for_measure,
    orders_for_cluster,
};
pub use model::{
    Device, DeviceData, DeviceOrder, DeviceStatus, DeviceWithDetails, INTEGRATION_ID, Id,
    PowerSource, Route, ValueType,
};
pub(crate) use model::{iso, iso_option};
pub use persist::{
    DeviceStore, VALUE_WRITE_INTERVAL_MS, WritePolicy, lock, parse_list, stored_item, to_json,
};
pub(crate) use registry::json_number;
pub use registry::{Devices, MAX_DATA, MAX_DEVICES, MAX_NAME_CHARS, MAX_ORDERS, Random};
pub(crate) use time::{civil_from_days, days_from_civil};
pub use time::{format_iso, parse_iso};
