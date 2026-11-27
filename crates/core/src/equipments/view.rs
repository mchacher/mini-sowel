//! An equipment with its details (Sowel's `EquipmentWithDetails`, T:663): its bindings with the
//! value of the device data behind them (`DataBindingWithValue` T:583, `OrderBindingWithDetails`
//! T:613), and its status. Serialised straight from the devices, no copy.

use std::collections::HashMap;

use serde::{Serialize, Serializer};

use super::status::{self, EquipmentStatus, Reading, StatusReason};
use super::{DataBinding, Equipment, OrderBinding, StoredEquipment};
use crate::devices::{Device, DeviceData, DeviceOrder, Devices, Id, ValueType, iso_option};

/// The device and the data behind a binding (`None` when the data is gone: the binding is then
/// dropped by `Home`, so it is seen only between two steps of a change).
pub(crate) fn resolve<'a>(
    devices: &'a Devices,
    binding: &DataBinding,
) -> Option<(&'a Device, &'a DeviceData)> {
    devices.data_of(&binding.device_data_id)
}

/// Every data of every device by id, built once (at most 64 devices x 8 data), so that finding
/// the data behind a binding is a hash lookup, not a walk through all the devices.
pub(crate) struct Index<'a>(HashMap<Id, (&'a Device, &'a DeviceData)>);

impl<'a> Index<'a> {
    pub(crate) fn new(devices: &'a Devices) -> Self {
        Index(
            devices
                .list()
                .iter()
                .flat_map(|item| item.data.iter().map(move |d| (d.id, (&item.device, d))))
                .collect(),
        )
    }

    pub(crate) fn resolve(&self, binding: &DataBinding) -> Option<(&'a Device, &'a DeviceData)> {
        self.0.get(&binding.device_data_id).copied()
    }
}

/// The readings of an equipment's data bindings, as the status sees them.
fn readings<'a>(
    find: impl Fn(&DataBinding) -> Option<(&'a Device, &'a DeviceData)>,
    item: &'a StoredEquipment,
) -> Vec<Reading<'a>> {
    item.data_bindings
        .iter()
        .filter_map(|binding| {
            let (device, data) = find(binding)?;
            Some(Reading {
                device,
                alias: &binding.alias,
                category: &data.category,
                value_type: data.value_type,
                last_updated: data.last_updated,
            })
        })
        .collect()
}

/// The status of an equipment, and why when it is not `online`.
pub fn status_of(
    devices: &Devices,
    item: &StoredEquipment,
    now: u64,
) -> (EquipmentStatus, Option<StatusReason>) {
    status::derive(
        &readings(|binding| resolve(devices, binding), item),
        item.equipment.kind,
        now,
    )
}

/// The same with an index built by the caller (once for many equipments, as the aggregation
/// does; one equipment alone looks its few bindings up directly).
pub(crate) fn status_with(
    index: &Index<'_>,
    item: &StoredEquipment,
    now: u64,
) -> (EquipmentStatus, Option<StatusReason>) {
    status::derive(
        &readings(|binding| index.resolve(binding), item),
        item.equipment.kind,
        now,
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataValue<'a> {
    equipment_id: Id,
    #[serde(flatten)]
    binding: &'a DataBinding,
    device_id: Id,
    device_name: &'a str,
    key: &'a str,
    #[serde(rename = "type")]
    value_type: ValueType,
    category: &'a str,
    value: &'a serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'a str>,
    #[serde(with = "iso_option")]
    last_updated: Option<u64>,
    #[serde(with = "iso_option")]
    last_changed: Option<u64>,
    stale: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderValue<'a> {
    equipment_id: Id,
    #[serde(flatten)]
    binding: &'a OrderBinding,
    device_id: Id,
    device_name: &'a str,
    key: &'a str,
    #[serde(rename = "type")]
    value_type: ValueType,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<i64>,
}

struct DataList<'a> {
    devices: &'a Devices,
    item: &'a StoredEquipment,
    now: u64,
}

impl Serialize for DataList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.item.data_bindings.iter().filter_map(|binding| {
            let (device, data) = resolve(self.devices, binding)?;
            Some(DataValue {
                equipment_id: self.item.equipment.id,
                binding,
                device_id: device.id,
                device_name: &device.name,
                key: &data.key,
                value_type: data.value_type,
                category: &data.category,
                value: &data.value,
                unit: data.unit.as_deref(),
                last_updated: data.last_updated,
                last_changed: data.last_changed,
                stale: status::is_stale(
                    &data.category,
                    data.value_type,
                    data.last_updated,
                    self.now,
                    self.item.equipment.kind,
                ),
            })
        }))
    }
}

struct OrderList<'a> {
    devices: &'a Devices,
    item: &'a StoredEquipment,
}

impl Serialize for OrderList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.item.order_bindings.iter().filter_map(|binding| {
            let (device, order): (&Device, &DeviceOrder) =
                self.devices.order_of(&binding.device_order_id)?;
            Some(OrderValue {
                equipment_id: self.item.equipment.id,
                binding,
                device_id: device.id,
                device_name: &device.name,
                key: &order.key,
                value_type: order.value_type,
                category: order.category.as_deref(),
                min: order.min,
                max: order.max,
            })
        }))
    }
}

/// Sowel's `EquipmentWithDetails`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details<'a> {
    #[serde(flatten)]
    equipment: &'a Equipment,
    data_bindings: DataList<'a>,
    order_bindings: OrderList<'a>,
    status: EquipmentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_reason: Option<StatusReason>,
}

/// An equipment with its details, as of `now`.
pub fn details<'a>(devices: &'a Devices, item: &'a StoredEquipment, now: u64) -> Details<'a> {
    let (status, status_reason) = status_of(devices, item, now);
    Details {
        equipment: &item.equipment,
        data_bindings: DataList { devices, item, now },
        order_bindings: OrderList { devices, item },
        status,
        status_reason,
    }
}
