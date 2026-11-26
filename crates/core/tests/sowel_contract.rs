//! The Sowel contract check (spec 027, R1): Solite's names and values are Sowel's, or a documented
//! deviation. Three inputs:
//!
//! - the snapshot `contract/sowel-snapshot.json`, extracted from the Sowel repository (its
//!   `src/shared/types.ts`, `src/core/timezone.ts`, its routes and its `home.*` keys) and checked
//!   in, so CI checks without Sowel;
//! - the Sowel repository itself when it is present (`SOWEL_REPO`, else `../sowel` beside this
//!   repository): Solite is then checked against it too, and a snapshot that differs is a warning;
//! - `docs/sowel-subset.md`: what Solite has (its tables must equal the code) and the deviations,
//!   each with its decision.
//!
//! `SOWEL_SNAPSHOT=write cargo test -p solite-core --test sowel_contract` rewrites the snapshot
//! from the Sowel repository.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use solite_core::api::{ROUTES, Reach, Request, Response, handle};
use solite_core::devices::{DeviceStatus, Devices, PowerSource, ValueType, catalog};
use solite_core::equipments::{EquipmentFamily, EquipmentType, rules};
use solite_core::home::Home;
use solite_core::live::EngineEvent;
use solite_core::settings;

// ---------------------------------------------------------------- the snapshot and its extractor

/// The string unions taken from Sowel, with the file each is in.
const UNIONS: [(&str, &str); 9] = [
    ("EquipmentType", "src/shared/types.ts"),
    ("EquipmentFamily", "src/shared/types.ts"),
    ("DataType", "src/shared/types.ts"),
    ("DataCategory", "src/shared/types.ts"),
    ("OrderCategory", "src/shared/types.ts"),
    ("DeviceStatus", "src/shared/types.ts"),
    ("PowerSource", "src/shared/types.ts"),
    ("DeviceSource", "src/shared/types.ts"),
    ("TimezoneSource", "src/core/timezone.ts"),
];

/// The interfaces whose fields are taken from Sowel's `types.ts` (with those they extend).
const INTERFACES: [&str; 17] = [
    "Device",
    "DeviceWithDetails",
    "DeviceData",
    "DeviceOrder",
    "Zone",
    "ZoneWithChildren",
    "ZoneAggregatedData",
    "Equipment",
    "EquipmentWithDetails",
    "DataBinding",
    "DataBindingWithValue",
    "OrderBinding",
    "OrderBindingWithDetails",
    "RecipeInfo",
    "RecipeSlotDef",
    "RecipeInstance",
    "RecipeLogEntry",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    /// The Sowel commit the snapshot was taken from.
    commit: String,
    unions: BTreeMap<String, BTreeSet<String>>,
    interfaces: BTreeMap<String, BTreeSet<String>>,
    slot_types: BTreeSet<String>,
    events: BTreeSet<String>,
    settings: BTreeSet<String>,
    /// `GET /api/v1/zones/:`: the method in capitals, a parameter written `:`.
    routes: BTreeSet<String>,
}

/// The text with its `//` and `/* */` comments removed, string literals kept whole.
fn strip_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            quote = Some(c);
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The `"..."` literals of a text.
fn literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

/// The body of `export type <name> = ... ;` (comments already stripped).
fn union_body<'a>(text: &'a str, name: &str) -> Result<&'a str, String> {
    let head = format!("export type {name} =");
    let start = text
        .find(&head)
        .ok_or_else(|| format!("union {name} not found"))?
        + head.len();
    let end = text[start..]
        .find(';')
        .ok_or_else(|| format!("union {name} has no end"))?;
    Ok(&text[start..start + end])
}

/// The string values of a union.
fn union_values(text: &str, name: &str) -> Result<BTreeSet<String>, String> {
    let values: BTreeSet<String> = literals(union_body(text, name)?).into_iter().collect();
    if values.is_empty() {
        return Err(format!("union {name} has no string value"));
    }
    Ok(values)
}

/// The body of `export interface <name> ... { ... }` and the names it extends.
fn interface_body<'a>(text: &'a str, name: &str) -> Result<(&'a str, Vec<String>), String> {
    let head = format!("export interface {name} ");
    let start = text
        .find(&head)
        .ok_or_else(|| format!("interface {name} not found"))?
        + head.len();
    let open = start
        + text[start..]
            .find('{')
            .ok_or_else(|| format!("interface {name} has no body"))?;
    let extends: Vec<String> = text[start..open]
        .trim()
        .strip_prefix("extends")
        .map(|list| {
            list.split(',')
                .map(|n| n.split('<').next().unwrap_or("").trim().to_string())
                .filter(|n| !n.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let mut depth = 0usize;
    for (offset, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&text[open + 1..open + offset], extends));
                }
            }
            _ => {}
        }
    }
    Err(format!("interface {name} is not closed"))
}

/// The top-level field names of an interface, with those of the interfaces it extends.
fn interface_fields(text: &str, name: &str) -> Result<BTreeSet<String>, String> {
    let (body, extends) = interface_body(text, name)?;
    let mut fields = BTreeSet::new();
    for parent in extends {
        fields.extend(interface_fields(text, &parent)?);
    }
    // A field starts a statement at depth 0 of the body: `name:` or `name?:`.
    let mut depth = 0i32;
    let mut statement = String::new();
    let mut previous = ' ';
    for c in body.chars() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            // The `>` of an arrow (`=>`) closes nothing.
            '>' if previous == '=' => {}
            '}' | ')' | ']' | '>' => depth -= 1,
            _ => {}
        }
        previous = c;
        if depth == 0 && (c == ';' || c == '\n' || c == ',') {
            take_field(&statement, &mut fields);
            statement.clear();
        } else if depth == 0 || !statement.contains(':') {
            statement.push(c);
        }
    }
    take_field(&statement, &mut fields);
    if fields.is_empty() {
        return Err(format!("interface {name} has no field"));
    }
    Ok(fields)
}

fn take_field(statement: &str, fields: &mut BTreeSet<String>) {
    let statement = statement.trim().trim_start_matches("readonly ").trim();
    let Some(colon) = statement.find(':') else {
        return;
    };
    let name = statement[..colon].trim().trim_end_matches('?');
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        fields.insert(name.to_string());
    }
}

/// The literal values of one field's type in an interface (`RecipeSlotDef.type`).
fn field_literals(text: &str, interface: &str, field: &str) -> Result<BTreeSet<String>, String> {
    let (body, _) = interface_body(text, interface)?;
    let head = format!("\n  {field}:");
    let start = body
        .find(&head)
        .ok_or_else(|| format!("{interface}.{field} not found"))?
        + head.len();
    let end = body[start..].find(';').unwrap_or(body.len() - start);
    let values: BTreeSet<String> = literals(&body[start..start + end]).into_iter().collect();
    if values.is_empty() {
        return Err(format!("{interface}.{field} has no literal"));
    }
    Ok(values)
}

/// The `type` of every member of `EngineEvent`.
fn engine_events(text: &str) -> Result<BTreeSet<String>, String> {
    let head = "export type EngineEvent =";
    let start = text.find(head).ok_or("union EngineEvent not found")? + head.len();
    let end = text[start..]
        .find("\nexport ")
        .unwrap_or(text.len() - start);
    let body = &text[start..start + end];
    let mut events = BTreeSet::new();
    let mut rest = body;
    while let Some(at) = rest.find("type: \"") {
        let after = &rest[at + 7..];
        let Some(close) = after.find('"') else { break };
        events.insert(after[..close].to_string());
        rest = &after[close..];
    }
    if events.is_empty() {
        return Err("EngineEvent has no member".into());
    }
    Ok(events)
}

/// The `"home.<word>"` literals of a file, translation keys left out (an argument of `t(`, as
/// Sowel's interface writes `t("home.welcome")`).
fn settings_keys(text: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("\"home.") {
        let start = from + at;
        let after = &text[start + 1..];
        from = start + 1;
        let Some(end) = after.find('"') else { break };
        let literal = &after[..end];
        let word = &literal["home.".len()..];
        if word.is_empty() || !word.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let before = text[..start].trim_end();
        if before.ends_with("t(") {
            continue;
        }
        keys.insert(literal.to_string());
    }
    keys
}

/// The routes of a file: `app.<method>[<...>](\n "<path>"`, as `METHOD /path/:`.
fn routes_of(text: &str) -> BTreeSet<String> {
    let mut routes = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("app.") {
        rest = &rest[at + 4..];
        let method: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        if !matches!(method.as_str(), "get" | "post" | "put" | "delete" | "patch") {
            continue;
        }
        let mut tail = rest[method.len()..].trim_start();
        if tail.starts_with('<') {
            let mut depth = 0i32;
            let mut end = 0;
            for (i, c) in tail.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            tail = tail[end..].trim_start();
        }
        let Some(tail) = tail.strip_prefix('(') else {
            continue;
        };
        let tail = tail.trim_start();
        let Some(path) = tail.strip_prefix('"').and_then(|t| t.split('"').next()) else {
            continue;
        };
        routes.insert(route_key(&method.to_uppercase(), path));
    }
    routes
}

/// `METHOD /path/:`, every parameter written `:`.
fn route_key(method: &str, path: &str) -> String {
    let path: Vec<&str> = path
        .split('/')
        .map(|s| if s.starts_with(':') { ":" } else { s })
        .collect();
    format!("{method} {}", path.join("/"))
}

/// The files under a folder whose name ends in `.ts` or `.tsx`, tests left out.
fn ts_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            ts_files(&path, out);
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let source = name.ends_with(".ts") || name.ends_with(".tsx");
            let test = name.contains(".test.") || name.ends_with(".d.ts");
            if source && !test {
                out.push(path);
            }
        }
    }
    out.sort();
}

fn read(repo: &Path, file: &str) -> Result<String, String> {
    let path = repo.join(file);
    fs::read_to_string(&path)
        .map(|t| strip_comments(&t))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// The snapshot of a Sowel repository.
fn extract(repo: &Path) -> Result<Snapshot, String> {
    let types = read(repo, "src/shared/types.ts")?;
    let mut unions = BTreeMap::new();
    for (name, file) in UNIONS {
        let text = if file == "src/shared/types.ts" {
            types.clone()
        } else {
            read(repo, file)?
        };
        unions.insert(
            name.to_string(),
            union_values(&text, name).map_err(|e| format!("{file}: {e}"))?,
        );
    }
    let mut interfaces = BTreeMap::new();
    for name in INTERFACES {
        interfaces.insert(
            name.to_string(),
            interface_fields(&types, name).map_err(|e| format!("src/shared/types.ts: {e}"))?,
        );
    }
    let slot_types = field_literals(&types, "RecipeSlotDef", "type")
        .map_err(|e| format!("src/shared/types.ts: {e}"))?;
    let events = engine_events(&types).map_err(|e| format!("src/shared/types.ts: {e}"))?;
    // The settings keys are those of the engine and of Sowel's interface (which writes
    // `home.name`); the routes are the engine's.
    let mut files = Vec::new();
    ts_files(&repo.join("src"), &mut files);
    ts_files(&repo.join("ui/src"), &mut files);
    let mut settings = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let text = strip_comments(&text);
        settings.extend(settings_keys(&text));
        if file.starts_with(repo.join("src/api")) {
            routes.extend(routes_of(&text));
        }
    }
    if settings.is_empty() || routes.is_empty() {
        return Err(format!(
            "{}: no settings key or no route found",
            repo.join("src").display()
        ));
    }
    let commit = fs::read_to_string(repo.join(".git/HEAD"))
        .ok()
        .and_then(|head| {
            let head = head.trim();
            match head.strip_prefix("ref: ") {
                Some(reference) => fs::read_to_string(repo.join(".git").join(reference)).ok(),
                None => Some(head.to_string()),
            }
        })
        .map(|c| c.trim().chars().take(8).collect())
        .unwrap_or_else(|| "unknown".into());
    Ok(Snapshot {
        commit,
        unions,
        interfaces,
        slot_types,
        events,
        settings,
        routes,
    })
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn snapshot_path() -> PathBuf {
    root().join("contract/sowel-snapshot.json")
}

/// A folder checked to be a Sowel repository (it has `src/shared/types.ts`).
fn checked_repo(path: PathBuf) -> Result<PathBuf, String> {
    if path.join("src/shared/types.ts").is_file() {
        Ok(path)
    } else {
        Err(format!(
            "SOWEL_REPO: {} has no src/shared/types.ts",
            path.display()
        ))
    }
}

/// The Sowel repository, when present: `SOWEL_REPO`, else `../sowel` beside this repository.
/// `SOWEL_REPO` set to a folder that is not one is an error, never a silent skip.
fn sowel_repo() -> Result<Option<PathBuf>, String> {
    if let Ok(given) = std::env::var("SOWEL_REPO") {
        return checked_repo(PathBuf::from(given)).map(Some);
    }
    // `../sowel` absent: checked against the snapshot only. Present but not a Sowel repository
    // (its `types.ts` moved): an error, never a silent skip.
    let beside = root().join("../sowel");
    if !beside.is_dir() {
        return Ok(None);
    }
    checked_repo(beside).map(Some)
}

/// What differs between two snapshots, one line each (the commit left out).
fn differences(old: &Snapshot, new: &Snapshot) -> Vec<String> {
    fn sets(
        kind: &str,
        old: &BTreeMap<String, BTreeSet<String>>,
        new: &BTreeMap<String, BTreeSet<String>>,
        out: &mut Vec<String>,
    ) {
        for (name, values) in new {
            let before = old.get(name).cloned().unwrap_or_default();
            set(&format!("{kind} {name}"), &before, values, out);
        }
    }
    fn set(kind: &str, old: &BTreeSet<String>, new: &BTreeSet<String>, out: &mut Vec<String>) {
        for added in new.difference(old) {
            out.push(format!("{kind}: + {added}"));
        }
        for removed in old.difference(new) {
            out.push(format!("{kind}: - {removed}"));
        }
    }
    let mut out = Vec::new();
    sets("union", &old.unions, &new.unions, &mut out);
    sets("interface", &old.interfaces, &new.interfaces, &mut out);
    set("slot types", &old.slot_types, &new.slot_types, &mut out);
    set("events", &old.events, &new.events, &mut out);
    set("settings", &old.settings, &new.settings, &mut out);
    set("routes", &old.routes, &new.routes, &mut out);
    out
}

// ---------------------------------------------------------------- Solite's side

/// One name Solite has, of one kind, to be found in Sowel's set or in the deviations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Name {
    kind: String,
    name: String,
}

fn name(kind: &str, name: &str) -> Name {
    Name {
        kind: kind.into(),
        name: name.into(),
    }
}

fn counter() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u32;
    move |buffer: &mut [u8]| {
        n += 1;
        buffer.fill(0x5A);
        buffer[..4].copy_from_slice(&n.to_be_bytes());
        Ok(())
    }
}

const T0: u64 = 1_795_252_323_120;

fn call(home: &mut Home, method: &str, path: &str, body: &str) -> Response {
    handle(
        home,
        &Request {
            method,
            path,
            host: Some("127.0.0.1:8080"),
            port: 8080,
            reach: Reach::Loopback,
            content_type: Some("application/json"),
            authorization: None,
            content_length: None,
            body: body.as_bytes(),
        },
        T0 + 2000,
        &mut counter(),
    )
}

fn json(response: Response) -> Value {
    let body = response.body.expect("a body");
    serde_json::from_str(&body).expect("JSON")
}

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// Every field name Solite writes, as `Interface.field`, from real answers of the API, and the
/// recipe slot types it declares.
fn solite_fields() -> (BTreeMap<String, BTreeSet<String>>, BTreeSet<String>) {
    let mut devices = Devices::new();
    let mut random = counter();
    let th = [0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x78];
    let relay = [0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x7A];
    devices
        .identified(th, Some("eWeLink"), Some("TH01"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(th, &[0x0001, 0x0405, 0x0402, 0x0406], T0, &mut random)
        .unwrap();
    devices
        .identified(relay, Some("SONOFF"), Some("ZBMINIR2"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(relay, &[0x0006, 0x0008], T0, &mut random)
        .unwrap();
    let ids: Vec<String> = devices
        .list()
        .iter()
        .map(|d| d.device.id.to_string())
        .collect();
    let mut home = Home::from_devices(devices, T0);
    let mut fields: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut slot_types = BTreeSet::new();
    let mut add = |interface: &str, value: &Value| {
        fields
            .entry(interface.to_string())
            .or_default()
            .extend(keys(value));
    };

    let device = json(call(
        &mut home,
        "GET",
        &format!("/api/v1/devices/{}", ids[1]),
        "",
    ));
    add("DeviceWithDetails", &device);
    for data in device["data"].as_array().expect("data") {
        add("DeviceData", data);
    }
    for order in device["orders"].as_array().expect("orders") {
        add("DeviceOrder", order);
    }

    let zone = json(call(
        &mut home,
        "POST",
        "/api/v1/zones",
        r#"{"name":"Garage","description":"d","icon":"car"}"#,
    ));
    let zone_id = zone["id"].as_str().expect("an id").to_string();
    add("Zone", &zone);
    let tree = json(call(&mut home, "GET", "/api/v1/zones", ""));
    for root in tree.as_array().expect("a tree") {
        add("ZoneWithChildren", root);
    }

    let body = format!(
        r#"{{"name":"Light","type":"light_onoff","zoneId":"{zone_id}","deviceIds":["{}"],"icon":"i","description":"d"}}"#,
        ids[1]
    );
    let created = json(call(&mut home, "POST", "/api/v1/equipments", &body));
    let equipment_id = created["id"].as_str().expect("an id").to_string();
    let body = format!(
        r#"{{"name":"Sensor","type":"sensor","zoneId":"{zone_id}","deviceIds":["{}"]}}"#,
        ids[0]
    );
    let sensor = json(call(&mut home, "POST", "/api/v1/equipments", &body));
    let details = json(call(
        &mut home,
        "GET",
        &format!("/api/v1/equipments/{equipment_id}"),
        "",
    ));
    for value in [&details, &sensor] {
        add("EquipmentWithDetails", value);
        for binding in value["dataBindings"].as_array().expect("data bindings") {
            add("DataBindingWithValue", binding);
        }
        for binding in value["orderBindings"].as_array().expect("order bindings") {
            add("OrderBindingWithDetails", binding);
        }
    }

    let aggregation = json(call(&mut home, "GET", "/api/v1/zones/aggregation", ""));
    for (_, data) in aggregation.as_object().expect("a map") {
        add("ZoneAggregatedData", data);
    }

    let recipes = json(call(&mut home, "GET", "/api/v1/recipes", ""));
    for recipe in recipes.as_array().expect("recipes") {
        add("RecipeInfo", recipe);
        for slot in recipe["slots"].as_array().expect("slots") {
            add("RecipeSlotDef", slot);
            if let Some(kind) = slot["type"].as_str() {
                slot_types.insert(kind.to_string());
            }
        }
    }
    let body = format!(
        r#"{{"recipeId":"motion-light","params":{{"zone":"{zone_id}","lights":["{equipment_id}"]}}}}"#
    );
    let instance = call(&mut home, "POST", "/api/v1/recipe-instances", &body);
    assert_eq!(instance.status, 201, "{:?}", instance.body);
    let instance = json(instance);
    add("RecipeInstance", &instance);
    let instance_id = instance["id"].as_str().expect("an id").to_string();
    let log = json(call(
        &mut home,
        "GET",
        &format!("/api/v1/recipe-instances/{instance_id}/log"),
        "",
    ));
    let entries = log.as_array().expect("a log");
    assert!(!entries.is_empty(), "a log entry to read");
    for entry in entries {
        add("RecipeLogEntry", entry);
    }
    (fields, slot_types)
}

/// An equipment with all of Sowel's optional fields, read and written back by Solite.
const EQUIPMENT_WITH_EVERY_FIELD: &str = r#"[{"id":"7b1e3f0a-1c2d-4e5f-8a9b-0c1d2e3f4a5b","name":"Plug","zoneId":"00000000-0000-0000-0000-000000000001","type":"switch","icon":"plug","description":"d","enabled":true,"createdAt":"2026-11-24T10:00:00.000Z","updatedAt":"2026-11-24T10:00:00.000Z","energyProfile":{"class":"deferrable","nominalPowerW":2000,"minOnS":600,"minOffS":600},"requireConfirmation":true,"trackOrderOutcome":false,"meteringParentId":"8b1e3f0a-1c2d-4e5f-8a9b-0c1d2e3f4a5b","separateSupply":false,"localProduction":false,"family":"climate","plugInHybrid":false,"usualVehicleId":"9b1e3f0a-1c2d-4e5f-8a9b-0c1d2e3f4a5b","solarProfile":{"planes":[{"tiltDeg":30,"azimuthDeg":180,"peakWc":3000}]},"invertDirection":false,"timedCommand":{"alias":"state","value":true,"revertValue":false,"durationMs":60000},"dataBindings":[],"orderBindings":[]}]"#;

fn solite_equipment_keys() -> BTreeSet<String> {
    let list = solite_core::equipments::parse_list(EQUIPMENT_WITH_EVERY_FIELD).expect("parsed");
    let value = serde_json::to_value(&list[0].equipment).expect("serialised");
    keys(&value)
}

/// Solite's names of each kind, from its code.
fn solite_names() -> Vec<Name> {
    let mut names = Vec::new();
    for family in EquipmentFamily::ALL {
        names.push(name("union EquipmentFamily", family.as_str()));
    }
    for value in ValueType::ALL {
        names.push(name("union DataType", value.as_str()));
    }
    for status in DeviceStatus::ALL {
        names.push(name("union DeviceStatus", status.as_str()));
    }
    for source in PowerSource::ALL {
        names.push(name("union PowerSource", source.as_str()));
    }
    for category in data_categories() {
        names.push(name("union DataCategory", &category));
    }
    for category in order_categories() {
        names.push(name("union OrderCategory", &category));
    }
    for event in EngineEvent::NAMES {
        names.push(name("event", event));
    }
    for key in settings::KEYS {
        names.push(name("setting", key));
    }
    for (method, path) in ROUTES {
        names.push(name("route", &route_key(method, path)));
    }
    let (fields, slot_types) = solite_fields();
    assert!(!slot_types.is_empty(), "the recipes declare slot types");
    for (interface, fields) in fields {
        for field in fields {
            names.push(name("field", &format!("{interface}.{field}")));
        }
    }
    for kind in slot_types {
        names.push(name("slot type", &kind));
    }
    for field in solite_equipment_keys() {
        names.push(name("field", &format!("Equipment.{field}")));
    }
    names
}

fn data_categories() -> BTreeSet<String> {
    catalog::all_data()
        .map(|d| d.category.to_string())
        .collect()
}

fn order_categories() -> BTreeSet<String> {
    catalog::all_orders()
        .map(|o| o.category.to_string())
        .collect()
}

/// Whether Sowel has this name.
fn sowel_has(sowel: &Snapshot, item: &Name) -> bool {
    if let Some(union) = item.kind.strip_prefix("union ") {
        return sowel
            .unions
            .get(union)
            .is_some_and(|v| v.contains(&item.name));
    }
    match item.kind.as_str() {
        "event" => sowel.events.contains(&item.name),
        "setting" => sowel.settings.contains(&item.name),
        "route" => sowel.routes.contains(&item.name),
        "slot type" => sowel.slot_types.contains(&item.name),
        "field" => {
            let Some((interface, field)) = item.name.split_once('.') else {
                return false;
            };
            sowel
                .interfaces
                .get(interface)
                .is_some_and(|f| f.contains(field))
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- docs/sowel-subset.md

/// The first cells (backticks removed) of the table under each `## ` heading.
fn tables(text: &str) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut out: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    let mut heading = String::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            heading = h.trim().to_string();
            continue;
        }
        let line = line.trim();
        if !line.starts_with('|') || line.starts_with("| -") || line.starts_with("|-") {
            continue;
        }
        let cells: Vec<String> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().trim_matches('`').to_string())
            .collect();
        // The header row is the first of each table: kept apart by its plain first word.
        out.entry(heading.clone()).or_default().push(cells);
    }
    for rows in out.values_mut() {
        if !rows.is_empty() {
            rows.remove(0);
        }
    }
    out
}

fn subset_doc() -> String {
    fs::read_to_string(root().join("docs/sowel-subset.md")).expect("docs/sowel-subset.md")
}

/// A deviation: a Solite name Sowel does not have, kept by a decision.
#[derive(Debug, Clone)]
struct Deviation {
    kind: String,
    name: String,
    decision: String,
}

fn deviations(doc: &str) -> Vec<Deviation> {
    tables(doc)
        .remove("Deviations")
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.len() >= 3)
        .map(|row| Deviation {
            kind: row[0].clone(),
            name: row[1].clone(),
            decision: row[2].clone(),
        })
        .collect()
}

/// The failures of Solite's names against Sowel's: a name neither Sowel's nor a deviation; a
/// deviation row whose name Solite does not have, or Sowel has, or whose decision is not in
/// `decisions`.
fn check_names(
    solite: &[Name],
    sowel: &Snapshot,
    deviations: &[Deviation],
    decisions: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let listed = |item: &Name| {
        deviations
            .iter()
            .any(|d| d.kind == item.kind && d.name == item.name)
    };
    for item in solite {
        if !sowel_has(sowel, item) && !listed(item) {
            failures.push(format!(
                "{} `{}`: not Sowel's, and not in the Deviations table of docs/sowel-subset.md",
                item.kind, item.name
            ));
        }
    }
    for row in deviations {
        let item = name(&row.kind, &row.name);
        if !solite.contains(&item) {
            failures.push(format!(
                "deviation row {} `{}`: Solite does not have it (stale row)",
                row.kind, row.name
            ));
        } else if sowel_has(sowel, &item) {
            failures.push(format!(
                "deviation row {} `{}`: Sowel has it, it is no deviation (stale row)",
                row.kind, row.name
            ));
        }
        if !decisions.contains(&format!("## {}:", row.decision)) {
            failures.push(format!(
                "deviation row {} `{}`: decision {} not in docs/decisions.md",
                row.kind, row.name, row.decision
            ));
        }
    }
    failures
}

/// The failures of one subset table against the code's list.
fn check_table(
    heading: &str,
    rows: Option<&Vec<Vec<String>>>,
    code: &BTreeSet<String>,
) -> Vec<String> {
    let Some(rows) = rows else {
        return vec![format!(
            "docs/sowel-subset.md: no table under `## {heading}`"
        )];
    };
    let table: BTreeSet<String> = rows.iter().filter_map(|r| r.first().cloned()).collect();
    let mut failures = Vec::new();
    for missing in code.difference(&table) {
        failures.push(format!(
            "docs/sowel-subset.md, {heading}: `{missing}` is in the code, not in the table"
        ));
    }
    for extra in table.difference(code) {
        failures.push(format!(
            "docs/sowel-subset.md, {heading}: `{extra}` is in the table, not in the code"
        ));
    }
    failures
}

/// The subset tables against the code.
fn check_subset(doc: &str) -> Vec<String> {
    let tables = tables(doc);
    let routes: BTreeSet<String> = ROUTES
        .iter()
        .map(|(method, path)| format!("{method} {path}"))
        .collect();
    let behaviour: BTreeSet<String> = EquipmentType::ALL
        .into_iter()
        .filter(|t| rules::has_rules(*t))
        .map(|t| t.as_str().to_string())
        .collect();
    let events: BTreeSet<String> = EngineEvent::NAMES.iter().map(|e| e.to_string()).collect();
    let keys: BTreeSet<String> = settings::KEYS.iter().map(|k| k.to_string()).collect();
    let mut failures = Vec::new();
    for (heading, code) in [
        ("Data categories", data_categories()),
        ("Order categories", order_categories()),
        ("Events", events),
        ("Settings keys", keys),
        ("Routes", routes),
        ("Equipment types with behaviour", behaviour),
    ] {
        failures.extend(check_table(heading, tables.get(heading), &code));
    }
    failures
}

/// Everything that fails Solite against one snapshot.
fn check_all(sowel: &Snapshot) -> Vec<String> {
    let mut failures = Vec::new();
    let types: BTreeSet<String> = EquipmentType::ALL
        .iter()
        .map(|t| t.as_str().to_string())
        .collect();
    let sowel_types = sowel
        .unions
        .get("EquipmentType")
        .cloned()
        .unwrap_or_default();
    for missing in sowel_types.difference(&types) {
        failures.push(format!(
            "EquipmentType: Sowel's `{missing}` missing in Solite"
        ));
    }
    for extra in types.difference(&sowel_types) {
        failures.push(format!("EquipmentType: `{extra}` is not Sowel's"));
    }
    let equipment = solite_equipment_keys();
    let sowel_equipment = sowel
        .interfaces
        .get("Equipment")
        .cloned()
        .unwrap_or_default();
    for missing in sowel_equipment.difference(&equipment) {
        failures.push(format!(
            "Equipment: Sowel's field `{missing}` not kept by Solite"
        ));
    }
    let doc = subset_doc();
    let decisions = fs::read_to_string(root().join("docs/decisions.md")).expect("decisions");
    failures.extend(check_names(
        &solite_names(),
        sowel,
        &deviations(&doc),
        &decisions,
    ));
    failures.extend(check_subset(&doc));
    failures
}

// ---------------------------------------------------------------- the tests

#[test]
fn solite_is_a_subset_of_sowel() {
    let snapshot_file = snapshot_path();
    let repo = sowel_repo().unwrap_or_else(|e| panic!("sowel contract: {e}"));
    if std::env::var("SOWEL_SNAPSHOT").as_deref() == Ok("write") {
        let repo = repo
            .as_ref()
            .expect("SOWEL_SNAPSHOT=write needs the Sowel repository");
        let snapshot = extract(repo).unwrap_or_else(|e| panic!("sowel contract: {e}"));
        let text = serde_json::to_string_pretty(&snapshot).expect("serialised") + "\n";
        fs::write(&snapshot_file, text).expect("snapshot written");
        println!("sowel contract: snapshot written from {}", repo.display());
    }
    let snapshot: Snapshot = serde_json::from_str(
        &fs::read_to_string(&snapshot_file)
            .unwrap_or_else(|e| panic!("sowel contract: {}: {e}", snapshot_file.display())),
    )
    .expect("a snapshot");
    let mut failures = check_all(&snapshot);
    let against = match repo {
        None => format!(
            "../sowel not found, checked against the snapshot only ({})",
            snapshot.commit
        ),
        Some(repo) => {
            let live = extract(&repo).unwrap_or_else(|e| panic!("sowel contract: {e}"));
            let drift = differences(&snapshot, &live);
            if !drift.is_empty() {
                println!(
                    "sowel contract: warning, the snapshot ({}) differs from {} ({}); rewrite it with SOWEL_SNAPSHOT=write cargo test -p solite-core --test sowel_contract:",
                    snapshot.commit,
                    repo.display(),
                    live.commit
                );
                for line in &drift {
                    println!("  {line}");
                }
            }
            for failure in check_all(&live) {
                let line = format!("{failure} (against {})", repo.display());
                if !failures.contains(&failure) {
                    failures.push(line);
                }
            }
            format!(
                "checked against the snapshot ({}) and {} ({})",
                snapshot.commit,
                repo.display(),
                live.commit
            )
        }
    };
    if !failures.is_empty() {
        panic!(
            "sowel contract: {} deviation(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
    println!("sowel contract: OK ({against})");
}

const SAMPLE: &str = r#"
// A comment with "quotes" and a union: export type Nope = "x";
export type Kind =
  | "one" // trailing
  /* block "comment" */
  | "two"
  | (string & {});

export interface Base {
  id: string;
}

export interface Thing extends Base {
  /** A doc comment: with a colon. */
  name?: string;
  nested: { inner: number; other: string };
  list: Array<{ deep: string }>;
  kind: "a" | "b";
  readonly fixed: number;
}

export type EngineEvent =
  | { type: "x.created"; thing: Thing }
  | {
      type: "x.removed";
      id: string;
    };

export function register(app: App) {
  app.get("/api/v1/things", async () => []);
  app.post<{
    Params: { id: string };
    Body: { name: string };
  }>(
    "/api/v1/things/:id/rename",
    async () => ({}),
  );
}
"#;

#[test]
fn the_extractor_reads_unions_interfaces_events_and_routes() {
    let text = strip_comments(SAMPLE);
    assert_eq!(
        union_values(&text, "Kind").unwrap(),
        ["one", "two"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(
        interface_fields(&text, "Thing").unwrap(),
        ["id", "name", "nested", "list", "kind", "fixed"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(
        field_literals(&text, "Thing", "kind").unwrap(),
        ["a", "b"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(
        engine_events(&text).unwrap(),
        ["x.created", "x.removed"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(
        routes_of(&text),
        ["GET /api/v1/things", "POST /api/v1/things/:/rename"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
}

#[test]
fn translation_keys_are_not_settings_keys() {
    let text = r#"const a = s["home.name"]; t("home.welcome"); t(
  "home.noZones"); const k = "home.";"#;
    assert_eq!(
        settings_keys(text),
        ["home.name".to_string()].into_iter().collect()
    );
}

#[test]
fn a_name_the_extractor_cannot_find_fails_naming_it() {
    let text = strip_comments(SAMPLE);
    assert_eq!(
        union_values(&text, "Nope").unwrap_err(),
        "union Nope not found"
    );
    assert_eq!(
        interface_fields(&text, "Missing").unwrap_err(),
        "interface Missing not found"
    );
}

#[test]
fn sowel_repo_pointing_at_a_folder_without_types_fails_naming_the_file() {
    let error = checked_repo(std::env::temp_dir().join("solite-not-sowel")).unwrap_err();
    assert!(
        error.contains("solite-not-sowel has no src/shared/types.ts"),
        "{error}"
    );
}

fn tiny_sowel() -> Snapshot {
    Snapshot {
        commit: "test".into(),
        unions: BTreeMap::new(),
        interfaces: BTreeMap::new(),
        slot_types: BTreeSet::new(),
        events: ["device.discovered".to_string()].into_iter().collect(),
        settings: BTreeSet::new(),
        routes: ["GET /api/v1/zones".to_string()].into_iter().collect(),
    }
}

#[test]
fn a_name_outside_sowel_and_not_a_deviation_fails() {
    let solite = vec![
        name("event", "device.discovered"),
        name("event", "device.renamed"),
    ];
    let failures = check_names(&solite, &tiny_sowel(), &[], "");
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains("device.renamed"), "{failures:?}");
}

#[test]
fn a_listed_deviation_passes_and_a_stale_row_fails() {
    let solite = vec![name("route", "GET /api/v1/settings/timezones")];
    let listed = |name: &str, decision: &str| Deviation {
        kind: "route".into(),
        name: name.into(),
        decision: decision.into(),
    };
    let decisions = "## D41: time zones\n";
    assert!(
        check_names(
            &solite,
            &tiny_sowel(),
            &[listed("GET /api/v1/settings/timezones", "D41")],
            decisions
        )
        .is_empty()
    );
    // A row Solite no longer has.
    let failures = check_names(
        &solite,
        &tiny_sowel(),
        &[
            listed("GET /api/v1/settings/timezones", "D41"),
            listed("GET /api/v1/gone", "D41"),
        ],
        decisions,
    );
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].contains("gone"), "{failures:?}");
    // A row Sowel now has.
    let solite = vec![name("route", "GET /api/v1/zones")];
    let failures = check_names(
        &solite,
        &tiny_sowel(),
        &[listed("GET /api/v1/zones", "D41")],
        decisions,
    );
    assert!(failures[0].contains("Sowel has it"), "{failures:?}");
    // A decision that does not exist.
    let solite = vec![name("route", "GET /api/v1/settings/timezones")];
    let failures = check_names(
        &solite,
        &tiny_sowel(),
        &[listed("GET /api/v1/settings/timezones", "D99")],
        decisions,
    );
    assert!(failures[0].contains("D99"), "{failures:?}");
}

#[test]
fn a_subset_table_that_differs_from_the_code_fails() {
    let code: BTreeSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let rows = vec![vec!["a".to_string()], vec!["c".to_string()]];
    let failures = check_table("Events", Some(&rows), &code);
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures.iter().any(|f| f.contains("`b` is in the code")));
    assert!(failures.iter().any(|f| f.contains("`c` is in the table")));
    assert_eq!(check_table("Events", None, &code).len(), 1);
}

#[test]
fn an_equipment_type_missing_from_solite_fails() {
    let mut sowel = tiny_sowel();
    let mut types: BTreeSet<String> = EquipmentType::ALL
        .iter()
        .map(|t| t.as_str().to_string())
        .collect();
    types.insert("teleporter".into());
    sowel.unions.insert("EquipmentType".into(), types);
    let failures = check_all(&sowel);
    assert!(
        failures
            .iter()
            .any(|f| f == "EquipmentType: Sowel's `teleporter` missing in Solite"),
        "{failures:?}"
    );
}

#[test]
fn the_equipment_with_every_optional_field_keeps_them_all() {
    let keys = solite_equipment_keys();
    for field in [
        "energyProfile",
        "requireConfirmation",
        "trackOrderOutcome",
        "meteringParentId",
        "separateSupply",
        "localProduction",
        "family",
        "plugInHybrid",
        "usualVehicleId",
        "solarProfile",
        "invertDirection",
        "timedCommand",
    ] {
        assert!(keys.contains(field), "{field} lost: {keys:?}");
    }
}
