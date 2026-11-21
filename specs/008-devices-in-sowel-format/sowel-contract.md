# Sowel contract reference (for mini-sowel)

Source: `../sowel` (= /Users/marcchachereau/Documents/10_Dev/sowel), read 2026-11-21. Paths are relative to it unless prefixed `../`.
Convention: `T:NNN` = `src/shared/types.ts:NNN`. "(inferred)" marks inference. `types.ts` wins over docs.

## 0. Conventions (IDs, dates, casing)
- Ids: UUID v4 strings, `randomUUID()` (`src/devices/device-manager.ts:194`, `src/equipments/equipment-manager.ts:406`). Exception: root zone id is the fixed `"00000000-0000-0000-0000-000000000001"` (`src/shared/constants.ts:15`, `ROOT_ZONE_ID`); zones created without parentId go under it (`zones.ts:68`).
- Dates: ISO 8601 strings (`createdAt`, `updatedAt`, `lastSeen`, `lastUpdated`, `timestamp`). Exceptions: `sourceTimestamp`, `startedAt`, `expiresAt` (in `equipment.timed_action.armed`) are epoch numbers (T:1797+).
- JSON fields: camelCase everywhere (`deviceId`, `zoneId`, `lastUpdated`). Event `type` strings are dotted lowercase with snake_case parts (`device.status_changed`, `equipment.order.executed`). Enum values are snake_case (`light_state`, `light_toggle`, `light_onoff`).
- Absent optional fields are omitted (TS `?:`); nullable ones are explicit `null` (`zoneId: string|null`, `lastSeen`, `lastUpdated`).

## 1. Types

### DataType, DataCategory (T:5-104)
```ts
export type DataType = "boolean" | "number" | "enum" | "text" | "json";   // T:5
```
`DataCategory` (T:7-104), measurement/state values, open list also contains vendor-specific ones:
motion, temperature, humidity, pressure, luminosity, contact_door, contact_window, light_state, light_brightness, light_color_temp, light_color, shutter_position, lock_state, battery, power, energy, voltage, current, water_leak, smoke, co2, voc, noise, rain, wind, action, gate_state, cover_state, runtime_daily, weather_condition, uv, solar_radiation, setpoint, temperature_outdoor, temperature_device, humidity_outdoor, media_*, appliance_state, solar_state, pool_*, firmware_version, uptime, rssi, language, display_brightness, camera_*, ups_status, battery_runtime, ups_load, operation_mode, ev_*, generic. Fallback is `"generic"`.

`OrderCategory` (T:106-160): light_toggle, set_brightness, set_color_temp, set_color, shutter_move, set_shutter_position, toggle_power, set_setpoint, set_operation_mode, gate_trigger, valve_toggle, ... (relevant for mini: `light_toggle`, `set_brightness`).

### Device, DeviceStatus, DeviceData, DeviceOrder (T:163-257)
```ts
export type DeviceSource = "zigbee2mqtt" | "lora2mqtt" | "tasmota" | ... | "simulator" | (string & {});  // T:163, open
export type DeviceStatus = "online" | "offline" | "unknown";   // T:176  (availability)
export type PowerSource = "battery" | "mains" | "dc" | "unknown";  // T:183
export interface Device {                                      // T:185
  id: string; integrationId: string; sourceDeviceId: string; name: string;
  manufacturer?: string; model?: string; ieeeAddress?: string;
  zoneId: string | null; source: DeviceSource; status: DeviceStatus;
  powerSource: PowerSource; lastSeen: string | null; rawExpose?: unknown;
  createdAt: string; updatedAt: string;
}
export interface DeviceData {                                  // T:225
  id: string; deviceId: string; key: string; type: DataType; category: DataCategory;
  value: unknown; unit?: string; lastUpdated: string | null;
}
export interface DeviceOrder {                                 // T:236
  id: string; deviceId: string; key: string; type: DataType; category?: OrderCategory;
  min?: number; max?: number; enumValues?: string[]; unit?: string;
  valueOn?: OrderWireValue; valueOff?: OrderWireValue;         // wire repr of boolean orders
}
export type OrderWireValue = string | number | boolean;        // T:253
export interface DeviceWithDetails extends Device { data: DeviceData[]; orders: DeviceOrder[]; }  // T:259
```
Note: `DeviceData` has no `enumValues`/`valueOn` in the shared type (they exist in DB rows only). Booleans are normalised to `true`/`false` before storing/emitting (`device-manager.ts:579-600`, spec 150): Z2M `"ON"` -> `true` thanks to `valueOn`.

### Zone, ZoneAggregatedData (T:268-337)
```ts
export interface Zone { id: string; name: string; parentId: string | null; icon?: string;
  description?: string; displayOrder: number; createdAt: string; updatedAt: string; }   // T:268
export interface ZoneWithChildren extends Zone { children: ZoneWithChildren[]; }       // T:279
export interface ZoneAggregatedData {                                                   // T:283
  temperature: number | null; humidity: number | null; luminosity: number | null;
  motion: boolean; motionSensors: number; motionSince: string | null;
  openDoors: number; openWindows: number; waterLeak: boolean; smoke: boolean;
  lightsOn: number; lightsTotal: number; shuttersOpen: number; shuttersTotal: number;
  averageShutterPosition: number | null; doorsOpen: number; doorsTotal: number;
  waterValvesOpen: number; waterValvesTotal: number; waterFlowTotal: number | null;
  powerTotal: number | null; sunrise: string | null; sunset: string | null;
  isDaylight: boolean | null; displaysOnline: number; displaysTotal: number;
  unavailableEquipmentsByCategory: Partial<Record<DataCategory, number>>;
  unavailableEquipmentNamesByCategory: Partial<Record<DataCategory, string[]>>;
}
```
Computation (`src/zones/zone-aggregator.ts`; direct accumulator per zone `:586-625`, per-binding `:776-870`, finalise `:149-181`, parent merge `:117-125`):
- Zone result = own equipments + all descendants (accumulators are added up the tree).
- `temperature`: mean of numeric bindings with category `temperature` AND alias exactly `"temperature"`; rounded to 1 decimal; `null` if none (`:149`, `:788`).
- `humidity`: mean, 1 decimal, any numeric `humidity` binding; `null` if none (`:152`).
- `luminosity`: mean, rounded to integer; `null` if none (`:153`).
- `motion`: OR of all `motion` bindings where value is `true` or `"ON"` (`isBooleanActive`, `:272`); `motionSensors` = count of motion bindings (`:806`).
- `motionSince`: ISO timestamp of last motion true/false transition; `null` when `motionSensors == 0`; kept unchanged while motion state unchanged (`:251-262`).
- `openDoors`/`openWindows`: count of `contact_door`/`contact_window` bindings with value `false` or `"OFF"` (contact "open" = false) (`:276`).
- `waterLeak`/`smoke`: OR of true/"ON".
- `lightsOn`/`lightsTotal`: per equipment of type light_onoff|light_dimmable|light_color that has a `light_state` binding: total+1; on+1 if any channel is true/"ON" (`:855-870`).
- shutters: only type `shutter`, category `shutter_position`; `shuttersOpen` = position>0; `averageShutterPosition` = rounded mean.
- `weather` equipments are excluded from zone aggregation (`:593`). Equipments with derived status `offline` are skipped and listed in `unavailable*` maps (`:610`); `degraded` still count.
- Aggregated data recomputed on device/equipment changes; `zone.data.changed` emitted (`:549`) only when something other than `motionSince` changed (`:200-220`, equality check), whole `ZoneAggregatedData` is sent.

### Equipment, EquipmentType, bindings (T:339-680)
```ts
export type EquipmentType = "light_onoff" | "light_dimmable" | "light_color" | "shutter" | "awning" | "door"
  | "switch" | "sensor" | "button" | "thermostat" | "weather" | "weather_forecast" | "gate" | "heater"
  | "water_heater" | "energy_meter" | "main_energy_meter" | "energy_production_meter" | "solar_panel"
  | "media_player" | "appliance" | "water_valve" | "pool_pump" | "pool_cover" | "pool_heat_pump"
  | "display" | "camera" | "vmc" | "ups" | "ev_charger" | "electric_vehicle";        // T:339-384
export interface Equipment {                                                         // T:419
  id: string; name: string; zoneId: string; type: EquipmentType; icon?: string;
  description?: string; enabled: boolean; createdAt: string; updatedAt: string;
  // + many optional flags (energyProfile, requireConfirmation, trackOrderOutcome, meteringParentId,
  //   separateSupply, localProduction, family, plugInHybrid, usualVehicleId, solarProfile,
  //   invertDirection, timedCommand). All optional; mini can omit them.
}
export type EquipmentStatus = "online" | "degraded" | "offline";                     // T:512, derived, never persisted
export interface EquipmentStatusReason { offlineDevices: string[]; staleBindings: string[]; offlineSince: string | null; } // T:514
export interface DataBinding { id: string; equipmentId: string; deviceDataId: string; alias: string;
  historize?: number | null; }                                                       // T:523
export interface OrderBinding { id: string; equipmentId: string; deviceOrderId: string; alias: string; } // T:576
export interface DataBindingWithValue extends DataBinding {                          // T:583
  deviceId: string; deviceName: string; key: string; type: DataType; category: DataCategory;
  value: unknown; unit?: string; enumValues?: string[]; lastUpdated: string | null;
  lastChanged: string | null; historize?: number | null; stale: boolean; freshnessBudgetMs?: number; }
export interface OrderBindingWithDetails extends OrderBinding {                      // T:613
  deviceId: string; deviceName: string; key: string; type: DataType; category?: OrderCategory;
  min?: number; max?: number; enumValues?: string[]; unit?: string; }
export interface EquipmentWithDetails extends Equipment {                            // T:663
  dataBindings: DataBindingWithValue[]; orderBindings: OrderBindingWithDetails[];
  computedData?: ComputedDataEntry[]; status: EquipmentStatus;
  statusReason?: EquipmentStatusReason; timedAction?: TimedAction; }
```
Binding model: an Equipment has N `DataBinding`s (alias -> DeviceData id) and N `OrderBinding`s (alias -> DeviceOrder id). All consumers (recipes, UI, orders) address an equipment by ALIAS, not device key. `stale` is true only for streaming categories whose `lastUpdated` exceeds a per-category timeout; never for event-like categories (motion, contact_door) (T:583-600, inferred from comment). Equipment status derivation: offline if bound device offline (spec 116, T:505-512).

### Recipe / RecipeInstance (summary) (T:680-870)
- `RecipeSlotDef { id, name, description, type: "zone"|"equipment"|"number"|"duration"|"time"|"boolean"|"text"|"data-key"|"select", required, list?, defaultValue?, options?, hiddenWhen?, constraints?{equipmentType?, min?, max?, crossZone?, includeDescendants?}, group? }` (T:680-717).
- `RecipeInfo { id, name, description, slots, actions?, tile?, i18n?, icon? }` (T:806) = what `GET /recipes` returns.
- `RecipeInstance { id, recipeId, params: Record<string,unknown>, enabled, createdAt }` (T:819).
- `RecipeDefinition` (package contract) adds `validate(params, ctx)` and `createInstance(params, ctx)` returning `{ stop(), onAction? }` (T:845).
- `RecipeLogEntry { id: number, instanceId, timestamp, message, level: "info"|"warn"|"error" }` (T:827).

### Mode (summary) (T:1377)
`Mode { id, name, icon?, description?, active: boolean, createdAt, updatedAt }`. Events `mode.created|updated|removed|activated|deactivated`. Not needed by motion-light v1 (no mode slot).

### EngineEvent (T:1797-2072), variants relevant to devices/equipments/zones
```ts
| { type: "device.discovered"; device: Device }
| { type: "device.removed"; deviceId: string; deviceName: string }
| { type: "device.status_changed"; deviceId: string; deviceName: string; status: DeviceStatus }
| { type: "device.data.updated"; deviceId: string; deviceName: string; dataId: string; key: string;
    value: unknown; previous: unknown; timestamp: string; sourceTimestamp?: number }
| { type: "device.heartbeat"; deviceId: string; timestamp: string }
| { type: "zone.created"; zone: Zone } | { type: "zone.updated"; zone: Zone }
| { type: "zone.removed"; zoneId: string; zoneName: string }
| { type: "zone.data.changed"; zoneId: string; aggregatedData: ZoneAggregatedData }
| { type: "equipment.created"; equipment: Equipment } | { type: "equipment.updated"; equipment: Equipment }
| { type: "equipment.removed"; equipmentId: string; equipmentName: string; zoneId: string }
| { type: "equipment.data.changed"; equipmentId: string; alias: string; value: unknown; previous: unknown; sourceTimestamp?: number }
| { type: "equipment.order.executed"; equipmentId: string; orderAlias: string; value: unknown; source?: OrderSource; startedAt?: number; partOf?: string; composite?: true }
| { type: "equipment.order.failed"; equipmentId: string; orderAlias: string; value: unknown; error: string; source?: OrderSource }
| { type: "equipment.order.unconfirmed"; ...; reason: "timeout"|"device_offline"|"integration_disconnected"|"device_fault" }
| { type: "equipment.order.outcome"; ... }   // gate/door tracking, skip
| { type: "equipment.timed_action.armed|reverted|disarmed|failed"; ... }  // spec 174, skip
| { type: "equipment.status.changed"; equipmentId: string; equipmentName: string; oldStatus: EquipmentStatus; newStatus: EquipmentStatus }
| { type: "recipe.instance.created|removed|started|stopped|state.changed"; instanceId: string; recipeId: string }
| { type: "recipe.instance.error"; instanceId: string; recipeId: string; error: string }
| { type: "system.started" } | { type: "system.integration.connected|disconnected"; integrationId: string } | { type: "system.error"; error: string }
```
(Also mode.*, calendar.*, settings.changed, shared_access.*, energy.*, mqtt-*, notification-*, system.alarm.*, system.update.*, activity.added: out of scope.)
`OrderSource` is at T:1528 (manual/recipe/...; REST orders use `{kind:"manual", userId}`, `equipments.ts:609`).

## 2. REST API
- Prefix `/api/v1` (all routes). OpenAPI JSON at `GET /api/v1/openapi.json` (`src/api/server.ts:341`). Health: public `GET /api/v1/health`.
- Doc: `docs/technical/api-reference.md` (893 lines). Code wins.

### Auth (`src/auth/auth-middleware.ts`, docs api-reference.md:43-55)
- Header `Authorization: Bearer <token>`; token is a JWT access token (from `POST /auth/login` -> `{accessToken, refreshToken}`) or an API token prefixed `swl_` (also `wch_`, `cbl_`) created via `POST /api/v1/me/tokens {name, expiresAt?}` (middleware:60-63; docs:141).
- Required for ALL `/api/` routes except `PUBLIC_ROUTES` (middleware:22-29: auth/status, auth/setup, auth/login, auth/refresh, health, auth/mfa/verify) plus oauth callbacks and `shared-access/public/*`. No "auth off" switch found (grep of `src/config.ts`: none). While no user exists, everything but `/auth/setup` answers `403 {"error":"Setup required","setupRequired":true}` (middleware:205-209).
- Missing/invalid: `401 {"error":"Authentication required"}` / `{"error":"Invalid API token"}` / `{"error":"Invalid or expired token"}` (:212-222). Standard role writing admin-only route: `403 {"error":"Admin access required"}` (middleware:228). Roles: `"admin"|"standard"` (T:1460).
- Implication for mini (inferred): to be Sowel-compatible a client must send a Bearer token; mini can accept a static `swl_...` token or, for local use, skip auth (a divergence to decide).

### Error format
`{ "error": "<message>" }` for every error: 400 (schema validation, normalised: `src/api/error-handler.ts`), 401/403 (above), 404 (`{"error":"Device not found"}`, `"Zone not found"`, `"Equipment not found"`, `"Recipe not found"`), 502 `{"error": <driver error>}` when an order fails (`equipments.ts:621`), 204 empty body on successful DELETE. `EquipmentError`/`ZoneError`/`RecipeError` carry their own `.status`.

### Devices (`src/api/routes/devices.ts`)
No create route: devices are discovered by integrations (event `device.discovered`).
| Method/path | Notes |
|---|---|
| `GET /devices` -> `DeviceWithDetails[]` (:27) | list with `data` + `orders` |
| `GET /devices/:id` -> `DeviceWithDetails` (:38) | 404 if absent |
| `PUT /devices/:id` body `{name?, zoneId?: string\|null}` -> `Device` (:47) | only these two fields |
| `DELETE /devices/:id` -> 204 (:65) | |
| `GET /devices/battery-alerts`, `/devices/suggest?type=`, `/devices/:id/raw` | extras |
Example `GET /devices/:id` (realistic, inferred from types + z2m parser):
```json
{ "id":"6f1c2c1e-3a5b-4a47-9a54-0b0d2a3f4c11","integrationId":"zigbee2mqtt","sourceDeviceId":"salon_th",
  "name":"salon_th","manufacturer":"SONOFF","model":"SNZB-02D","ieeeAddress":"0x00124b0029ab12cd",
  "zoneId":"00000000-0000-0000-0000-000000000001","source":"zigbee2mqtt","status":"online","powerSource":"battery",
  "lastSeen":"2026-11-21T09:12:03.120Z","createdAt":"2026-11-01T10:00:00.000Z","updatedAt":"2026-11-21T09:12:03.120Z",
  "data":[
   {"id":"<uuid>","deviceId":"6f1c...","key":"temperature","type":"number","category":"temperature","value":21.4,"unit":"°C","lastUpdated":"2026-11-21T09:12:03.120Z"},
   {"id":"<uuid>","deviceId":"6f1c...","key":"humidity","type":"number","category":"humidity","value":48.5,"unit":"%","lastUpdated":"2026-11-21T09:12:03.120Z"},
   {"id":"<uuid>","deviceId":"6f1c...","key":"battery","type":"number","category":"battery","value":97,"unit":"%","lastUpdated":"2026-11-21T08:00:00.000Z"}],
  "orders":[] }
```
(`rawExpose` is omitted from the list; `GET /devices/:id/raw` returns it.)

### Zones (`src/api/routes/zones.ts`)
| | |
|---|---|
| `GET /zones` -> `ZoneWithChildren[]` tree (:36) | |
| `GET /zones/aggregation` -> `Record<zoneId, ZoneAggregatedData>` (:41) | |
| `GET /zones/:id` -> `ZoneWithChildren` (:46) | |
| `POST /zones` body `{name, parentId?, icon?, description?, displayOrder?}` -> 201 `Zone` (:55) | `parentId` defaults to ROOT |
| `PUT /zones/:id` body `{name?, parentId?, icon?, description?, displayOrder?}` -> `Zone` (:81) | |
| `DELETE /zones/:id` -> 204 (:108); `PUT /zones/reorder {parentId, orderedIds}` -> 204 (:118) | |
| `POST /zones/:id/orders/:orderKey` body `{value?}` (:131) | zone-wide e.g. `allLightsOff` (docs:315) |

### Equipments (`src/api/routes/equipments.ts`)
| | |
|---|---|
| `GET /equipments[?type=]` -> `EquipmentWithDetails[]` (:221) | unknown type -> `[]` |
| `GET /equipments/:id` -> `EquipmentWithDetails` (:277) | |
| `POST /equipments` body `{name, type, zoneId, icon?, description?, deviceIds?: string[]}` -> 201 `EquipmentWithDetails` (:286) | with `deviceIds`, auto-binds ALL data+orders of each device (see 4) |
| `PUT /equipments/:id` body `{name?, type?, zoneId?, icon?, description?, enabled?, ...flags}` (:329) | |
| `DELETE /equipments/:id` -> 204 (:590) | |
| `POST /equipments/:id/data-bindings` body `{deviceDataId, alias}` -> 201 `DataBinding` (:713); `DELETE .../data-bindings/:bindingId` -> 204 | |
| `POST /equipments/:id/order-bindings` body `{deviceOrderId, alias}` -> 201 `OrderBinding` (:752); `DELETE .../order-bindings/:bindingId` -> 204 | |
| **Send an order**: `POST /equipments/:id/orders/:alias` body `{"value": <any>}` -> `200 {"success":true}`; failure `502 {"error":"..."}` (:600-625) | alias = order-binding alias |
Light order examples: `POST /equipments/<id>/orders/state` body `{"value": true}` (on) / `{"value": false}`; the core maps boolean onto the device's `valueOn`/`valueOff` ("ON"/"OFF") at dispatch (`DeviceOrder.valueOn`, T:236). Recipes use `"state"` + `resolveEnumValue(..., "on")` (git `0086ddf6^:src/recipes/engine/light-helpers.ts`). Brightness: `orders/brightness {"value": 128}`.
Example light/switch equipment (ZBMINIR2 relay, type `light_onoff`), `GET /equipments/:id`:
```json
{ "id":"b7d0...-uuid","name":"Plafonnier salon","zoneId":"<zone-uuid>","type":"light_onoff","enabled":true,
  "createdAt":"2026-11-01T10:05:00.000Z","updatedAt":"2026-11-01T10:05:00.000Z","status":"online",
  "dataBindings":[{"id":"<uuid>","equipmentId":"b7d0...","deviceDataId":"<uuid>","alias":"state","deviceId":"<uuid>",
    "deviceName":"relais_salon","key":"state","type":"boolean","category":"light_state","value":true,
    "lastUpdated":"2026-11-21T09:00:00.000Z","lastChanged":"2026-11-21T09:00:00.000Z","stale":false}],
  "orderBindings":[{"id":"<uuid>","equipmentId":"b7d0...","deviceOrderId":"<uuid>","alias":"state","deviceId":"<uuid>",
    "deviceName":"relais_salon","key":"state","type":"boolean","category":"light_toggle"}] }
```
(Values built from the types; not copied from a live capture: inferred.)

### Recipes (`src/api/routes/recipes.ts`)
`GET /recipes` -> `RecipeInfo[]` (:40); `GET /recipes/:recipeId` (:45); `GET /recipe-instances` -> `RecipeInstance[]` (:54); `POST /recipe-instances` body `{recipeId, params}` -> 201 `RecipeInstance` (:59); `PUT /recipe-instances/:id` body `{params}` (:83); `DELETE /recipe-instances/:id` 204 (:104); `POST .../:id/enable|disable` -> `{success:true}` (:117,:133); `POST .../:id/actions {action, payload?}` -> `{ok:true}` (:149); `GET .../:id/log` (:170).
Example (docs/_legacy/user-manual.md:355): `{"recipeId":"motion-light","params":{"zone":"<zone-id>","lights":["<eq-id-1>"],"timeout":"10m"}}`.

## 3. WebSocket (`src/api/websocket.ts`)
- URL `ws://<host>:3000/ws`. DOC/CODE MISMATCH: docs (api-reference.md:805) say `?token=` query; code (`:20-45`, `:300-315`) accepts ONLY `Authorization: Bearer <t>` header or subprotocol `Sec-WebSocket-Protocol: bearer.<t>`. No token: close 4001 "Authentication required"; bad token: 4001 "Invalid token"; bad Origin: 4003. No Origin header = allowed.
- On connect server sends (single object, not array): `{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}`.
- Subscribe (client -> server): `{"type":"subscribe","topics":["devices","equipments","zones",...]}`. Topics: devices, equipments, zones, modes, recipes, calendar, mqtt-publishers, system, logs, activity, energy. `system` always on (initial subscription = `system` only). Each subscribe REPLACES the set. admin-only: mqtt-publishers, logs. Topic of an event = first dotted segment: `device.*`->devices, `equipment.*`->equipments, `zone.*`->zones, `recipe.*`->recipes, `mode.*`->modes, `calendar.*`, `activity.*`, `energy.*`, everything else (`system.*`, `settings.*`) -> system.
- Server -> client envelope: a JSON ARRAY of EngineEvent objects (the raw event, no wrapper), flushed every 200 ms per client, only if non-empty; within a batch the same key is deduplicated keeping the latest: `device.data.updated` by deviceId+key, `device.status_changed` by device, `equipment.data.changed` by equipmentId+alias, `equipment.status.changed` by equipment, `zone.data.changed` by zone (`:155-175`). Other types are never deduplicated.
- Log lines are sent as single objects `{"type":"log.entry",...}`.
Exact JSON of the events (shape = EngineEvent, T:1797+):
```json
[{"type":"device.data.updated","deviceId":"6f1c...","deviceName":"salon_th","dataId":"<uuid>","key":"temperature","value":21.6,"previous":21.4,"timestamp":"2026-11-21T09:13:00.000Z"},
 {"type":"equipment.data.changed","equipmentId":"b7d0...","alias":"state","value":true,"previous":false},
 {"type":"zone.data.changed","zoneId":"<zone-uuid>","aggregatedData":{"temperature":21.6,"humidity":48.5,"luminosity":null,"motion":false,"motionSensors":1,"motionSince":"2026-11-21T09:00:00.000Z","openDoors":0,"openWindows":0,"waterLeak":false,"smoke":false,"lightsOn":1,"lightsTotal":1, "...":"all ZoneAggregatedData fields"}}]
```
(`equipment.data.changed` carries `alias`, not key; docs example at api-reference.md:841 is outdated: it shows `"zone.data.changed"` with `key/value`; types.ts + code send `aggregatedData`.)

## 4. Zigbee2MQTT mapping (plugin repo `../sowel-plugin-zigbee2mqtt`, `src/z2m-parser.ts`; registry says v2.8.2, `plugins/registry.json:5-17`)
- Plugin is an external package (not built in): it calls core `deviceManager.upsertFromDiscovery`-like API with `{key,type,category,unit,enumValues,valueOn,valueOff}` per expose (`flattenExposes`, parser:587-625). `Device.source = integrationId = "zigbee2mqtt"`, `sourceDeviceId` = Z2M friendly_name, `ieeeAddress` from Z2M.
- DataType from expose type (`Z2M_TYPE_TO_DATA_TYPE`, :58): binary->boolean, numeric->number, enum->enum, text->text, composite->json.
- Key = Z2M `property` verbatim; unit = Z2M `unit` (e.g. `"°C"`, `"%"`, `"lx"`).
- Category from property (`PROPERTY_TO_CATEGORY`, :102-139): occupancy/presence->motion; temperature/device_temperature/soil_temperature->temperature; humidity/soil_moisture->humidity; pressure; illuminance/illuminance_lux->luminosity; battery->battery; battery_low->generic; voltage, current, power, energy; co2, voc; action->action; contact->contact_door; state->light_state; brightness->light_brightness; color_temp->light_color_temp; position->shutter_position; water_leak, smoke, rain, wind, noise; else generic.
- `state`: category `light_state` when under a `light`/`switch` expose group, when light properties coexist, or when top-level binary; also endpoint-suffixed `state_l1`, `state_left`, ... for binary (`inferCategory`, :180-196).
- Orders (writable exposes, access bit 0b010, :610): key = property; category (`inferOrderCategory`, :199-216): `state` -> `light_toggle` (`shutter_move` under cover); brightness->set_brightness; color_temp->set_color_temp; color/color_xy/color_hs->set_color; position->set_shutter_position. `valueOn`/`valueOff` come from Z2M `value_on`/`value_off` (typically "ON"/"OFF"). `min/max` from value_min/value_max.
- Relay ZBMINIR2: data `{key:"state",type:"boolean",category:"light_state"}`, order `{key:"state",type:"boolean",category:"light_toggle",valueOn:"ON",valueOff:"OFF"}`. ("ZBMINIR2" is not mentioned in either repo; mapping derived from generic rules: inferred.)
- Motion sensor: data `{key:"occupancy",type:"boolean",category:"motion"}` (+ `illumination`/`illuminance` -> luminosity, `battery`, `tamper` -> generic). Value `true`/`false` (normalised from Z2M bool).
- Availability: Z2M `bridge/state` and per-device `availability` -> `deviceManager.updateDeviceStatus(integrationId, sourceId, "online"|"offline")` -> `device.status_changed`.
- Equipment creation is MANUAL (or UI-assisted): `POST /equipments {name,type,zoneId,deviceIds}`. Auto-binding (`equipment-manager.ts:428-502`, `createWithAutoBindings`): for each device, EVERY DeviceData becomes a DataBinding with `alias = data.key` and EVERY DeviceOrder an OrderBinding with `alias = order.key` (alias conflicts fall back to key/skip). Exceptions: vmc (`low`/`high`), gate (`command`), contract types (ev_charger, electric_vehicle) use category-derived aliases. So: light -> aliases `state` (+`brightness`...), temp sensor -> `temperature`,`humidity`,`battery`, motion sensor -> `occupancy`, relay -> `state`. A device with no equipment is not usable by recipes/zone aggregation (aggregation reads equipment bindings only: `zone-aggregator.ts:594`). A newly discovered device sits in the root zone (zoneId null or root, inferred) until `PUT /devices/:id {zoneId}` and an equipment is created.
- Zone `temperature` therefore needs an equipment (type `sensor`) whose binding alias is exactly `temperature`.

## 5. Recipe `motion-light`
- Registry entry (`plugins/registry.json:241-260`): id `"motion-light"`, type `recipe`, category `lighting`, name "Motion Light", description "Turns on lights when motion is detected, turns off after a timeout with no motion", repo `mchacher/sowel-recipe-motion-light`, version `1.1.2`, sowelVersion `>=0.10.0`, fr i18n name "Lumière sur mouvement". Sibling: `motion-light-dimmable` (registry:283).
- The repo `sowel-recipe-motion-light` is NOT cloned locally (`ls ..` shows other `sowel-recipe-*` only). Its source was in-repo until spec 054 and is recoverable from git history: `git show 0086ddf6^:src/recipes/motion-light.ts` and `.../engine/motion-light-base.ts` (class `MotionLightRecipe extends MotionLightBase`). Behaviour below is from that pre-externalisation version; v1.1.2 may differ (e.g. release-notes.md:471 and docs/planning/2026-09-01-timed-command.md:52 say motion-light now leans on the spec-174 timed-action primitive instead of its own clock) (inferred: same params).
- Slots (id, type, required, default):
  - `zone` : zone, required (zone to monitor).
  - `lights` : equipment, list, required, constraint `equipmentType: "light_onoff"` (dimmable variant: light_dimmable|light_color); each light must be in the same zone (exactly `equipment.zoneId === zone`) and have an order binding aliased `"state"`.
  - `timeout` : duration, required, default `"10m"`. Duration format `^(\d+)\s*(s|m|h)$` (`engine/duration.ts:13`).
  - `luxThreshold` : number, optional, min 0. Lights not turned on if zone luminosity > threshold; turned off if it rises above threshold x 1.1 (hysteresis `LUX_HYSTERESIS_FACTOR = 0.1`).
  - `maxOnDuration` : duration, optional (failsafe: force off after it even with motion).
  - `buttons` : equipment list, optional, type `button`, each needs an `action` data binding (manual toggle -> override mode).
  - `disableWhenDaylight` : boolean, optional; true -> no turn-on while root zone `isDaylight === true` (null = treated as night).
  - Legacy param `light` (single id) is normalised to `lights: [id]`.
- Behaviour: input = `zone.data.changed` for the zone (`aggregatedData.motion` and `.luminosity`) and `equipment.data.changed` (alias `state` of lights, alias `action` of buttons). Motion true + lights off -> turn on (order alias `state`, value "on"); motion true + lights on -> reset off timer; motion false + lights on -> start off timer(`timeout`); timer expiry -> off. A manual off while motion is active -> override mode (recipe suspended until zone vacancy lasts `timeout`); light turned on externally with no motion -> off timer starts. 5 s grace after own turn-off to ignore echoes. On start/activation: motion && not too bright && not daytime -> on, else off. validate() throws `Error("Zone parameter is required" | "Zone not found: <id>" | "At least one light is required" | ...)` and only logs a warning when the zone has `motionSensors === 0`. No mode condition. Instance persists `overrideMode`, `timerExpiresAt`, `failsafeExpiresAt` in per-instance state.
- Instance JSON: `{"id":"<uuid>","recipeId":"motion-light","params":{"zone":"<id>","lights":["<id>"],"timeout":"10m"},"enabled":true,"createdAt":"<iso>"}`.

## 6. Doc/code divergences found (types.ts wins)
1. WS auth: docs `?token=`; code header or `bearer.<token>` subprotocol only.
2. WS `zone.data.changed`: docs `{key,value}`; code/types `{zoneId, aggregatedData}`.
3. WS `equipment.data.changed` docs example omits `previous` (always sent in types).
4. `PUT /equipments/:id` accepts many more fields than docs list (equipments.ts:329+).
5. Docs say `Device.data` values of Z2M binary are "ON"; the core stores normalised booleans (spec 150), aggregator still accepts `"ON"`.
