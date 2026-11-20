# EZSP 13-19 minimal-coordinator reference (from zigbee-herdsman 11.2.2, ember adapter)
Source root: `tools/z2m-coverage/node_modules/zigbee-herdsman/dist/`. Short names used below:

- `EA` = `adapter/ember/adapter/emberAdapter.js`
- `EZ` = `adapter/ember/ezsp/ezsp.js`
- `BU` = `adapter/ember/ezsp/buffalo.js`
- `EC` = `adapter/ember/ezsp/consts.js`
- `ZE` = `adapter/ember/ezsp/enums.js` (EZSP ids)
- `EN` = `adapter/ember/enums.js` (Ember/SL enums)
- `CN` = `adapter/ember/consts.js`
- `EP` = `adapter/ember/adapter/endpoints.js`
- `ZCL` = `zspec/zcl/...`, `ZDO` = `zspec/zdo/...`

"(inferred)" = not read directly. All multi-byte integers are little-endian (inferred from Buffalo; the IEEE writer emits the low 32 bits first, `BU` buffalo base class `buffalo/buffalo.js:184-190`). EUI64 strings are `0x` + 16 hex chars.

Herdsman supports EZSP 13 (0x0d) to 19 (0x13): `EC` `EZSP_MIN_PROTOCOL_VERSION=0x0d`, `EZSP_PROTOCOL_VERSION=0x13`.

## 0. Wire frame (EZSP inside ASH DATA payload; ASH is out of scope)
- **Frame IDs are version-independent in herdsman**: a single `EzspFrameID` enum (`ZE:5`). None of the IDs below differ between 13 and 19. Version differences are only in field layouts and status widths (section 6).
- **Legacy format** is used only for the very first VERSION command: `[seq][fc=0x00][frameId=0x00][desiredVersion]` (`EZ:282-296`, `323-337`: `initialVersionSent`).
- **Extended format**, every other command (`EZ:290-296`, `332-337`):
  - byte0 = sequence (incremented mod 256)
  - byte1 = frame control low = `0x00` command | sleep mode IDLE(0) | network index 0
  - byte2 = frame control high = `0x01` (format version 1; padding/security bits 0)
  - byte3 = frameId low, byte4 = frameId high
  - byte5.. = parameters (`EC` `EZSP_EXTENDED_PARAMETERS_INDEX=5`)
- Response (and callback) header is parsed the same way. If `byte2 & 0x03 == 1` it is extended, else legacy, params at index 3 (`BU:100-127`).
- **Frame control low bits** (`EC`):
  - 0x80 = response, 0x00 = command
  - 0x60 = network index
  - 0x03 = sleep mode
  - 0x01 = overflow (ignored, mapped to success)
  - 0x02 = truncated (error)
  - 0x04 = pending callback (sets `ash.ncpHasCallbacks`)
  - 0x08 = synchronous callback
  - **0x10 = asynchronous callback**
  - Callback frames: `byte1 & 0x10` (`EZ:215`), checked on the raw header byte 1.
- **Callbacks are routed by this flag, not by frame id** (`EZ:206-245`):
  - ASYNCH_CB set: copied to a separate callback buffer and dispatched immediately (`callbackDispatch`, `EZ:423`). This happens even while a command awaits its response.
  - Otherwise: it is the response to the in-flight command (`responseWaiter`).
  - Herdsman never sends CALLBACK(6)/NO_CALLBACKS(7), so the NCP is in async-callback mode by default (inferred).
- **Max frame length** = `218+1+1 = 220` bytes total (`EC` `EZSP_MAX_FRAME_LENGTH`). Longer commands fail `ERROR_COMMAND_TOO_LONG` (`EZ:345`). `MAXIMUM_APS_PAYLOAD_LENGTH = 82 - 0` (`CN:49`, source-routing reserve is 0 there).
- **Invalid command**: frame id `INVALID_COMMAND` (88) returns an EzspStatus byte at param[0] (`EZ:385-392`).
- **Response timeout**: `ash.responseTimeout` (`EZ:362`, `ash.js:223`). Commands are serialised by `this.queue` (one in flight, `EZ:353`).

## 1. Start-up sequence (`EA initEzsp`, `EA:502-565`)
Order, in `EA:505-557`:

| # | Call (EZ wrapper) | Frame (dec / hex) | Params | Essential? |
|---|---|---|---|---|
| 1 | `ezsp.start()` = ASH reset + connect (`EZ:155-170`) | n/a | n/a | yes |
| 2 | `ezspVersion(0x13)` (`EA:1056-1094`) | VERSION 0 / 0x00 | u8 desiredVersion (first call legacy format). Resp: u8 protocolVersion, u8 stackType, u16 stackVersion (`EZ:1162-1173`) | yes |
| 2b | If NCP version < 0x13 and >= 0x0d: `ezspVersion(ncpVer)` again (extended format now), then `setProtocolVersion(ncpVer)` (`EA:1067-1075`) | 0 / 0x00 | as above | yes |
| 2c | Stack type must be `0x02` (`EZSP_STACK_TYPE_MESH`) else abort (`EA:1061`) | | | yes |
| 2d | `ezspGetVersionStruct` = GET_VALUE VERSION_INFO(0x11), 7 bytes: u16 build, u8 major, minor, patch, special, type (`EZ:966-985`) | GET_VALUE 170 / 0xAA | u8 valueId | optional (logging) |
| 3 | setConfigurationValue TRUST_CENTER_ADDRESS_CACHE_SIZE (0x19) = 2 (`EA:512`) | SET_CONFIGURATION_VALUE 83 / 0x53 | u8 id, u16 value | recommended (needed for TC rejoin across hops per AN1233, `EA:553-567`) |
| 4 | INDIRECT_TRANSMISSION_TIMEOUT (0x12) = 7680 (`EA:514`) | 0x53 | | optional (firmware default) |
| 5 | MAX_HOPS (0x10) = 30 (`EA:516`) | 0x53 | | optional |
| 6 | SUPPORTED_NETWORKS (0x2D) = 1 (`EA:517`) | 0x53 | | optional |
| 7 | setPolicy BINDING_MODIFICATION_POLICY (1) = 0x12 `CHECK_BINDING_MODIFICATIONS_ARE_VALID_ENDPOINT_CLUSTERS` (`EA:519`) | SET_POLICY 85 / 0x55 | u8 policyId, u8 decisionId | optional |
| 8 | setPolicy MESSAGE_CONTENTS_IN_CALLBACK_POLICY (4) = 0x40 `MESSAGE_TAG_ONLY_IN_CALLBACK` (`EA:521`) | 0x55 | | **recommended**: messageSentHandler then carries no payload; herdsman's parser does not read it (`EZ:530-538` comment) |
| 9 | setValue TRANSIENT_DEVICE_TIMEOUT (0x43), len 2, value = 10000 ms as LE bytes (`EA:522`) | SET_VALUE 171 / 0xAB | u8 valueId, u8 len, bytes | optional |
| 10 | `ezspSetManufacturerCode(0x1049)` (SILICON_LABORATORIES, `EA:523`, `EA:113`) | SET_MANUFACTURER_CODE 21 / 0x15 | u16 code; **response ignored** (no read, `EZ:2138-2146`) | optional |
| 11 | STACK_PROFILE (0x0C) = 2 (`CN:83`), SECURITY_LEVEL (0x0D) = 5 (`CN:84`) (`EA:526-527`) | 0x53 | | **essential** (Zigbee PRO, security level 5) |
| 12 | MAX_END_DEVICE_CHILDREN (0x11) = 32, END_DEVICE_POLL_TIMEOUT (0x13) = 8, TRANSIENT_KEY_TIMEOUT_S (0x36) = 300 (`EA:529-531`, defaults `EA:104-117`) | 0x53 | | recommended (the transient key is what lets a device join with ZigBeeAlliance09, section 3). The poll timeout 8 is an index into the NCP's poll-timeout table (inferred) |
| 13 | setValue CCA_THRESHOLD (0x15), len 1, [0] (`EA:533`) | 0xAB | | optional workaround |
| 14 | `registerFixedEndpoints` (`EA:587-623`) | see below | | **essential** (endpoint 1) |
| 15 | `initTrustCenter` = policies + networkInit + form/resume (`EA:625-780`) | | | essential |
| 16 | `initNCPConcentrator` (`EA:576-583`) | | | **essential** for a TC with routers (section 7) |
| 17 | `ezspGetNetworkParameters`, `ezspSetRadioPower` (only if the user configured power), `ezspGetEui64` (`EA:545-557`) | | | optional / cache |

Note the file's own warning (`EA:535`): after step 13, do not call anything that resizes NCP memory tables. Config values can only be set before the network is up; once NETWORK_UP, SET_CONFIGURATION_VALUE returns invalid call (`EZ:1196-1206`).

All SET_CONFIG / SET_POLICY / SET_VALUE statuses are read as `readStatus(version, false)` (EzspStatus-style, `EZ:1217`, `1335`, `1452`): u8 on v13, u32 on v14+. On failure herdsman only logs ("Firmware value will be used instead", `EA:1103-1106`).

### Endpoint registration (`EA:587-623`, `EP:12-46`, `EZ:1301-1319`)
1. `ezspGetEndpointFlags(1)` = GET_EXTENDED_VALUE (frame 3, `ZE`), params: u8 valueId = ENDPOINT_FLAGS (0), u32 characteristics = endpoint number; response: status, u8 len, bytes (`EZ:1000-1007`, `1414-1430`). If status is not OK, the endpoint is not yet registered.
2. `ezspAddEndpoint` = ADD_ENDPOINT (2 / 0x02), params in order (`EZ:1301-1319`):
   - u8 endpoint = 1
   - u16 profileId = 0x0104 (`HA_PROFILE_ID`, `zspec/consts.js:11`)
   - u16 deviceId = 0x0065 (`EP:15`; the comment there is "?")
   - u8 deviceVersion = 1
   - u8 inCount, u8 outCount
   - u16[] inClusters, u16[] outClusters
   - Response: status (u8 on v13, u32 on v14+).
3. Endpoint 1 input clusters: 0x0000, 0x0003, 0x0006, 0x0008, 0x000A, 0x0019, 0x0300.
4. Endpoint 1 output clusters: 0x0000, 0x0003, 0x0004, 0x0005, 0x0006, 0x0008, 0x0020, 0x0300, 0x0400, 0x0402, 0x0405, 0x0406, 0x0500, 0x0702, 0x0B01, 0x0B03, 0x0B04, 0x1000 (`EP:23-45`).
5. Herdsman also registers a Green Power endpoint 0xF2, profile 0xA1E0, device 0x66 (`EP:48-`). Not needed for a minimal coordinator.
6. Multicast entries 0 and 901 are set with SET_MULTICAST_TABLE_ENTRY (100 / 0x64) (`EA:603-620`). Optional.
7. A minimal coordinator that only does unicast needs just endpoint 1 with Basic (0x0000) in the output cluster list. Inferred: so that ZCL replies from devices addressed to endpoint 1 are accepted.

## 2. Resume or form
### Trust-centre policies set before networkInit (`EA:625-640`)
- setPolicy TC_KEY_REQUEST_POLICY (5) = 0x51 `ALLOW_TC_KEY_REQUESTS_AND_SEND_CURRENT_KEY`.
- setPolicy APP_KEY_REQUEST_POLICY (6) = 0x60 `DENY_APP_KEY_REQUESTS`.
- setPolicy TRUST_CENTER_POLICY (0) = bitmask `ALLOW_JOINS(0x01) | ALLOW_UNSECURED_REJOINS(0x02)` = **0x03** (`emberSetJoinPolicy(USE_PRECONFIGURED_KEY)`, `EA:1135-1155`).
- Other TRUST_CENTER_POLICY bits (`ZE` `EzspDecisionBitmask`): 0x04 SEND_KEY_IN_CLEAR, 0x08 IGNORE_UNSECURED_REJOINS, 0x10 JOINS_USE_INSTALL_CODE_KEY, 0x20 DEFER_JOINS.
- Rejoin-only: 0x02 (`ALLOW_REJOINS_ONLY`), plain 0x00 (default) for other values.

### networkInit and the decision (`EA:644-780`)
- Frame NETWORK_INIT 23 / 0x17. Param: struct = **u16 bitmask**, herdsman sends `PARENT_INFO_IN_TOKEN(0x01) | END_DEVICE_REJOIN_ON_REBOOT(0x02)` = **0x0003** (`EA:645-648`, `BU:565-567`). The struct is the same for 13 to 19.
- Response: status (u8 EmberStatus on v13 mapped to SL, u32 SLStatus on v14+; `EZ:2194-2203`).
  - `OK` (0): a network is stored in the NCP tokens. Herdsman then waits for a **STACK_STATUS NETWORK_UP** callback (10 s, `EA:656`).
  - `NOT_JOINED` (SL 0x17; Ember 0x93): no stored network, so form.
  - Anything else: fatal error.
- If `OK`, it checks the stored network matches the config (`EA:657-680`):
  - GET_NETWORK_PARAMETERS gives nodeType == COORDINATOR(1), panId == config, extendedPanId == config.
  - Then EXPORT_KEY (context coreKeyType NETWORK, keyIndex 0) key == config network key.
  - If not matching: LEAVE_NETWORK (32 / 0x20; the `options` u8 param is sent only for v14+, `EZ:2406-2415`), wait for NETWORK_DOWN, wait 200 ms, then form.
- If the network matches: "resumed", done. Nothing more is sent (no re-form, no security state call).
- **A minimal coordinator** (inferred): call networkInit. If it returns OK, wait for NETWORK_UP and keep going. If NOT_JOINED, run the form sequence below. Herdsman additionally supports restoring from a JSON backup (FORM_BACKUP, `EA:715-760`), skippable.

### stackStatusHandler (callback)
- Frame STACK_STATUS_HANDLER **25 / 0x19** (`EZ:456-460`). Payload = one status. v13 = u8 EmberStatus (mapped), v14+ = **u32 SLStatus** (`BU:1055-1073`).
- Values (SLStatus, `EN` `SLStatus`; Ember in brackets for v13):
  - NETWORK_UP = 0x15 [0x90]
  - NETWORK_DOWN = 0x16 [0x91]
  - NOT_JOINED = 0x17 [0x93]
  - ZIGBEE_NETWORK_OPENED = 0x0C18 [0x9C]
  - ZIGBEE_NETWORK_CLOSED = 0x0C19 [0x9D]
  - ZIGBEE_CHANNEL_CHANGED = 0x0C17 [0x9B]
  - ZIGBEE_PAN_ID_CHANGED = 0x0C16 [0x9A]
  - ZIGBEE_NODE_ID_CHANGED = 0x0C07 [0x99]
- Handling (`EA:262-310`): NETWORK_UP/DOWN/OPENED/CLOSED resolve waiters. On NETWORK_CLOSED it resets the manufacturer code to 0x1049 (a workaround). CHANNEL_CHANGED invalidates the channel cache. Others are logged only. Every status clears the network-parameter cache.
- Full v13 EmberStatus to SLStatus table: `BU:14-60`.

### Form sequence (`EA:795-852`)
1. (backup only: SET_VALUE NWK_FRAME_COUNTER 0x23, 4 bytes LE.)
2. **SET_INITIAL_SECURITY_STATE** 104 / 0x68 (`EZ:4855-4863`). The state struct (`BU:405-411`) = in order:
   - u16 bitmask
   - u8[16] preconfiguredKey
   - u8[16] networkKey
   - u8 networkKeySequenceNumber
   - u8[8] preconfiguredTrustCenterEui64
   - Total 43 bytes. The layout is identical for 13..19.
   - Response: status (u8/u32).
3. Herdsman values (`EA:796-805`):
   - bitmask = `TRUST_CENTER_GLOBAL_LINK_KEY(0x0004) | HAVE_PRECONFIGURED_KEY(0x0100) | HAVE_NETWORK_KEY(0x0200) | TRUST_CENTER_USES_HASHED_LINK_KEY(0x0084) | REQUIRE_ENCRYPTED_KEY(0x0800)`.
   - That is **0x0B84** (note `TRUST_CENTER_USES_HASHED_LINK_KEY` is 0x84, i.e. it also contains 0x04; `EN:681`).
   - `preconfiguredKey` = **16 random bytes** (`EA:763`, `randomBytes`). In hashed mode it is the TC root key from which the NCP derives per-device link keys (`EN:676-681`). It is NOT ZigBeeAlliance09.
   - `networkKey` = the configured 16-byte network key (herdsman gets it from the app config, which is random-generated at first run by Z2M; herdsman does not generate it, inferred).
   - networkKeySequenceNumber = 0 (when forming from config).
   - preconfiguredTrustCenterEui64 = `ffffffffffffffff` (`BLANK_EUI64`).
4. **SET_VALUE EXTENDED_SECURITY_BITMASK** (valueId 0x0A, len 2, LE u16) = `JOINER_GLOBAL_LINK_KEY(0x0010) | NWK_LEAVE_REQUEST_NOT_ALLOWED(0x0100)` = **0x0110** (`EA:822-826`, `EZ:1069-1071`, there is no dedicated frame).
5. CLEAR_KEY_TABLE 177 / 0xB1 (only if not from backup; failure only logged) (`EA:828-832`, `EZ:5077-5085`).
6. **FORM_NETWORK** 30 / 0x1E (`EZ:2338-2347`). Param struct `EmberNetworkParameters` (`BU:162-171`), 20 bytes:
   - u8[8] extendedPanId
   - u16 panId
   - u8 radioTxPower
   - u8 radioChannel
   - u8 joinMethod
   - u16 nwkManagerId
   - u8 nwkUpdateId
   - u32 channels
   - Herdsman values (`EA:834-843`): radioTxPower 5 (dBm), joinMethod `MAC_ASSOCIATION`(0), nwkManagerId 0x0000, nwkUpdateId 0, channels `0x07FFF800` (all 11..26, `zspec/consts.js:36`), radioChannel = first configured channel, panId/extendedPanId from config (Z2M generates random ones).
   - Response: status (u8/u32). Note: it is the same struct for 13..19.
7. Wait for STACK_STATUS NETWORK_UP (10 s), then `SET_VALUE STACK_TOKEN_WRITING` (valueId 0x07, len 1, [1]) so the stack persists tokens (`EA:848-851`, `EZ:1087-1089`). Do not skip: otherwise the network is not persisted (inferred from the name).
8. **No key import in this path.** Herdsman uses SET_INITIAL_SECURITY_STATE on all versions, with `EXPORT_KEY` (276 / 0x114, v14+: response `u32 status, key[16], context`; v13: `key, u32 status`, `EZ:4930-4950`) only to compare the network key. IMPORT_KEY (277 / 0x115) is used for backup restore (`EA:916-948`) and for link-key import (not needed).
   - Secure EZSP (encrypted EZSP frames, extended frame control 0x80 SECURE) is NOT used: herdsman always sends frame control high = 0x01 (`EZ:337`).

### getNetworkParameters (`EZ:2516-2525`)
- GET_NETWORK_PARAMETERS 40 / 0x28, no param. Response: status (u8 v13 / u32 v14+), **u8 nodeType**, then the 20-byte `EmberNetworkParameters` above (`BU:172-195`).
- `nodeType`: 0 unknown, 1 COORDINATOR, 2 ROUTER, 3 END_DEVICE, 4 SLEEPY_END_DEVICE (`EN` `EmberNodeType`).
- GET_EUI64 38 / 0x26 (no params, response is 8 bytes LE EUI64); GET_NODE_ID 39 / 0x27 (u16).

## 3. Permit joining (`EA:1385-1435`)
Herdsman, when `seconds > 0` (`EA:1388-1398`):

1. **IMPORT_TRANSIENT_KEY** 273 / 0x111 (`EZ:5389-5405`): params `u8[8] eui64 = ffffffffffffffff` (wildcard), `u8[16] key`, and **v13 only: a trailing u8 `flags` (0)**. Key = `5a 69 67 42 65 65 41 6c 6c 69 61 6e 63 65 30 39` = "ZigBeeAlliance09" (`CN:86-88`). Response: u32 status (read as u32 on all versions).
2. setPolicy TRUST_CENTER_POLICY (0) = 0x03 (ALLOW_JOINS | ALLOW_UNSECURED_REJOINS).
3. **PERMIT_JOINING** 34 / 0x22 (`EZ:2463-2472`), param u8 duration (seconds; 0xFF = forever, `zspec/consts` `PERMIT_JOIN_FOREVER`). Response: status (u8/u32).
4. Then a ZDO Mgmt_Permit_Joining_req broadcast (cluster 0x0036, to 0xFFFC, payload `[tsn][duration u8][tc_significance u8 = 1]`, no response awaited, `EA:1430-1433`). Via SEND_BROADCAST (section 5). Optional if no routers.

When `seconds == 0` (`EA:1399-1405`): CLEAR_TRANSIENT_LINK_KEYS (107 / 0x6B, empty response, awaited: `ezsp.js:5153`) then TRUST_CENTER_POLICY = 0x02 (rejoins only).

Minimal coordinator: steps 1 + 2 + 3 are what make a fresh router/end device with the well-known key join. Step 2 is already in effect from the init (the init sets 0x03). `TRANSIENT_KEY_TIMEOUT_S` (config 0x36) sets how long the transient key remains valid (300 s).

## 4. Join detection
### trustCenterJoinHandler (callback)
- Frame **TRUST_CENTER_JOIN_HANDLER 36 / 0x24** (`EZ:698-706`). Layout, the same for 13..19 (statuses are u8 here, not SLStatus):
  - u16 newNodeId
  - u8[8] newNodeEui64 (LE)
  - u8 status (`EmberDeviceUpdate`)
  - u8 policyDecision (`EmberJoinDecision`)
  - u16 parentOfNewNodeId
- `EmberDeviceUpdate` (`EN`): 0 STANDARD_SECURITY_SECURED_REJOIN, 1 STANDARD_SECURITY_UNSECURED_JOIN, 2 DEVICE_LEFT, 3 STANDARD_SECURITY_UNSECURED_REJOIN.
- `EmberJoinDecision`: 0 USE_PRECONFIGURED_KEY, 1 SEND_KEY_IN_THE_CLEAR, 2 DENY_JOIN, 3 NO_ACTION, 4 ALLOW_REJOINS_ONLY.
- Herdsman (`EA:456-487`): status==2 means device left. Else if decision != 2 means joined. It emits `deviceJoined{networkAddress, ieeeAddr}`. A `DENY_JOIN` is only logged.

### childJoinHandler (callback, not used for the decision)
- Frame **CHILD_JOIN_HANDLER 35 / 0x23** (`EZ:486-494`): u8 index, u8 joining(bool), u16 childId, u8[8] childEui64, u8 childType. Fired only for the coordinator's direct children. Herdsman only logs it (`EZ:2482`). The trustCenterJoin callback is the one to use (fires for any join through any router).

### ZDO End_Device_annce (cluster 0x0013)
- Arrives as INCOMING_MESSAGE_HANDLER with `apsFrame.profileId == 0x0000` (`EZ:4216-4228`: emitted as a "zdoResponse"). Herdsman then parses the message (`ZDO` buffaloZdo `readResponse(hasZdoMessageOverhead=true, clusterId, bytes)`, `EA:378-395`).
- ZDO message = `[u8 tsn][payload]` (`ZDO_MESSAGE_OVERHEAD = 1`, `zspec/zdo/definition/consts.js:9`).
- Device_annce payload (`buffaloZdo.js:1466-1473`): u16 nwkAddress, u8[8] eui64, u8 macCapabilities. The cluster is named `END_DEVICE_ANNOUNCE` = 19 (0x0013). In the controller, `onDeviceAnnounce` (`controller.js:532-549`) updates the nwk address of a known device. The *interview* is NOT started from the announce; it is started after `deviceJoined` (`controller.js:609-`; `Device.interview()` `device.js:691`, called in the join handler, inferred).
- MAC capability bits (inferred, 802.15.4): bit0 alt PAN coord, bit1 FFD, bit2 mains power, bit3 rx-on-when-idle, bit6 security, bit7 allocate address. An SNZB-02 end device will have rx-on-when-idle = 0 (sleepy).

### incomingMessageHandler (callback) — layouts per version
Frame **INCOMING_MESSAGE_HANDLER 69 / 0x45** (`EZ:550-580`). Common: first u8 type (`EmberIncomingMessageType`: 0 UNICAST, 1 UNICAST_REPLY, 2 MULTICAST, 3 MULTICAST_LOOPBACK, 4 BROADCAST, 5 BROADCAST_LOOPBACK).

| Field order | EZSP 13 (< 0x0e) (`EZ:551-571`) | EZSP 14 to 19 (>= 0x0e) (`EZ:572-579`) |
|---|---|---|
| 1 | u8 type | u8 type |
| 2 | ApsFrame (see below) | ApsFrame (see below) |
| 3 | u8 lastHopLqi | **EmberRxPacketInfo (18 bytes (2+8+1+1+1+1+4, herdsman `readEmberRxPacketInfo`))**: u16 senderShortId, u8[8] senderLongId (EUI64), u8 bindingIndex, u8 addressIndex, u8 lastHopLqi, i8 lastHopRssi, u32 lastHopTimestamp (`BU:1113-1131`) |
| 4 | i8 lastHopRssi | (inside RxPacketInfo) |
| 5 | u16 senderShortId | (inside RxPacketInfo) |
| 6 | u8 bindingIndex | (inside RxPacketInfo) |
| 7 | u8 addressIndex | (inside RxPacketInfo) |
| last | u8 length + bytes (`writePayload` style: `readPayload` `BU:158-161`) | u8 length + bytes |

- **No difference between 14 and 19** in the layout that herdsman reads. The only 14+ vs 13 difference here is the RxPacketInfo struct, and in 13 senderLongId is unknown (herdsman sets BLANK).
- **ApsFrame struct** (`BU:203-225`, 11 bytes, identical for all versions): u16 profileId, u16 clusterId, u8 sourceEndpoint, u8 destinationEndpoint, u16 options, u16 groupId, u8 sequence. (The `radius` is NOT part of the struct; it is a separate param on send, `BU:211` comment.)
- For incoming ZDO: `apsFrame.clusterId` is the ZDO cluster (e.g. 0x0013 annce, 0x8005 active-ep response), `sourceEndpoint` = 0.
- Dispatch (`EZ:4216-4235`):
  - profile 0x0000: ZDO.
  - profile 0x0104 (HA), 0xFFFF (wildcard) or 0xA1E0 GP (not on BROADCAST_LOOPBACK) etc: ZCL.
  - Other profiles are dropped.

## 5. Reading the Basic cluster
### Flow herdsman uses for a new device (`device.js:801-1010`)
1. NODE_DESCRIPTOR_REQUEST (ZDO 0x0002): up to 6 tries. Payload `[tsn][u16 nwkAddrOfInterest]` (`buildNodeDescriptorRequest(target)`, `buffaloZdo.js:734`). The response (0x8002) gives logical type and manufacturer code.
2. ACTIVE_ENDPOINTS_REQUEST (ZDO 0x0005, up to 2 tries): `[tsn][u16 nwkAddr]` (`buffaloZdo.js:770-773`). Response 0x8005: `[tsn][u8 status][u16 nwkAddr][u8 count][u8 eps..]` (`buffaloZdo.js:1430-1445`).
3. SIMPLE_DESCRIPTOR_REQUEST (0x0004) per endpoint: `[tsn][u16 nwkAddr][u8 endpoint]` (`buffaloZdo.js:761-765`). Response 0x8004: `[tsn][u8 status][u16 nwkAddr][u8 len][u8 ep][u16 profile][u16 device][u8 devVer][u8 inCount][u16[] in][u8 outCount][u16[] out]` (`buffaloZdo.js:1398-1428`).
4. Read genBasic attributes one at a time: `modelId`(0x0005), `manufacturerName`(0x0004), `powerSource`, `zclVersion`, `appVersion`, `stackVersion`, `hwVersion`, `dateCode`, `swBuildId` (`device.js:53-63`, loop `device.js:864-905`). For modelId/manufacturerName, a failure is retried once after 10 s.
5. For devices with manufacturerID 0 or 4098 (probable Tuya): the endpoint is **assumed to be 1** and Basic is read immediately (`device.js:826-844`).
- **Endpoint 1 default**: herdsman uses endpoint 1 only in that Tuya shortcut; otherwise it relies on Active_EP. For an SNZB-02 endpoint 1 is the expected endpoint but herdsman has no hard-coded rule (inferred, safe default to try first, fall back to Active_EP).

### ZDO request over EZSP (`EA:1336-1383`)
- ZDO request = SEND_UNICAST with `type=DIRECT(0)`, destination = nwk address, apsFrame{profileId 0, clusterId, sourceEndpoint 0, destinationEndpoint 0, options `0x1140`, groupId 0, sequence 0 (stack fills)}, `messageTag` = ZDO sequence, payload `[tsn=messageTag][payload]`.
- `options 0x1140` = `RETRY(0x0040) | ENABLE_ROUTE_DISCOVERY(0x0100) | ENABLE_ADDRESS_DISCOVERY(0x1000)` (`EA:130`, `EN` `EmberApsOption`).
- The ZDO sequence is `++seq & 0x7F` (high bit left to the stack).
- Response matched by (sender nwk addr, response cluster id = request | 0x8000). Timeout 15 s (`EA:132`).

### SEND_UNICAST (`EZ:3831-3851`)
Frame **SEND_UNICAST 52 / 0x34**. Params in order:

| Field | EZSP 13 | EZSP 14 to 19 |
|---|---|---|
| u8 type (`EmberOutgoingMessageType`: 0 DIRECT, 1 VIA_ADDRESS_TABLE, 2 VIA_BINDING, 3 MULTICAST, 4 MULTICAST_WITH_ALIAS, 5 BROADCAST_WITH_ALIAS, 6 BROADCAST) | yes | yes |
| u16 indexOrDestination (nwk address for DIRECT) | yes | yes |
| ApsFrame (11 bytes) | yes | yes |
| messageTag | **u8** | **u16** |
| u8 length + message bytes | yes | yes |

Response: status (u8 v13 mapped / u32 SLStatus v14+), **u8 apsSequence** (`EZ:3852-3858`).

Herdsman's `ezsp.send` wrapper (`EZ:916-962`) generates `messageTag` itself (`nextSendSequence`, `EZ:894`) and for DIRECT uses SEND_UNICAST. `type=DIRECT` needs an entry via the address table cache or `ENABLE_ADDRESS_DISCOVERY` (`EA:130`).

### ZCL Read Attributes frame bytes
Herdsman builds (`endpoint.js:380-407`, `zclFrame.toBuffer`, `zclHeader.js:44-56`):

- Frame control byte = `frameType`(bits0-1: GLOBAL=0, SPECIFIC=1; `ZCL enums.js:159`) | manufSpecific(bit2) | direction(bit3: CLIENT_TO_SERVER=0) | disableDefaultResponse(bit4=0x10) | reserved(bits5-7).
- Herdsman's `read()` uses `disableDefaultResponse = true` (`endpoint.js:383`): frame control = **0x10** (global, client to server, DDR on).
- Then u8 transaction sequence number (herdsman global counter). Then u8 command id **0x00** (Read Attributes, `foundation.js:10-26`). Then u16 attribute ids, repeated.
- Read of manufacturerName + modelId in one frame: `10 <tsn> 00 04 00 05 00` (inferred byte string from the above; herdsman reads attributes one per request).
- Manufacturer-specific only if a manufacturer code is set (header + 2 bytes). Not for Basic.
- APS frame for the ZCL send (`EA:1445-1453`): profileId 0x0104, clusterId 0x0000, sourceEndpoint 1 (`FIXED_ENDPOINTS[0]`), destinationEndpoint = device endpoint, options = `0x1140` (RETRY cleared if no response expected, `EA:1454-1456`), groupId 0, sequence 0.
- Sent through `ezsp.send(DIRECT, nwkAddr, apsFrame, data, 0, 0)`. Retry on BUSY / MAX_MESSAGE_LIMIT_REACHED up to 3 attempts, waits 500 ms; on NETWORK_DOWN waits 1500 ms (`EA:93-95`, `1464-1500`).
- **End devices (sleepy)**: the NCP parent buffers the frame until the child polls. `INDIRECT_TRANSMISSION_TIMEOUT` is 7.68 s (`EA:514`). So send right after join/announce while the device is awake, and allow a long timeout (herdsman default 10 s per ZCL command, `endpoint.js:686`; 15 s for ZDO).

### messageSentHandler (callback)
Frame **MESSAGE_SENT_HANDLER 63 / 0x3F** (`EZ:517-540`):

| Field order | EZSP 13 | EZSP 14 to 19 |
|---|---|---|
| 1 | u8 type | **u32 status** (SLStatus) |
| 2 | u16 indexOrDestination | u8 type |
| 3 | ApsFrame | u16 indexOrDestination |
| 4 | **u8** messageTag | ApsFrame |
| 5 | **u8** status (EmberStatus mapped) | **u16** messageTag |
| 6 | (message contents not parsed when policy MESSAGE_TAG_ONLY) | same |

- Status that matter (`EA:315-370`): OK (0), `ZIGBEE_DELIVERY_FAILED` (SL 0x0C02, Ember 0x66: no APS ACK), others logged.
- On delivery failure herdsman rejects the pending waiter for that destination early (`oneWaitress.deliveryFailedFor`, `EA:324-340`). For broadcast and multicast it only logs.
- **Ordering warning** (`EA:305-309`): the messageSent callback may arrive after the incomingMessage response ("some devices mix it up"). Do not require messageSent before matching a response.
- With MESSAGE_TAG_ONLY_IN_CALLBACK (policy, section 1), the contents are absent. Without it, a payload (u8 len + bytes) follows (inferred, `EZ:530` comment).

### Read Attributes Response arrival
- INCOMING_MESSAGE_HANDLER with apsFrame profile 0x0104, cluster 0x0000, sourceEndpoint = device endpoint, destinationEndpoint = 1. Payload = ZCL frame (`EA:398-420`: `Zcl.Header.fromBuffer(messageContents)`).
- ZCL header: u8 frame control (global, **direction SERVER_TO_CLIENT = bit3 set**), [u16 manuf code if bit2], u8 tsn (same as the request), u8 commandId = **0x01** `readRsp` (`foundation.js:32-60`).
- Payload records, repeated (`foundation.js:35-47`): u16 attrId, u8 status (0 = SUCCESS), and when SUCCESS: u8 dataType, then the data. Status 0x86 UNSUPPORTED_ATTRIBUTE has no type/data.
- Data type **0x42 = CHAR_STR** (`ZCL enums.js:94`): `u8 length` + `length` bytes (`buffaloZcl.js:980`). 0x41 is OCTET_STR and 0x44 LONG_CHAR_STR (u16 length).
- Matching (`oneWaitress.js:99-118`): (sender nwk addr, APS cluster, zcl frame type, tsn, command id = response id).
- A sleepy device may also send a Default Response (cmd 0x0B) if the request lacks DDR; herdsman sets DDR so only readRsp is expected.

## 6. Frame IDs (decimal / hex), all versions 13..19 unless a note says otherwise
Source: `ZE` `EzspFrameID` (`ZE:5`), verified by running the enum.

| Name | Dec | Hex | Notes |
|---|---|---|---|
| VERSION | 0 | 0x00 | first call legacy format |
| ADD_ENDPOINT | 2 | 0x02 | |
| GET_EXTENDED_VALUE | 3 | 0x03 | used for endpoint flags |
| CALLBACK / NO_CALLBACKS | 6 / 7 | 0x06 / 0x07 | not used by herdsman |
| SET_MANUFACTURER_CODE | 21 | 0x15 | no response read |
| SET_CONCENTRATOR | 16 | 0x10 | |
| NETWORK_INIT | 23 | 0x17 | |
| STACK_STATUS_HANDLER | 25 | 0x19 | cb, status u8 (13) / u32 (14+) |
| FORM_NETWORK | 30 | 0x1E | |
| LEAVE_NETWORK | 32 | 0x20 | v14+ adds u8 options param |
| PERMIT_JOINING | 34 | 0x22 | |
| CHILD_JOIN_HANDLER | 35 | 0x23 | cb |
| TRUST_CENTER_JOIN_HANDLER | 36 | 0x24 | cb |
| GET_EUI64 | 38 | 0x26 | |
| GET_NODE_ID | 39 | 0x27 | |
| GET_NETWORK_PARAMETERS | 40 | 0x28 | |
| SEND_UNICAST | 52 | 0x34 | messageTag u8 (13) / u16 (14+) |
| SEND_BROADCAST | 54 | 0x36 | layout differs 13 vs 14+ (below) |
| SEND_MULTICAST | 56 | 0x38 | |
| MESSAGE_SENT_HANDLER | 63 | 0x3F | cb, layout differs |
| INCOMING_MESSAGE_HANDLER | 69 | 0x45 | cb, layout differs |
| GET_CONFIGURATION_VALUE | 82 | 0x52 | |
| SET_CONFIGURATION_VALUE | 83 | 0x53 | |
| SET_POLICY | 85 | 0x55 | |
| GET_POLICY | 86 | 0x56 | |
| INVALID_COMMAND | 88 | 0x58 | |
| SET_SOURCE_ROUTE_DISCOVERY_MODE | 90 | 0x5A | response u32 ms |
| SET_MULTICAST_TABLE_ENTRY | 100 | 0x64 | |
| SET_INITIAL_SECURITY_STATE | 104 | 0x68 | |
| GET_CURRENT_SECURITY_STATE | 105 | 0x69 | |
| CLEAR_TRANSIENT_LINK_KEYS | 107 | 0x6B | empty response, awaited (`ezsp.js:5153`) |
| SET_RADIO_POWER | 153 | 0x99 | |
| ID_CONFLICT_HANDLER | 124 | 0x7C | cb |
| INCOMING_MANY_TO_ONE_ROUTE_REQUEST_HANDLER | 125 | 0x7D | cb |
| INCOMING_ROUTE_ERROR_HANDLER | 128 | 0x80 | cb |
| GET_VALUE | 170 | 0xAA | |
| SET_VALUE | 171 | 0xAB | |
| CLEAR_KEY_TABLE | 177 | 0xB1 | |
| INCOMING_NETWORK_STATUS_HANDLER | 196 | 0xC4 | cb |
| IMPORT_TRANSIENT_KEY | 273 | 0x111 | v13: extra u8 flags param |
| EXPORT_KEY | 276 | 0x114 | response order differs (13 vs 14+) |
| IMPORT_KEY | 277 | 0x115 | |
| GET_NETWORK_KEY_INFO | 278 | 0x116 | |

Frame ids >= 0x100 only exist in the extended format (2-byte id), so they require the version negotiation to be done first.

### Per-version differences actually exercised (13 = 0x0d vs 14+ = 0x0e+)
- **Status width**: v13 returns u8 EmberStatus/EzspStatus (herdsman maps non-zero via `BU:14-60`; EzspStatus becomes `ZIGBEE_EZSP_ERROR`). v14+ returns **u32 SLStatus** for NETWORK_INIT, FORM_NETWORK, PERMIT_JOINING, SET_INITIAL_SECURITY_STATE, CLEAR_KEY_TABLE, SEND_*, SET_CONFIG/POLICY/VALUE, STACK_STATUS payload, MESSAGE_SENT (`BU:1055-1073`).
  - IMPORT_TRANSIENT_KEY and EXPORT_KEY always return u32 in herdsman, even on v13 (`EZ:5402`, `4945`). A v13 NCP's actual width for those two is not verified (inferred: sec-man commands are sl_status_t from v13).
- **SEND_UNICAST / MESSAGE_SENT / SEND_BROADCAST**: messageTag u8 to u16 (`EZ:3836-3843`, `517-540`).
- **SEND_BROADCAST** (`EZ:3868-3885`): v13: `u16 destination, ApsFrame, u8 radius, u8 tag, payload`. v14+: `u16 alias, u16 destination, u8 nwkSequence, ApsFrame, u8 radius, u16 tag, payload`.
- **INCOMING_MESSAGE_HANDLER**: v13 scattered fields vs v14+ RxPacketInfo (table above).
- **LEAVE_NETWORK**: v14+ has u8 `options` (0 = WITH_NO_OPTION) (`EZ:2408-2410`).
- **EXPORT_KEY**: v13 response = key then status; v14+ = status, key, context (`EZ:4940-4950`).
- **IMPORT_TRANSIENT_KEY**: v13 appends u8 flags (`EZ:5398-5400`).
- **Many v14+-only commands** throw `ERROR_INVALID_FRAME_ID` on v13 (e.g. SET_ADDRESS_TABLE_INFO, SET_EXTENDED_TIMEOUT, GET_MANUFACTURER_CODE, CONCENTRATOR_START_DISCOVERY...). None are on the minimal path.
- **v >= 0x11** (17): MUX_INVALID_RX_HANDLER callback appears (`EZ:445`); GET memory usage via extended value.
- **v < 0x12** (18): extra field on some handlers around `EZ:659` (MAC filter match, not relevant); SET_PENDING_NETWORK_UPDATE_CHANNEL has a v18 change (`EZ:1498`, not relevant).
- **v < 0x10** (16): GP incoming message handler layout (`EZ:869`), not relevant.
- Not found anywhere in herdsman: any difference of FORM_NETWORK, NETWORK_INIT, SET_INITIAL_SECURITY_STATE, TRUST_CENTER_JOIN, PERMIT_JOINING, GET_NETWORK_PARAMETERS, ADD_ENDPOINT layouts across 13..19.

## 7. Pitfalls and requirements herdsman handles
1. **Callbacks between command and response**: classify by `fc & 0x10` (async callback) before treating a frame as the response (`EZ:206-245`). A callback may arrive at any time, including before the awaited response. Keep two buffers.
2. **Event ordering**: messageSent vs incomingMessage ordering is not guaranteed (`EA:305-309`). STACK_STATUS NETWORK_UP arrives after FORM_NETWORK / NETWORK_INIT returns OK; wait for the callback (10 s, `EA:656`, `848`).
4. **NCP busy / limits** (`EA:1464-1500`): SEND_* can return `BUSY`, `ZIGBEE_MAX_MESSAGE_LIMIT_REACHED`, `ALLOCATION_FAILED` (no buffers) or `NETWORK_DOWN`; it retries up to 3 times, with 500 ms (busy) or 1500 ms (net down) waits. Herdsman queues commands (concurrency 16 by default, `EA:182`, but only one EZSP command is in flight at a time, `EZ:353`).
5. **Overflow (fc 0x01)**: treated as success, logged (`EZ:405-415`). **Truncated (0x02)**, wrong direction and unsupported control are errors (`EZ:385-400`). Errors other than security-params-invalid / overflow / queue-full / wrong-direction cause a full NCP reset + re-init (`EZ:240-252`).
6. **Many-to-one routing / source routing** (`EA:553-584`, citing AN1233): a trust centre must be a concentrator, record source routes, and keep an address cache.
   - **SET_CONCENTRATOR 16 / 0x10** params (`EZ:3127-3140`): u8 on(1), u16 concentratorType (`HIGH_RAM 0xFFF9`, `LOW_RAM 0xFFF8`, `INACTIVE 0xFFFF`), u16 minTime = 5 s, u16 maxTime = 60 s, u8 routeErrorThreshold = 3, u8 deliveryFailureThreshold = 1, u8 maxHops = 0 (`EA:104-117`). Response status (u8/u32). Failure aborts start-up.
   - **SET_SOURCE_ROUTE_DISCOVERY_MODE 90 / 0x5A**, u8 mode = 2 (RESCHEDULE) (`EA:581`), response u32 ms until next MTORR (`EZ:4238-4250`). Called after NETWORK_UP.
   - Herdsman does **not** store route records itself: the INCOMING_ROUTE_RECORD, MANY_TO_ONE_ROUTE_REQUEST, ROUTE_ERROR, NETWORK_STATUS callbacks are only logged (`EZ:4256-4320`). The NCP firmware keeps the source route table. Hence a host needs no `setSourceRoute`; no SET_SOURCE_ROUTE call exists in herdsman (inferred: the NCP does it).
   - Address cache: TRUST_CENTER_ADDRESS_CACHE_SIZE = 2 (`EA:512`). Herdsman has no address-table management of its own; it leans on `ENABLE_ADDRESS_DISCOVERY` (0x1000) in APS options.
7. **APS options on every send** = `RETRY | ENABLE_ROUTE_DISCOVERY | ENABLE_ADDRESS_DISCOVERY` = 0x1140 (`EA:130`). The code comment explains that dropping ENABLE_ROUTE_DISCOVERY led to devices that fail to reconnect, so it is kept even with MTORR (`EA:119-129`). For a send with no expected response, RETRY is cleared (`EA:1454-1456`).
8. **Radius**: ZDO uses 0xFF (`EA:85`); `ezsp.send` uses `ZA_MAX_HOPS = 12` (`EZ:72`, `920`).
9. **Manufacturer-code workaround**: herdsman changes the coordinator manufacturer code (SET_MANUFACTURER_CODE) for certain joining IEEE prefixes, and resets it on NETWORK_CLOSED (`EA:285-290`, `456-487`). Not needed for a minimal coordinator.
10. **Order of config vs endpoints**: config values (steps 3 to 13) before `registerFixedEndpoints`, before network init. `ADD_ENDPOINT` fails if the NCP was not reset since the last add, which is why herdsman checks the endpoint flags first (`EA:589-596`).
11. **Frame counter / key persistence**: after FORM_NETWORK, enable STACK_TOKEN_WRITING (`EA:849`). Herdsman logs a warning when the NWK frame counter nears 0xFEEEEEEE (`EA` near `770`).

## 8. Minimal call list (inferred from the above, not tested)
1. ASH up. VERSION(13..19) legacy first, re-send VERSION(ncpVersion) if lower. Verify stackType == 2.
2. SET_CONFIGURATION_VALUE: STACK_PROFILE=2, SECURITY_LEVEL=5, TRUST_CENTER_ADDRESS_CACHE_SIZE=2, TRANSIENT_KEY_TIMEOUT_S=300 (others are firmware defaults).
3. SET_POLICY MESSAGE_CONTENTS_IN_CALLBACK_POLICY=0x40 (so messageSent has no payload).
4. GET_EXTENDED_VALUE ENDPOINT_FLAGS(1); if not OK, ADD_ENDPOINT(1, 0x0104, 0x0065, 1, in=[0x0000,...], out=[0x0000,...]).
5. SET_POLICY TC_KEY_REQUEST=0x51, APP_KEY_REQUEST=0x60, TRUST_CENTER_POLICY=0x03.
6. NETWORK_INIT(0x0003). If NOT_JOINED: SET_INITIAL_SECURITY_STATE (bitmask 0x0B84, random 16 B TC root key, random 16 B network key, seq 0, EUI64 ff*8), SET_VALUE EXTENDED_SECURITY_BITMASK=0x0110, CLEAR_KEY_TABLE, FORM_NETWORK(random extPan, panId, power 5, channel, join 0, nwkManager 0, updateId 0, mask 0x07FFF800). Wait for STACK_STATUS NETWORK_UP, then SET_VALUE STACK_TOKEN_WRITING=1.
7. SET_CONCENTRATOR(1, 0xFFF9, 5, 60, 3, 1, 0), SET_SOURCE_ROUTE_DISCOVERY_MODE(2).
8. Permit join: IMPORT_TRANSIENT_KEY(ff*8, "ZigBeeAlliance09"), SET_POLICY TRUST_CENTER_POLICY=0x03, PERMIT_JOINING(n).
9. On TRUST_CENTER_JOIN_HANDLER (status 0x01 or 0x03, decision != 2): record (nodeId, eui64). On INCOMING_MESSAGE (profile 0, cluster 0x0013): confirm nwk and eui64.
10. SEND_UNICAST ZDO Active_EP_req (cluster 0x0005), `[tsn][nwk LE]`, wait for 0x8005. Then ZCL Read Attributes (0x10, tsn, 0x00, 04 00, 05 00) to cluster 0x0000, endpoint from the response (or 1), src endpoint 1; parse readRsp (0x01) records; strings are type 0x42 (u8 len + bytes).

