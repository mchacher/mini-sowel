# Spec 005: pair-a-device

## Context

Spec 004 lets the host identify the Dongle Max's radio. To automate anything, the radio must run
a Zigbee network and let devices join it, and the host must know what joined (manufacturer and
model, read from the device's Basic cluster). This is increment A4 of the roadmap; the device for
the demo is the user's Sonoff SNZB-02 (temperature and humidity sensor), which arrives with the
dongle on 2026-11-21.

The EZSP calls come from Silicon Labs' UG100 and from Zigbee2MQTT's `ember` adapter
(`zigbee-herdsman` 11.2.2, MIT), summarised with sources in `ezsp-reference.md` in this folder.
Written and tested without hardware, against a fake radio; shown on the real dongle.

Specified in autonomy: the user approved in advance (2026-11-21) building milestone A and B as
far as possible without hardware, with specs reviewed by `spec-reviewer`. Choices made without
the user are listed under "Decided in autonomy".

## Goals

1. The radio runs a Zigbee network: formed the first time, resumed after.
2. The user opens pairing from the host; a device that joins is shown with its address,
   manufacturer and model.
3. Everything tested on the Mac with a fake radio speaking EZSP 13 and EZSP 14 and later.

## Out of scope

- Reading the device's measures (temperature...), binding and reporting: A5.
- Storing devices on the host (B2). The host keeps only a backup of the network it formed (R5);
  restoring it to another radio is out of scope.
- Leaving or removing a device; changing channel; backups of the network.
- Install codes, Zigbee 3.0 link-key negotiation beyond the well-known key (the radio's
  defaults).

## Requirements

### R1: Start the coordinator

After identifying the radio (spec 004), `Radio::start_network(settings)`:

1. configures the stack and policies, and registers the host's endpoint 1 (Home Automation
   profile 0x0104), as `ezsp-reference.md` section 1 (start-up), including the values a sleepy
   end device needs: INDIRECT_TRANSMISSION_TIMEOUT 7680, MAX_END_DEVICE_CHILDREN 32,
   END_DEVICE_POLL_TIMEOUT 8, as Zigbee2MQTT sets them;
2. calls `networkInit`; if the radio already has a network, waits for the stack status "network
   up" and the network is **resumed**; if it has none ("not joined"), **forms** one: random network
   key, random trust-centre key, random PAN id and extended PAN id (randomness given by the caller:
   the core has no random source; PAN 0x0000 and 0xFFFF, and an extended PAN of all 0x00 or all
   0xFF, are drawn again), channel from the settings (default 15), with the full sequence of
   `ezsp-reference.md` section 2 (initial security state, extended security bitmask 0x0110, clear
   key table, form network), waits for "network up", then sets STACK_TOKEN_WRITING to 1 so the
   radio saves the network in its flash;
3. sets up the concentrator and source routing (step 7);
4. reads the network parameters (`getNetworkParameters`): a node type other than coordinator (1)
   is the error "not a coordinator"; the PAN id, extended PAN id and channel shown come from it;
5. returns the network: PAN id, extended PAN id, channel, and whether it was formed or resumed.

Callbacks that arrive while a command waits for its response (for example "network up" before
the `formNetwork` response) are kept: this spec adds to spec 004's EZSP layer a queue of the four
callbacks it uses (stack status, trust-centre join, incoming message, message sent), at most 16,
the oldest dropped and logged; other callbacks are ignored. The EZSP overflow flag (the radio short
of buffers, common during a join burst) is logged, not an error (spec 004 made it an error).
Callback parsers accept trailing bytes. The network and trust-centre keys are never printed or
logged.

"Network up" not received within 10 s is an error (`network did not start`). A radio that is
joined to a network as a router or end device, not as coordinator, is an error naming the
problem (no automatic leave: leaving destroys a network the user may need).

### R2: Pairing

`Radio::permit_join(seconds)` (1 to 254) loads the well-known Zigbee link key as a transient key,
allows joins in the trust-centre policy, and opens joining for that time (step 8). When the time
is over, or the host stops, joining is closed (permit joining 0, transient keys cleared, policy
back to "no joins"; closing on stop is best effort).

### R3: A device joins

While joining is open, the radio's callbacks are read:

- a trust-centre join (`trustCenterJoinHandler`, status unsecured join or unsecured rejoin, not
  denied) or a device announce (ZDO `Device_annce` received in `incomingMessageHandler`, profile
  0, cluster 0x0013) gives an event **Joined** (64-bit address, network address), once per
  device per pairing session;
- the identification starts on the device announce (the device then has the network key), or
  5 s after the trust-centre join if no announce came; then the host asks the device for its active endpoints (ZDO `Active_EP_req`) and reads, on the
  first endpoint (endpoint 1 if the device does not answer within 10 s), the Basic cluster's
  manufacturer name (0x0004) and model identifier (0x0005), with ZCL Read Attributes; the
  answer gives an event **Identified** (64-bit address, manufacturer, model). A device that does
  not answer within 10 s, after one retry, gives **Identified** with both unknown. Strings are
  trimmed of trailing 0x00 bytes and read lossily if not UTF-8. Only the answer from that device,
  with the request's sequence number (TSN), counts; a ZCL Default Response does not.
- After 3 busy answers the identification fails: event **NotIdentified** (64-bit address).
- `incomingMessageHandler` and `messageSentHandler` are read in their EZSP 13 and EZSP 14+
  layouts (section 2 of `ezsp-reference.md`); `messageSentHandler` is not awaited before matching
  a response.
- A `sendUnicast` answered "busy" is retried up to 3 times, 500 ms apart.

Devices seen in a session are bounded to 32 (a bound written next to it; one past it, the event
is reported and the device ignored).

### R4: The host

`cargo run -p mini-sowel-host -- --radio tcp://<address>:6638 --pair [seconds]` (default 180):

```
mini-sowel 0.1.0
radio: tcp://192.168.1.50:6638
ezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0
radio address: 0x8C6FB9FFFE123456
network: formed, channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD
pairing: open for 180 s
joined: 0x00124B0012345678 (0x5C3D)
device: 0x00124B0012345678 eWeLink TH01
pairing: closed
```

The manufacturer and model are the strings the device sends (an original SNZB-02 sends
`eWeLink` and `TH01`). With `--pair`, spec 004's `network: <state>` line is not printed (after
the reset it always says no network); the `network:` line above replaces it. `network: resumed,
...` when the network existed; `network: resumed (made by another program), ...` when the radio
holds a coordinator network the host has no backup of (R5): it is used, never destroyed. Other
lines: `device: <eui64> unknown unknown` (no answer), `device: <eui64> not identified (radio
busy)`, `pairing: too many devices, <eui64> ignored`. The extended PAN and the addresses are
printed most significant byte first. When the time is over, identifications in progress finish
(at most 40 s more) before `pairing: closed`. Without `--pair`, the host behaves as in spec
004 (identify only; it does not start the network). Errors: one line on stderr, exit 1, as spec
004; the new ones are `radio: network did not start`, `radio: the radio is not a coordinator`.
Randomness comes from the operating system (`/dev/urandom` read by the host). Ctrl-C during
pairing closes joining (permit joining 0, transient keys cleared) then exits 130; it uses the
`ctrlc` crate (decision D12).

### R5: Network backup

As soon as the radio accepts the new network (`formNetwork` succeeded), before any later step
that can fail, the host writes `network.json` in its data folder (`--data <dir>`,
default `mini-sowel-data/` in the current folder, ignored by git): PAN id, extended PAN id,
channel, network key and trust-centre key, in hexadecimal, with the file readable by its owner
only (mode 0600). On resume, the host compares the radio's PAN id and extended PAN id with the
backup to tell its own network from another program's. If the folder already holds a backup of a different network,
it is kept, renamed `network-<PAN id>.json`, before the new one is written. A backup that
cannot be written after forming is an error (`cannot write <path>`) printed before anything else.
Nothing reads the keys back in this spec; they are kept so a later restore is possible. The file is never printed.

## Living spec delta

### docs/living-spec/README.md

- New section "Pairs a device": `--pair [seconds]`, the network formed the first time (channel
  15, random keys and identifiers) and resumed after, the lines shown, the errors.
- Changed: "it talks to the radio only to identify it" → "it talks to the radio to identify it,
  start its network and pair devices; it keeps nothing about devices between runs".
- New: History row 005.

### No longer true

- `docs/living-spec/README.md`: the sentence above.
- `docs/architecture.md`: the radio block (identification only) → also the coordinator; the
  data folder.
- `README.md`, `CLAUDE.md`: the `--radio ... # identify` line → a `--pair` line added beside it.
- `docs/roadmap.md`, A4 row: "network key and channel kept in a file" → stays true (R5), wording
  kept.
- `.gitignore`: `mini-sowel-data/`.
- Searched: `--radio`, `identify`, `pair`, `network`, `A4`.

## Acceptance criteria

- [x] Against the fake radio (EZSP 13 and EZSP 14+): formed then resumed network, a device joins,
      is identified, joining closes; the host prints the lines of R4.
- [ ] With the real Dongle Max and SNZB-02: a `device:` line shows the strings the sensor sends
      (expected `eWeLink TH01`) (demo, pending).

## Edge cases

- The radio has a network as coordinator: resumed, no new keys.
- The radio is joined as a router or end device: error, no leave.
- "Network up" never comes: error after 10 s.
- The same device announces twice, or rejoins: one Joined event per session.
- The device does not answer the endpoint request: endpoint 1; does not answer the Basic read:
  manufacturer and model unknown.
- Read Attributes response with one attribute unsupported (status not success): that one unknown.
- A string attribute with a wrong length: unknown, no panic.
- `sendUnicast` busy 3 times: the device's identification fails, the session goes on.
- A trust-centre join with decision "deny", or status "left": ignored.
- 33 devices in one session: the 33rd reported and ignored.
- "Network up" arriving before the `networkInit` or `formNetwork` response: kept (the queue of
  R1), the start succeeds.
- Many unused callbacks during a join burst (child join, route record...): ignored, they do not
  push the useful ones out.
- The overflow flag on a response: logged, the response used.
- A device joining in the last seconds of the window: identified before `pairing: closed`.
- An answer from another device, or with another TSN: ignored.
- `networkInit` "not joined" is 0x93 (u8) in EZSP 13 and 0x17 (u32) in EZSP 14+.
- A random PAN id of 0x0000 or 0xFFFF: drawn again.
- No data folder or not writable: error `cannot write <path>`, exit 1, before the radio is
  touched.

## Sowel contract

None yet: devices become Sowel `Device`s in B2. The manufacturer and model strings are kept as the
device sends them (Sowel's `Device` carries `manufacturer` and `model`, `../sowel/src/shared/types.ts`).

## Decided in autonomy

- Channel 15 by default (less overlap with the Wi-Fi channels 1, 6 and 11 than Zigbee2MQTT's 11);
  changeable later.
- The radio's flash is what resumes the network; the host writes a backup of the keys (R5), not
  read back yet (spec review question: the keys are not lost if the radio is erased).
- A coordinator network made by another program is used and named, never left or destroyed
  (spec review question).
- Ctrl-C closes joining, with the `ctrlc` crate (spec review question; decision D12).
- After the code review: the backup is written from a callback the coordinator calls right after
  `formNetwork` succeeds; the host installs a minimal logger on stderr (warnings and above), so
  the warnings of R1 are seen on the Mac; Ctrl-C before pairing opened exits without opening it;
  ZDO answers wait 15 s (Zigbee2MQTT's value; ZCL stays 10 s); a response arriving after its
  timeout (older sequence number) is ignored instead of ending the run.
- Added while building: the trust-centre policy is "rejoins only" (0x02) from the start, so
  joins stay closed until `permit_join`; the radio's parameters are read before the concentrator
  is set, so a foreign router is never configured; a non-busy `sendUnicast` refusal counts as a
  busy attempt; `pairing: closed` is also printed on Ctrl-C and on an error after pairing opened;
  the random source can fail (an error, never zero keys); the core gains the `log` crate;
  payloads are moved from the ASH session to an inbox of 64 as they arrive, so a burst of
  callbacks does not overflow the session's 8.
- The demo line expects `eWeLink TH01`, what an original SNZB-02 sends (spec review).
- Endpoint 1 as the fallback for the Basic read (the SNZB-02 and most Sonoff devices).

## How the user will check it

Run `cargo run -p mini-sowel-host -- --radio tcp://<address>:6638 --pair`, press the SNZB-02's
button for 5 seconds until its light blinks, and read the `joined` and `device` lines. Run it a
second time: `network: resumed`.

Run on the real hardware (2026-11-21): `--pair 240` formed the network (channel 15, a PAN id) and opened pairing; a Sonoff ZBMINIR2 relay and a Sonoff SNZB-03PR2 motion sensor both joined and were named `SONOFF ZBMINIR2` and `SONOFF SNZB-03PR2`. The SNZB-02 was not used. A warning at start: `the radio refused setValue (status 0x10070), its own value is used` (open defect, to investigate).
