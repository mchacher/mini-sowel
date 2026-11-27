//! The simulated Zigbee devices: what they are, what they answer to the interview, and the
//! frames they report with. The simulated radio (`ncp.rs`) plays them.

/// What a simulated device is, which fixes its clusters and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A sleepy temperature and humidity sensor (Sonoff SNZB-02).
    TemperatureHumidity,
    /// A sleepy motion sensor with an IAS zone (Sonoff SNZB-03).
    Motion,
    /// A mains-powered relay answering On, Off and Toggle (Sonoff ZBMINIR2).
    Relay,
    /// A mains-powered dimmable light: the relay's commands and Move to Level with On/Off. Only
    /// the tests of brightness orders use it (spec 011); no shipped scenario has one.
    Dimmer,
    /// A mains-powered colour light: the dimmer's commands plus the Color Control cluster (Move to
    /// Color Temperature, Move to Color, Move to Hue and Saturation, spec 031).
    ColorLight,
    /// A mains-powered metering plug: On, Off and Toggle, Electrical Measurement and Metering
    /// (spec 048). It answers the reads of the scale: power divisor 10, current and energy
    /// divisor 1000, multipliers 1.
    Plug,
}

impl Kind {
    /// The name the scenario files use.
    pub fn name(self) -> &'static str {
        match self {
            Kind::TemperatureHumidity => "temperature-humidity",
            Kind::Motion => "motion",
            Kind::Relay => "relay",
            Kind::Dimmer => "dimmer",
            Kind::ColorLight => "color-light",
            Kind::Plug => "plug",
        }
    }

    /// The kind with this scenario name.
    pub fn from_name(name: &str) -> Option<Kind> {
        [
            Kind::TemperatureHumidity,
            Kind::Motion,
            Kind::Relay,
            Kind::Dimmer,
            Kind::ColorLight,
            Kind::Plug,
        ]
        .into_iter()
        .find(|kind| kind.name() == name)
    }

    /// The Zigbee device id of the endpoint's simple descriptor.
    pub(crate) fn device_id(self) -> u16 {
        match self {
            Kind::TemperatureHumidity => 0x0302,
            Kind::Motion => 0x0402,
            Kind::Relay => 0x0100,
            Kind::Dimmer => 0x0101,
            Kind::ColorLight => 0x010D,
            Kind::Plug => 0x0051,
        }
    }
}

/// What a report carries: each field is for some kinds only (the scenario checks it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceReport {
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    /// 0 to 100.
    pub battery: Option<u8>,
    pub motion: Option<bool>,
    pub light_state: Option<bool>,
    /// A colour temperature in mireds (150 to 500), `color-light` only.
    pub color_temp: Option<u16>,
    /// A colour as X and Y (each 0 to 1), `color-light` only: one frame with both attributes.
    pub color: Option<(f64, f64)>,
    /// Watts, `plug` only (reported with the plug's scale).
    pub power: Option<f64>,
    /// Amperes, `plug` only.
    pub current: Option<f64>,
    /// Kilowatt-hours, `plug` only.
    pub energy: Option<f64>,
}

/// A character attribute of the simulated device.
#[derive(Clone, Debug)]
pub enum Attr {
    /// The bytes of the string as the device sends them.
    Text(Vec<u8>),
    /// The device answers "unsupported attribute" (status 0x86).
    Unsupported,
}

/// How the device answers a Configure Reporting request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigureAnswer {
    /// A Configure Reporting Response with the single status byte 0x00.
    Single,
    /// A Configure Reporting Response with one record (status 0) per attribute.
    PerAttribute,
    /// A Configure Reporting Response refusing every attribute with this status.
    Refuse(u8),
    /// A Default Response with this status.
    Default(u8),
    /// A Configure Reporting Response with the single status byte given (the whole command
    /// failed when it is not 0x00).
    SingleStatus(u8),
    /// A Configure Reporting Response refusing this one attribute with the status, and accepting
    /// the others (as one record per attribute).
    RefuseAttribute(u16, u8),
    /// No answer.
    Silent,
}

/// How the device answers the write of the CIE address to its IAS zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CieAnswer {
    /// It keeps the address and answers with the single status byte 0x00.
    Accept,
    /// It refuses with this status (the address is not kept).
    Refuse(u8),
    /// No answer (the address is not kept).
    Silent,
}

/// How long a sleepy device listens after joining or after one of its reports, in milliseconds.
pub const AWAKE_MS: u64 = 10_000;
/// A motion sensor clears its alarm this long after raising it, in milliseconds.
pub const MOTION_CLEAR_MS: u64 = 60_000;

/// A ZCL Report Attributes frame from a device (server to client, default response disabled by
/// the device): the records are `(attribute, data type, value bytes)`.
pub fn report_frame(tsn: u8, records: &[(u16, u8, Vec<u8>)]) -> Vec<u8> {
    report_frame_with_control(0x18, tsn, records)
}

/// The same with the frame control given: 0x08 asks the coordinator for a Default Response.
pub fn report_frame_with_control(control: u8, tsn: u8, records: &[(u16, u8, Vec<u8>)]) -> Vec<u8> {
    let mut frame = vec![control, tsn, 0x0A];
    for (attribute, data_type, value) in records {
        frame.extend_from_slice(&attribute.to_le_bytes());
        frame.push(*data_type);
        frame.extend_from_slice(value);
    }
    frame
}

/// A temperature report, in hundredths of a degree.
pub fn temperature_report(tsn: u8, hundredths: i16) -> Vec<u8> {
    report_frame(tsn, &[(0x0000, 0x29, hundredths.to_le_bytes().to_vec())])
}

/// A humidity report, in hundredths of a percent.
pub fn humidity_report(tsn: u8, hundredths: u16) -> Vec<u8> {
    report_frame(tsn, &[(0x0000, 0x21, hundredths.to_le_bytes().to_vec())])
}

/// A battery report: half-percent units.
pub fn battery_report(tsn: u8, half_percent: u8) -> Vec<u8> {
    report_frame(tsn, &[(0x0021, 0x20, vec![half_percent])])
}

/// An On/Off report: the `OnOff` attribute (boolean).
pub fn on_off_report(tsn: u8, on: bool) -> Vec<u8> {
    report_frame(tsn, &[(0x0000, 0x10, vec![u8::from(on)])])
}

/// A level report: the `CurrentLevel` attribute (uint8).
pub fn level_report(tsn: u8, level: u8) -> Vec<u8> {
    report_frame(tsn, &[(0x0000, 0x20, vec![level])])
}

/// A colour temperature report: the `ColorTemperatureMireds` attribute (uint16).
pub fn color_temp_report(tsn: u8, mireds: u16) -> Vec<u8> {
    report_frame(tsn, &[(0x0007, 0x21, mireds.to_le_bytes().to_vec())])
}

/// A colour report: `CurrentX` and `CurrentY` (uint16, raw units) in one frame.
pub fn color_xy_report(tsn: u8, x: u16, y: u16) -> Vec<u8> {
    report_frame(
        tsn,
        &[
            (0x0003, 0x21, x.to_le_bytes().to_vec()),
            (0x0004, 0x21, y.to_le_bytes().to_vec()),
        ],
    )
}

/// The raw CurrentX or CurrentY of a value from 0 to 1.
pub fn color_raw(value: f64) -> u16 {
    ((value * 65535.0).round() as u16).min(0xFEFF)
}

/// The scale a simulated plug answers: (Electrical Measurement power multiplier, power divisor,
/// current multiplier, current divisor, Metering multiplier, divisor).
pub const PLUG_SCALE: (u16, u16, u16, u16, u32, u32) = (1, 10, 1, 1000, 1, 1000);

/// An Electrical Measurement report: ActivePower (int16, in 1/10 W) and RMSCurrent (uint16, mA)
/// of the plug's scale; `None` leaves the attribute out.
pub fn electrical_report(tsn: u8, watts: Option<f64>, amperes: Option<f64>) -> Vec<u8> {
    let mut records = Vec::new();
    if let Some(watts) = watts {
        let raw = (watts * f64::from(PLUG_SCALE.1)).round() as i16;
        records.push((0x050B, 0x29, raw.to_le_bytes().to_vec()));
    }
    if let Some(amperes) = amperes {
        let raw = (amperes * f64::from(PLUG_SCALE.3)).round() as u16;
        records.push((0x0508, 0x21, raw.to_le_bytes().to_vec()));
    }
    report_frame(tsn, &records)
}

/// A Metering report: CurrentSummationDelivered (uint48, in Wh).
pub fn metering_report(tsn: u8, kilowatt_hours: f64) -> Vec<u8> {
    let raw = (kilowatt_hours * f64::from(PLUG_SCALE.5)).round() as u64;
    report_frame(tsn, &[(0x0000, 0x25, raw.to_le_bytes()[..6].to_vec())])
}

/// An IAS Zone Status Change Notification (cluster-specific, server to client): the alarm bit
/// is set or cleared.
pub fn zone_notification(tsn: u8, alarm: bool) -> Vec<u8> {
    zone_status_notification(tsn, u16::from(alarm))
}

/// An IAS Zone Status Change Notification with these zone status bits.
pub fn zone_status_notification(tsn: u8, status: u16) -> Vec<u8> {
    let mut frame = vec![0x19, tsn, 0x00];
    frame.extend_from_slice(&status.to_le_bytes());
    frame.push(0x00); // extended status
    frame.push(0x00); // zone id
    frame.extend_from_slice(&0u16.to_le_bytes()); // delay
    frame
}

/// A simulated Zigbee device: it joins when pairing opens (or when its scenario says so),
/// answers the endpoint request and the Basic read, then its descriptor, binds and Configure
/// Reporting, and sends reports.
#[derive(Clone, Debug)]
pub struct SimDevice {
    pub kind: Kind,
    /// As on the wire (little-endian).
    pub eui_wire: [u8; 8],
    /// The network address; 0: given when the device joins (the radio gives each a free one).
    pub nwk: u16,
    /// Joins by itself when pairing opens. A device of a scenario joins at its `join` event.
    pub auto_join: bool,
    /// A sleepy device answers only for this long after joining or after one of its reports
    /// (`AWAKE_MS`); `None`: it always answers.
    pub sleepy_ms: Option<u64>,
    /// Time between the opening of pairing and the join, in milliseconds.
    pub join_delay_ms: u64,
    /// `EmberDeviceUpdate` of the trust-centre join, and its `EmberJoinDecision`.
    pub join_status: u8,
    pub join_decision: u8,
    /// Sends a `Device_annce` just after the join; twice if `announce_twice`.
    pub announce: bool,
    pub announce_twice: bool,
    /// The capability byte of its announce (bit 2: mains powered).
    pub capability: u8,
    /// The endpoints it lists; `None`: it never answers the endpoint request.
    pub endpoints: Option<Vec<u8>>,
    /// Status of its `Active_EP_rsp`.
    pub endpoint_status: u8,
    pub manufacturer: Attr,
    pub model: Attr,
    /// Number of Read Attributes requests it ignores before it answers; `u32::MAX`: never.
    pub ignore_reads: u32,
    /// Sends wrong answers (other device, other transaction number, Default Response) before
    /// each right one.
    pub decoys: bool,
    /// Time between a request and its answer, in milliseconds.
    pub answer_delay_ms: u64,
    /// Sends `messageSentHandler` after the answer instead of before.
    pub message_sent_last: bool,
    /// The input clusters of its endpoint, as its descriptor lists them.
    pub input_clusters: Vec<u16>,
    /// Number of Simple_Desc requests it ignores before it answers; `u32::MAX`: never.
    pub ignore_descriptors: u32,
    /// Clusters whose Bind it refuses (status 0x84).
    pub bind_refused: Vec<u16>,
    /// How it answers a Configure Reporting, and the clusters that get another answer.
    pub configure: ConfigureAnswer,
    pub configure_overrides: Vec<(u16, ConfigureAnswer)>,
    /// Number of Configure Reporting requests it ignores before it answers; `u32::MAX`: never.
    pub ignore_configures: u32,
    /// `(cluster, ZCL frame)` reports sent right after the first Bind is answered, before the
    /// Configure Reporting.
    pub reports_during_setup: Vec<(u16, Vec<u8>)>,
    /// `(cluster, ZCL frame)` reports sent once the Configure Reporting of every cluster the
    /// host sets up is answered (right after the last answer).
    pub reports: Vec<(u16, Vec<u8>)>,
    /// Sends all its `reports` when the network is resumed (a second run of the host).
    pub reports_on_resume: bool,
    // State of the device (kept in the radio's memory file).
    pub(crate) joined: bool,
    pub(crate) bound: bool,
    pub(crate) configured: Vec<u16>,
    /// `(cluster, attribute)` whose reporting the host configured: only these are reported.
    pub configured_attributes: Vec<(u16, u16)>,
    pub(crate) reports_sent: bool,
    pub(crate) relay_on: bool,
    /// The level of a dimmer (0 to 254).
    pub(crate) level: u8,
    /// The colour state of a colour light, in ZCL's raw units: mireds, CurrentX, CurrentY, hue and
    /// saturation (0 to 254). A hue and saturation command leaves X and Y unchanged.
    pub(crate) color_temp: u16,
    pub(crate) color_x: u16,
    pub(crate) color_y: u16,
    pub(crate) hue: u8,
    pub(crate) saturation: u8,
    /// The CIE address was written to the IAS zone (the first half of the enrollment).
    pub(crate) cie_written: bool,
    /// The Zone Enroll Response was received: the zone reports.
    pub(crate) enrolled: bool,
    /// The CIE address held by the IAS zone, as on the wire; zeros before any write.
    pub cie: [u8; 8],
    /// The zone type its IAS zone gives; `None`: it answers "unsupported attribute".
    pub zone_type: Option<u16>,
    /// How it answers the write of the CIE address.
    pub cie_answer: CieAnswer,
    /// The zone state stays "not enrolled" whatever it is sent.
    pub zone_stays_unenrolled: bool,
    /// Number of IAS zone reads it ignores before it answers; `u32::MAX`: never.
    pub ignore_zone_reads: u32,
    /// Number of IAS zone reads it answers; the next ones get no answer.
    pub zone_reads_answered: u32,
    /// The device answers no On/Off, Level Control or Color Control command or read, and changes
    /// nothing (the tests of orders; kept in the radio's memory file so a test can set it between
    /// two runs).
    pub silent: bool,
    /// The radio's address table answers "unknown" to a lookup of this device's address (a test
    /// switch: the host then reaches it only at the address its own files keep; kept in the
    /// memory file like `silent`).
    pub lookup_unknown: bool,
    /// The device answers every On/Off, Level Control or Color Control command with a Default
    /// Response of this status, and changes nothing.
    pub refuse_commands: Option<u8>,
    // State of the run, not kept.
    pub(crate) awake_until: u64,
    pub(crate) tsn: u8,
}

/// The colour state of a colour light before any command: 370 mireds, X 0.3127 and Y 0.3290 (D65
/// white).
pub(crate) const DEFAULT_COLOR_TEMP: u16 = 370;
pub(crate) const DEFAULT_COLOR_X: u16 = 20_493;
pub(crate) const DEFAULT_COLOR_Y: u16 = 21_561;

/// Announce capability of a battery device: allocate address only.
const CAPABILITY_BATTERY: u8 = 0x80;
/// Announce capability of a mains device: allocate address, mains powered, receiver on.
const CAPABILITY_MAINS: u8 = 0x8C;

/// The EUI64 of a scenario (most significant byte first) as the radio sends it.
pub fn wire(eui64: [u8; 8]) -> [u8; 8] {
    let mut reversed = eui64;
    reversed.reverse();
    reversed
}

impl SimDevice {
    fn base(kind: Kind, eui_wire: [u8; 8]) -> Self {
        SimDevice {
            kind,
            eui_wire,
            nwk: 0,
            auto_join: false,
            sleepy_ms: None,
            join_delay_ms: 0,
            join_status: 1,
            join_decision: 0,
            announce: true,
            announce_twice: false,
            capability: CAPABILITY_BATTERY,
            endpoints: Some(vec![1]),
            endpoint_status: 0,
            manufacturer: Attr::Unsupported,
            model: Attr::Unsupported,
            ignore_reads: 0,
            decoys: false,
            answer_delay_ms: 0,
            message_sent_last: false,
            input_clusters: Vec::new(),
            ignore_descriptors: 0,
            bind_refused: Vec::new(),
            configure: ConfigureAnswer::Single,
            configure_overrides: Vec::new(),
            ignore_configures: 0,
            reports_during_setup: Vec::new(),
            reports: Vec::new(),
            reports_on_resume: false,
            joined: false,
            bound: false,
            configured: Vec::new(),
            configured_attributes: Vec::new(),
            reports_sent: false,
            relay_on: false,
            level: 0,
            color_temp: DEFAULT_COLOR_TEMP,
            color_x: DEFAULT_COLOR_X,
            color_y: DEFAULT_COLOR_Y,
            hue: 0,
            saturation: 0,
            cie_written: false,
            enrolled: false,
            cie: [0; 8],
            zone_type: None,
            cie_answer: CieAnswer::Accept,
            zone_stays_unenrolled: false,
            ignore_zone_reads: 0,
            zone_reads_answered: u32::MAX,
            silent: false,
            lookup_unknown: false,
            refuse_commands: None,
            awake_until: 0,
            tsn: 0,
        }
    }

    /// A device of a scenario: of this kind, with this address (as on the wire), and the
    /// default identity of the kind unless the scenario gives one.
    pub fn of_kind(
        kind: Kind,
        eui_wire: [u8; 8],
        manufacturer: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        let mut device = SimDevice::base(kind, eui_wire);
        let (default_manufacturer, default_model, clusters, sleepy) = match kind {
            Kind::TemperatureHumidity => (
                "eWeLink",
                "TH01",
                vec![0x0000, 0x0001, 0x0003, 0x0402, 0x0405],
                true,
            ),
            Kind::Motion => (
                "eWeLink",
                "MS01",
                vec![0x0000, 0x0001, 0x0003, 0x0500],
                true,
            ),
            Kind::Relay => (
                "SONOFF",
                "ZBMINIR2",
                vec![0x0000, 0x0003, 0x0004, 0x0005, 0x0006],
                false,
            ),
            Kind::Dimmer => (
                "TEST",
                "DIM01",
                vec![0x0000, 0x0003, 0x0004, 0x0005, 0x0006, 0x0008],
                false,
            ),
            Kind::ColorLight => (
                "TEST",
                "CLR01",
                vec![0x0000, 0x0003, 0x0004, 0x0005, 0x0006, 0x0008, 0x0300],
                false,
            ),
            Kind::Plug => (
                "TEST",
                "PLG01",
                vec![0x0000, 0x0003, 0x0004, 0x0005, 0x0006, 0x0702, 0x0B04],
                false,
            ),
        };
        let text = |value: &str| Attr::Text(value.as_bytes().to_vec());
        device.manufacturer = text(manufacturer.unwrap_or(default_manufacturer));
        device.model = text(model.unwrap_or(default_model));
        device.input_clusters = clusters;
        if kind == Kind::Motion {
            device.zone_type = Some(0x000D);
        }
        device.sleepy_ms = sleepy.then_some(AWAKE_MS);
        if matches!(
            kind,
            Kind::Relay | Kind::Dimmer | Kind::ColorLight | Kind::Plug
        ) {
            device.capability = CAPABILITY_MAINS;
        }
        device
    }

    /// An original SNZB-02: `eWeLink` / `TH01`, on endpoint 1, announcing itself, joining when
    /// pairing opens, with the reports it sends once its reporting is set up. The fixture of the
    /// tests of the radio and the coordinator.
    pub fn sensor() -> Self {
        let mut device = SimDevice::of_kind(
            Kind::TemperatureHumidity,
            [0x78, 0x56, 0x34, 0x12, 0x00, 0x4B, 0x12, 0x00],
            None,
            None,
        );
        device.nwk = 0x5C3D;
        device.auto_join = true;
        device.sleepy_ms = None;
        device.input_clusters = vec![0x0000, 0x0001, 0x0402, 0x0405, 0x0003];
        device.reports = vec![
            (0x0402, temperature_report(0x31, 1952)),
            (0x0405, humidity_report(0x32, 5410)),
            (0x0001, battery_report(0x33, 200)),
        ];
        device.reports_on_resume = true;
        device
    }

    /// The IAS zone starts enrolled with this CIE address (as on the wire).
    pub fn enrolled_with(mut self, cie: [u8; 8]) -> Self {
        self.cie = cie;
        self.cie_written = true;
        self.enrolled = true;
        self
    }

    /// The device is in the network already, as after a restart of the host: it answers what is
    /// sent to it without joining again.
    pub fn in_network(mut self) -> Self {
        self.joined = true;
        self
    }

    /// The next transaction number of a frame the device sends.
    pub(crate) fn next_tsn(&mut self) -> u8 {
        self.tsn = self.tsn.wrapping_add(1);
        self.tsn
    }

    /// True when the host configured the reporting of this attribute.
    pub(crate) fn reports_attribute(&self, cluster: u16, attribute: u16) -> bool {
        self.configured_attributes.contains(&(cluster, attribute))
    }
}
