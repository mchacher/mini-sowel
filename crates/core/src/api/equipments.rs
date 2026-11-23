//! `/equipments` and their bindings (Sowel's `src/api/routes/equipments.ts`). Unknown fields are
//! refused on the binding routes and ignored on `POST` and `PUT /equipments`: Sowel's interface
//! and `sowel-plugin-mini` may send Sowel's other flags.

use serde_json::Value;

use super::input::{self, Fields};
use super::{Request, Response, error, failure, json, method_not_allowed, no_content};
use crate::devices::{Id, MAX_DEVICES, Random};
use crate::equipments::{DataBindingOut, EquipmentType, OrderBindingOut, details};
use crate::failure::Outcome;
use crate::home::{EquipmentUpdate, Home, NewEquipment};

pub fn route(
    home: &mut Home,
    segments: &[&str],
    request: &Request,
    now: u64,
    random: Random,
) -> Response {
    match (segments, request.method) {
        (["equipments"], "POST") => create(home, request.body, now, random),
        (["equipments"], _) => method_not_allowed(),
        (["equipments", id], "GET") => match home.equipments().get(id) {
            Some(item) => json(200, &details(&home.devices, item, now)),
            None => error(404, "Equipment not found"),
        },
        (["equipments", id], "PUT") => update(home, id, request.body, now),
        (["equipments", id], "DELETE") => match home.delete_equipment(id, now) {
            Ok(()) => no_content(),
            Err(e) => failure(&e),
        },
        (["equipments", _], _) => method_not_allowed(),
        (["equipments", id, "data-bindings"], "POST") => add_data(home, id, request.body, random),
        (["equipments", id, "order-bindings"], "POST") => add_order(home, id, request.body, random),
        (["equipments", _, "data-bindings" | "order-bindings"], _) => method_not_allowed(),
        (["equipments", id, "data-bindings", binding], "DELETE") => {
            match home.remove_data_binding(id, binding, now) {
                Ok(()) => no_content(),
                Err(e) => failure(&e),
            }
        }
        (["equipments", id, "order-bindings", binding], "DELETE") => {
            match home.remove_order_binding(id, binding, now) {
                Ok(()) => no_content(),
                Err(e) => failure(&e),
            }
        }
        (["equipments", _, "data-bindings" | "order-bindings", _], _) => method_not_allowed(),
        _ => error(404, "Not found"),
    }
}

/// The `type` of a query string (`type=sensor`): `None` without one, `Some(None)` for a type that
/// does not exist (the list is then empty).
pub(crate) fn type_filter(query: &str) -> Option<Option<EquipmentType>> {
    // An empty `type=` is no filter (Sowel: `type ? filter : all`).
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("type="))
        .filter(|text| !text.is_empty())
        .map(EquipmentType::parse)
}

fn create(home: &mut Home, body: &[u8], now: u64, random: Random) -> Response {
    let result = input::object(body).and_then(|fields| {
        let name = input::string(&fields, "name")?;
        let kind = input::string(&fields, "type")?;
        let zone_id = input::string(&fields, "zoneId")?;
        let (icon, description) = input::text_fields(&fields)?;
        let device_ids = device_ids(&fields)?;
        let name = input::name(&name.ok_or_else(|| input::missing_field("name"))?)?;
        let kind = kind.ok_or_else(|| input::missing_field("type"))?;
        let zone_id = zone_id.ok_or_else(|| input::missing_field("zoneId"))?;
        home.create_equipment(
            NewEquipment {
                name,
                kind,
                zone_id,
                icon: icon.flatten(),
                description: description.flatten(),
                device_ids,
            },
            now,
            random,
        )
    });
    match result {
        Ok(item) => json(201, &details(&home.devices, &item, now)),
        Err(e) => failure(&e),
    }
}

/// `deviceIds`: a list of strings, at most `MAX_DEVICES` of them.
fn device_ids(fields: &Fields) -> Outcome<Vec<String>> {
    match fields.get("deviceIds") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(list)) if list.len() <= MAX_DEVICES => list
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| input::invalid_field("deviceIds"))
            })
            .collect(),
        Some(_) => Err(input::invalid_field("deviceIds")),
    }
}

fn update(home: &mut Home, id: &str, body: &[u8], now: u64) -> Response {
    let result = input::object(body).and_then(|fields| {
        let name = input::string(&fields, "name")?;
        let kind = input::string(&fields, "type")?;
        let zone_id = input::string(&fields, "zoneId")?;
        let enabled = input::boolean(&fields, "enabled")?;
        let (icon, description) = input::text_fields(&fields)?;
        let name = match name {
            Some(text) => Some(input::name(&text)?),
            None => None,
        };
        home.update_equipment(
            id,
            EquipmentUpdate {
                name,
                kind,
                zone_id,
                icon,
                description,
                enabled,
            },
            now,
        )
    });
    match result {
        Ok(equipment) => json(200, &equipment),
        Err(e) => failure(&e),
    }
}

/// The two fields of a binding body: the id of the data (or order), and the alias.
fn binding_fields(body: &[u8], id_field: &str) -> Outcome<(String, String)> {
    let fields = input::object(body)?;
    input::refuse_unknown(&fields, &[id_field, "alias"])?;
    let target = input::string(&fields, id_field)?;
    let alias = input::string(&fields, "alias")?;
    let target = target.ok_or_else(|| input::missing_field(id_field))?;
    let alias = alias.ok_or_else(|| input::missing_field("alias"))?;
    Ok((target, input::alias(&alias)?))
}

fn add_data(home: &mut Home, id: &str, body: &[u8], random: Random) -> Response {
    match binding_fields(body, "deviceDataId")
        .and_then(|(data, alias)| home.add_data_binding(id, &data, &alias, random))
    {
        Ok(binding) => match Id::parse(id) {
            Some(equipment_id) => json(
                201,
                &DataBindingOut {
                    equipment_id,
                    binding: &binding,
                },
            ),
            None => error(500, "Internal error"),
        },
        Err(e) => failure(&e),
    }
}

fn add_order(home: &mut Home, id: &str, body: &[u8], random: Random) -> Response {
    match binding_fields(body, "deviceOrderId")
        .and_then(|(order, alias)| home.add_order_binding(id, &order, &alias, random))
    {
        Ok(binding) => match Id::parse(id) {
            Some(equipment_id) => json(
                201,
                &OrderBindingOut {
                    equipment_id,
                    binding: &binding,
                },
            ),
            None => error(500, "Internal error"),
        },
        Err(e) => failure(&e),
    }
}
