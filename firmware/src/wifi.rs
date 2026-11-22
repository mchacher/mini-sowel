//! The radio image's Wi-Fi link (feature `radio`, spec 023 R1 to R3): the station built from
//! `/data/wifi.json` with no copy in NVS, its retries, the clock set by SNTP, and the loop of
//! `main` that starts the server once the engine has loaded the home. The rules (settings, waits,
//! reasons, lines) are the core's (`solite_core::network`, spec 022); this file gives them
//! ESP-IDF.

use std::ffi::CString;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use esp_idf_svc::eventloop::{EspSubscription, EspSystemEventLoop, System};
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::netif::IpEvent;
use esp_idf_svc::sntp::{EspSntp, SntpConf};
use esp_idf_svc::sys::{
    esp_log_level_set, esp_log_level_t_ESP_LOG_WARN, esp_netif_set_hostname,
    esp_sntp_servermode_dhcp, esp_wifi_set_ps, esp_wifi_set_storage, wifi_ps_type_t_WIFI_PS_NONE,
    wifi_storage_t_WIFI_STORAGE_RAM,
};
use esp_idf_svc::wifi::{
    AuthMethod, ClientConfiguration, Configuration, EspWifi, WifiDeviceId, WifiEvent,
};
use solite_core::network::{Backoff, WifiSettings, connected_line, retry_line, wifi_reason};

use crate::http_server::{self, Server};
use crate::memory;
use crate::radio_engine::Shared;

/// Set once the station has started: the ESP32's random generator is then fed by its radio, and
/// the entropy switch of spec 021 must not run (R3.4).
pub static WIFI_STARTED: AtomicBool = AtomicBool::new(false);
/// The saver's gate (R3.2): set by the first SNTP sync or `CLOCK_WAIT_MS` after the station's
/// start, whichever comes first; set at once without Wi-Fi.
pub static CLOCK_SET: AtomicBool = AtomicBool::new(false);
/// Set by the first SNTP sync: its line is logged once, by `run`'s loop.
static CLOCK_SYNCED: AtomicBool = AtomicBool::new(false);
/// The time of the first SNTP sync, in s since the epoch (0 until then; 32 bits, the ESP32 has no
/// 64-bit atomics; enough until 2106): the callback runs on
/// lwIP's task, whose stack is small, so it only stores it and `run` formats the line.
static CLOCK_SYNCED_SECS: AtomicU32 = AtomicU32::new(0);
/// How often `run`'s loop looks for the first sync's line while the clock is not synced, in ms.
const SYNC_LINE_POLL_MS: u64 = 1_000;

/// The engine's saves wait at most this long for the clock, from the station's start, in ms.
pub const CLOCK_WAIT_MS: u64 = 20_000;
/// The `memory-network:` snapshot is logged this long after the first `serving:` line, in ms.
const SNAPSHOT_AFTER_MS: u64 = 10_000;
/// The server's port.
pub const HTTP_PORT: u16 = 80;

/// What wakes `main`'s loop: the Wi-Fi driver's events, and the engine's home.
pub enum Event {
    Started,
    Disconnected(u16),
    GotIp(Ipv4Addr),
    /// The engine has loaded the home: the server can start.
    Home(Shared),
    /// The engine stopped before it had a home.
    NoHome(String),
}

/// The DHCP host name, `solite-<6 hex>`: the last three bytes of the station's MAC address.
fn host_name(mac: [u8; 6]) -> String {
    format!("solite-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5])
}

/// The settings live in RAM only: ESP-IDF never writes the SSID or the password to NVS (D33).
#[allow(unsafe_code)]
fn keep_in_ram() -> Result<(), String> {
    // SAFETY: takes an enum value; valid once the Wi-Fi driver is initialised (EspWifi::new did).
    let code = unsafe { esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_RAM) };
    check(code, "wifi: cannot keep the settings in RAM")
}

/// ESP-IDF's own `wifi` tag at warnings only: its information lines name the SSID (R1.6).
#[allow(unsafe_code)]
fn quiet_driver_log() {
    // SAFETY: the tag is a static NUL-terminated string; the call only records a level for it.
    unsafe { esp_log_level_set(c"wifi".as_ptr(), esp_log_level_t_ESP_LOG_WARN) };
}

/// The time server the DHCP server gives (option 42) is taken when it gives one (R3.1).
#[allow(unsafe_code)]
fn ntp_from_dhcp() {
    // SAFETY: takes a flag; called before the DHCP client starts, as ESP-IDF asks.
    unsafe { esp_sntp_servermode_dhcp(true) };
}

/// Power save off (R1.5): the box is on USB or mains, and the modem sleep delays every packet.
#[allow(unsafe_code)]
fn power_save_off() -> Result<(), String> {
    // SAFETY: takes an enum value; valid once the Wi-Fi driver is initialised.
    let code = unsafe { esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE) };
    check(code, "wifi: cannot switch power save off")
}

/// Gives the station's network interface its DHCP host name, before DHCP starts.
#[allow(unsafe_code)]
fn set_host_name(wifi: &EspWifi<'static>, name: &str) -> Result<(), String> {
    let name = CString::new(name).map_err(|e| e.to_string())?;
    // SAFETY: the interface handle is the live station interface owned by `wifi`; ESP-IDF copies
    // the NUL-terminated name, which outlives the call.
    let code = unsafe { esp_netif_set_hostname(wifi.sta_netif().handle(), name.as_ptr()) };
    check(code, "wifi: cannot set the host name")
}

fn check(code: i32, what: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{what} (error {code})"))
    }
}

/// The station, its event subscriptions and its clock service, kept for the whole run.
pub struct Station {
    wifi: EspWifi<'static>,
    name: &'static str,
    _wifi_events: EspSubscription<'static, System>,
    _ip_events: EspSubscription<'static, System>,
    _sntp: Option<EspSntp<'static>>,
}

/// Starts the Wi-Fi station with the settings and SNTP; `wifi: connecting` once. The events are
/// sent to `events`.
pub fn start(
    modem: Modem<'static>,
    settings: &WifiSettings,
    events: &Sender<Event>,
) -> Result<Station, String> {
    let sysloop = EspSystemEventLoop::take().map_err(|e| format!("wifi: {e}"))?;
    // No NVS partition: ESP-IDF's `nvs_enable` is 0 (R1.4).
    let mut wifi = EspWifi::new(modem, sysloop.clone(), None).map_err(|e| format!("wifi: {e}"))?;
    keep_in_ram()?;
    quiet_driver_log();
    let mac = wifi
        .driver()
        .get_mac(WifiDeviceId::Sta)
        .map_err(|e| format!("wifi: cannot read the MAC address: {e}"))?;
    let name: &'static str = Box::leak(host_name(mac).into_boxed_str());
    set_host_name(&wifi, name)?;
    let open = settings.password().is_empty();
    let configuration = Configuration::Client(ClientConfiguration {
        ssid: settings
            .ssid()
            .try_into()
            .map_err(|_| "wifi: the network name is too long".to_string())?,
        password: settings
            .password()
            .try_into()
            .map_err(|_| "wifi: the password is too long".to_string())?,
        // WPA2-Personal at least (WPA3 when the access point chooses it); an empty password asks
        // for an open network.
        auth_method: if open {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    });
    // Never printed: the configuration holds the password (and esp-idf-svc's debug line about it
    // stays below the log level, see `main`).
    wifi.set_configuration(&configuration)
        .map_err(|e| format!("wifi: cannot set the configuration: {e}"))?;
    if open {
        log::info!("wifi: open network, no encryption");
    }

    let sender = events.clone();
    let wifi_events = sysloop
        .subscribe::<WifiEvent, _>(move |event| {
            let note = match event {
                WifiEvent::StaStarted => Some(Event::Started),
                WifiEvent::StaDisconnected(info) => Some(Event::Disconnected(info.reason())),
                _ => None,
            };
            if let Some(note) = note {
                let _ = sender.send(note);
            }
        })
        .map_err(|e| format!("wifi: cannot follow its events: {e}"))?;
    let sender = events.clone();
    let ip_events = sysloop
        .subscribe::<IpEvent, _>(move |event| {
            if let IpEvent::DhcpIpAssigned(assignment) = event {
                let _ = sender.send(Event::GotIp(assignment.ip()));
            }
        })
        .map_err(|e| format!("wifi: cannot follow the IP events: {e}"))?;

    ntp_from_dhcp();
    log::info!("wifi: connecting");
    wifi.start()
        .map_err(|e| format!("wifi: cannot start: {e}"))?;
    WIFI_STARTED.store(true, Ordering::SeqCst);
    if let Err(e) = power_save_off() {
        // The station is stopped before the flag falls: the entropy switch (R3.4) runs again only
        // with no radio of the ESP32 running.
        let _ = wifi.stop();
        WIFI_STARTED.store(false, Ordering::SeqCst);
        return Err(e);
    }
    let sntp = start_clock();
    Ok(Station {
        wifi,
        name,
        _wifi_events: wifi_events,
        _ip_events: ip_events,
        _sntp: sntp,
    })
}

/// SNTP on `pool.ntp.org`, twice: the DHCP server's time server, when it gives one, takes the
/// first place (lwIP's rule), and `pool.ntp.org` stays in the second. ESP-IDF resyncs every hour.
fn start_clock() -> Option<EspSntp<'static>> {
    let conf = SntpConf {
        servers: ["pool.ntp.org"; 2],
        ..Default::default()
    };
    match EspSntp::new_with_callback(&conf, |since_epoch| {
        // On lwIP's task: flags only, no formatting and no log line.
        if CLOCK_SYNCED_SECS.load(Ordering::SeqCst) == 0 {
            let secs = u32::try_from(since_epoch.as_secs()).unwrap_or(u32::MAX);
            CLOCK_SYNCED_SECS.store(secs.max(1), Ordering::SeqCst);
        }
        CLOCK_SET.store(true, Ordering::SeqCst);
    }) {
        Ok(sntp) => Some(sntp),
        Err(e) => {
            log::warn!("clock: cannot start SNTP: {e}");
            None
        }
    }
}

/// The address of the link, while it is up.
struct Up {
    address: Ipv4Addr,
}

/// `main`'s loop with a station: reconnects after the core's waits, logs the link's lines, ends
/// the clock wait, starts the server when the home arrives, logs the network snapshot. Never
/// returns.
pub fn run(mut station: Station, events: &Receiver<Event>) -> ! {
    let started = Instant::now();
    let mut backoff = Backoff::new();
    let mut connected_before = false;
    let mut retry_at: Option<Instant> = None;
    let mut up: Option<Up> = None;
    let mut server: Option<Server> = None;
    let mut snapshot_at: Option<Instant> = None;
    let clock_deadline = started + Duration::from_millis(CLOCK_WAIT_MS);
    loop {
        let now = Instant::now();
        let mut next = now + Duration::from_secs(60);
        for at in [retry_at, snapshot_at].into_iter().flatten() {
            next = next.min(at);
        }
        if !CLOCK_SET.load(Ordering::SeqCst) {
            next = next.min(clock_deadline);
        }
        if !CLOCK_SYNCED.load(Ordering::SeqCst) {
            next = next.min(now + Duration::from_millis(SYNC_LINE_POLL_MS));
        }
        let event = match events.recv_timeout(next.saturating_duration_since(now)) {
            Ok(event) => Some(event),
            Err(RecvTimeoutError::Timeout) => None,
            // Never: the station's subscriptions hold senders for the whole run.
            Err(RecvTimeoutError::Disconnected) => None,
        };
        let now = Instant::now();
        let synced_ms = u64::from(CLOCK_SYNCED_SECS.load(Ordering::SeqCst)) * 1_000;
        if synced_ms != 0 && !CLOCK_SYNCED.swap(true, Ordering::SeqCst) {
            log::info!(
                "clock: synced, {}",
                solite_core::devices::format_iso(synced_ms)
            );
        }
        if !CLOCK_SET.load(Ordering::SeqCst) && now >= clock_deadline {
            CLOCK_SET.store(true, Ordering::SeqCst);
            log::warn!(
                "clock: not synced after {} s, dates count from 1970 until it is",
                CLOCK_WAIT_MS / 1000
            );
        }
        if retry_at.is_some_and(|at| now >= at) {
            retry_at = None;
            connect(&mut station, &mut backoff, connected_before, &mut retry_at);
        }
        if snapshot_at.is_some_and(|at| now >= at) {
            snapshot_at = None;
            log::info!("{}", network_snapshot());
        }
        match event {
            None => {}
            Some(Event::Started) => {
                connect(&mut station, &mut backoff, connected_before, &mut retry_at);
            }
            Some(Event::Disconnected(code)) => {
                up = None;
                // A disconnection seen while a retry already waits changes nothing.
                if retry_at.is_none() {
                    let delay = backoff.next_delay_ms();
                    log::warn!(
                        "{}",
                        retry_line(&wifi_reason(code), connected_before, delay)
                    );
                    retry_at = Some(Instant::now() + Duration::from_millis(delay));
                }
            }
            Some(Event::GotIp(address)) => {
                backoff.reset();
                connected_before = true;
                retry_at = None;
                let (channel, rssi) = station
                    .wifi
                    .driver()
                    .get_ap_info()
                    .map_or((0, 0), |info| (info.channel, info.signal_strength));
                log::info!("{}", connected_line(address, station.name, channel, rssi));
                up = Some(Up { address });
                if server.is_some() {
                    serving(address, &mut snapshot_at);
                }
            }
            Some(Event::Home(shared)) => {
                match http_server::start(shared, station.name, HTTP_PORT) {
                    Ok(started) => {
                        server = Some(started);
                        if let Some(link) = &up {
                            serving(link.address, &mut snapshot_at);
                        }
                    }
                    Err(e) => log::error!("serving: none ({e})"),
                }
            }
            Some(Event::NoHome(reason)) => log::error!("serving: none ({reason})"),
        }
    }
}

/// The `serving:` line; the first one schedules the network snapshot.
fn serving(address: Ipv4Addr, snapshot_at: &mut Option<Instant>) {
    static FIRST: AtomicBool = AtomicBool::new(true);
    log::info!("serving: http://{address}/");
    if FIRST.swap(false, Ordering::SeqCst) {
        *snapshot_at = Some(Instant::now() + Duration::from_millis(SNAPSHOT_AFTER_MS));
    }
}

/// Asks the driver to connect. When the request itself fails, no disconnection event follows:
/// the retry is scheduled here, after the core's wait, with the core's line (R2.1).
fn connect(
    station: &mut Station,
    backoff: &mut Backoff,
    connected_before: bool,
    retry_at: &mut Option<Instant>,
) {
    if let Err(e) = station.wifi.connect() {
        let delay = backoff.next_delay_ms();
        let reason = format!("cannot start a connection: {e}");
        log::warn!("{}", retry_line(&reason, connected_before, delay));
        *retry_at = Some(Instant::now() + Duration::from_millis(delay));
    }
}

/// The free memory with the server idle, for information (R9.1): not the network's cost.
fn network_snapshot() -> String {
    let now = memory::read();
    let (psram_free, psram_total) = now.psram.map_or((0, 0), |p| (p.free, p.total));
    format!(
        "memory-network: internal {} KB free (largest block {} KB), psram {} KB free of {} KB",
        now.internal_free / 1024,
        now.internal_largest / 1024,
        psram_free / 1024,
        psram_total / 1024
    )
}
