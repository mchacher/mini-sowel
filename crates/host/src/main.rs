//! Solite on the Mac.

mod backup;
mod backup_files;
mod fetch;
mod front;
mod logger;
mod radio_tcp;
mod random;
mod serve;
mod static_files;
mod store;
mod update_cli;
mod update_files;
mod ws;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use std::sync::Mutex;

use solite_core::engine::CLOSE_GRACE_MS;
use solite_core::radio::{Radio, RadioTransport, identity_lines, network_line};
use solite_sim::{RealClock, Scenario, SimTransport};

use radio_tcp::TcpTransport;
use random::SystemRandom;
use solite_core::engine::Saver;
use store::{FileStore, show_on_stderr};

const USAGE: &str = "usage: solite-host [--radio tcp://<address>:<port> | \
--sim <scenario> [--speed <1-100>]] [--pair [seconds] | --listen | --serve [port] [--verbose] [--ui <dir>]] \
[--data <dir>] | \
--update-keygen <file> | \
--update-sign <image> --version <x.y.z> --key <file> --out <file> | \
--update-verify <file> --public-key <64 hex>";
/// Pairing time when `--pair` has no number.
const DEFAULT_PAIR_SECONDS: u8 = 180;
/// Port of `--serve` when it has no number.
const DEFAULT_SERVE_PORT: u16 = 8080;
const DEFAULT_DATA_DIR: &str = "solite-data";
/// Under `--sim` the data folder is another one, so the real radio's backup is never touched.
const DEFAULT_SIM_DATA_DIR: &str = "solite-data/sim";
/// The grace after the pairing time: the core's `CLOSE_GRACE_MS`, or the number of seconds the
/// variable `SOLITE_CLOSE_GRACE_SECONDS` gives (for the tests). `--pair` and `--serve` use it.
fn close_grace() -> Duration {
    std::env::var("SOLITE_CLOSE_GRACE_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(Duration::from_millis(CLOSE_GRACE_MS), Duration::from_secs)
}
/// What a shell reports for a program stopped by Ctrl-C.
const EXIT_INTERRUPTED: u8 = 130;

/// Where the radio is.
#[derive(Debug, PartialEq, Eq)]
enum Source {
    /// The radio's `host:port`.
    Tcp(String),
    /// The scenario file of the simulated radio.
    Sim(String),
}

#[derive(Debug, PartialEq, Eq)]
struct Args {
    source: Option<Source>,
    /// How much faster than real time the scenario's clock runs (`--sim` only).
    speed: u32,
    /// Pairing time in seconds, if `--pair` is given.
    pair: Option<u8>,
    /// Show the measures of the paired devices, without pairing.
    listen: bool,
    /// Port of the device API, if `--serve` is given.
    serve: Option<u16>,
    /// Print the measures too while serving.
    verbose: bool,
    /// The folder of the interface's files (`--serve` only).
    ui: Option<PathBuf>,
    data: PathBuf,
    /// `--update-keygen`, `--update-sign` or `--update-verify`: a task of its own, without a radio.
    update: Option<update_cli::Command>,
}

/// Reads the command line.
fn parse_args(args: &[String]) -> Result<Args, String> {
    let usage = || USAGE.to_string();
    let mut parsed = Args {
        source: None,
        speed: 1,
        pair: None,
        listen: false,
        serve: None,
        verbose: false,
        ui: None,
        data: PathBuf::new(),
        update: None,
    };
    let mut data = None;
    // `--update-sign` and its three values.
    let (mut sign_image, mut sign_version, mut sign_key, mut sign_out) = (None, None, None, None);
    // `--update-verify` and its public key.
    let (mut verify_file, mut verify_key) = (None, None);
    let mut speed_given = false;
    let mut rest = args.iter().peekable();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--radio" => {
                if parsed.source.is_some() {
                    return Err(usage());
                }
                let value = rest.next().ok_or_else(usage)?;
                let address = value.strip_prefix("tcp://").ok_or_else(usage)?;
                let (host, port) = address.rsplit_once(':').ok_or_else(usage)?;
                if host.is_empty() || port.parse::<u16>().is_err() {
                    return Err(usage());
                }
                parsed.source = Some(Source::Tcp(address.to_string()));
            }
            "--sim" => {
                if parsed.source.is_some() {
                    return Err(usage());
                }
                let value = rest
                    .next_if(|next| !next.starts_with("--"))
                    .ok_or_else(usage)?;
                parsed.source = Some(Source::Sim(value.clone()));
            }
            "--speed" => {
                let value = rest.next().ok_or_else(usage)?;
                if !value.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(usage());
                }
                parsed.speed = match value.parse::<u32>() {
                    Ok(speed) if (1..=100).contains(&speed) => speed,
                    _ => return Err(usage()),
                };
                speed_given = true;
            }
            "--pair" => {
                parsed.pair = Some(match rest.next_if(|next| !next.starts_with("--")) {
                    None => DEFAULT_PAIR_SECONDS,
                    Some(text) => match text.parse::<u8>() {
                        Ok(seconds) if (1..=254).contains(&seconds) => seconds,
                        _ => return Err(usage()),
                    },
                });
            }
            "--listen" => parsed.listen = true,
            "--serve" => {
                parsed.serve = Some(match rest.next_if(|next| !next.starts_with("--")) {
                    None => DEFAULT_SERVE_PORT,
                    Some(text) => match text.parse::<u16>() {
                        Ok(port) if port >= 1 => port,
                        _ => return Err(usage()),
                    },
                });
            }
            "--verbose" => parsed.verbose = true,
            "--ui" => parsed.ui = Some(PathBuf::from(rest.next().ok_or_else(usage)?)),
            "--data" => data = Some(PathBuf::from(rest.next().ok_or_else(usage)?)),
            "--update-keygen" => {
                let file = PathBuf::from(rest.next().ok_or_else(usage)?);
                if parsed
                    .update
                    .replace(update_cli::Command::Keygen(file))
                    .is_some()
                {
                    return Err(usage());
                }
            }
            "--update-sign" => sign_image = Some(PathBuf::from(rest.next().ok_or_else(usage)?)),
            "--update-verify" => verify_file = Some(PathBuf::from(rest.next().ok_or_else(usage)?)),
            "--public-key" => verify_key = Some(rest.next().ok_or_else(usage)?.clone()),
            "--version" => sign_version = Some(rest.next().ok_or_else(usage)?.clone()),
            "--key" => sign_key = Some(PathBuf::from(rest.next().ok_or_else(usage)?)),
            "--out" => sign_out = Some(PathBuf::from(rest.next().ok_or_else(usage)?)),
            _ => return Err(usage()),
        }
    }
    let sign_given = [
        sign_image.is_some(),
        sign_version.is_some(),
        sign_key.is_some(),
        sign_out.is_some(),
    ];
    if let (Some(image), Some(version), Some(key), Some(out)) =
        (sign_image, sign_version, sign_key, sign_out)
    {
        let command = update_cli::Command::Sign {
            image,
            version,
            key,
            out,
        };
        if parsed.update.replace(command).is_some() {
            return Err(usage());
        }
    } else if sign_given.iter().any(|given| *given) {
        // One of `--update-sign`, `--version`, `--key`, `--out` without the others.
        return Err(usage());
    }
    match (verify_file, verify_key) {
        (Some(file), Some(public_key)) => {
            let command = update_cli::Command::Verify { file, public_key };
            if parsed.update.replace(command).is_some() {
                return Err(usage());
            }
        }
        (None, None) => {}
        // One of `--update-verify`, `--public-key` without the other.
        _ => return Err(usage()),
    }
    // The update tools are a task of their own: no radio, no other mode.
    if parsed.update.is_some()
        && (parsed.source.is_some()
            || parsed.pair.is_some()
            || parsed.listen
            || parsed.serve.is_some()
            || parsed.verbose
            || parsed.ui.is_some()
            || speed_given
            || data.is_some())
    {
        return Err(usage());
    }
    if (parsed.pair.is_some() || parsed.listen || parsed.serve.is_some()) && parsed.source.is_none()
    {
        return Err(usage());
    }
    // One mode at a time; `--verbose` belongs to `--serve`.
    let modes = [parsed.pair.is_some(), parsed.listen, parsed.serve.is_some()];
    if modes.iter().filter(|on| **on).count() > 1
        || (parsed.verbose && parsed.serve.is_none())
        || (parsed.ui.is_some() && parsed.serve.is_none())
    {
        return Err(usage());
    }
    if speed_given && !matches!(parsed.source, Some(Source::Sim(_))) {
        return Err(usage());
    }
    parsed.data = data.unwrap_or_else(|| {
        PathBuf::from(match parsed.source {
            Some(Source::Sim(_)) => DEFAULT_SIM_DATA_DIR,
            _ => DEFAULT_DATA_DIR,
        })
    });
    Ok(parsed)
}

fn clock() -> impl FnMut() -> u64 {
    let start = Instant::now();
    move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn connect(address: &str) -> Result<TcpTransport, String> {
    TcpTransport::connect(address)
        .map_err(|reason| format!("radio: cannot connect to {address}: {reason}"))
}

/// The identification of the radio: the lines to print.
fn identify<T: RadioTransport>(transport: T, label: &str) -> Result<[String; 4], String> {
    let identity = Radio::new(transport, clock())
        .identify()
        .map_err(|e| format!("radio: {e}"))?;
    Ok(identity_lines(label, &identity))
}

/// Starts the radio's network, opens pairing and shows what joins. `Ok(true)` when Ctrl-C
/// stopped it.
fn pair<T: RadioTransport>(
    transport: T,
    label: &str,
    seconds: u8,
    data: &Path,
    stop: &AtomicBool,
) -> Result<bool, String> {
    let mut random = SystemRandom::open()?;
    let mut fill = |buffer: &mut [u8]| random.fill(buffer);
    let store = FileStore::new(data);
    // The whole home is read (a device's zone must not be lost), but `--pair` writes the devices
    // only.
    let home = Mutex::new(store.load_home(serve::now_ms())?);
    let mut saver = Saver::devices_only(store, serve::now_ms()).with_sink(show_on_stderr);
    let mut radio = Radio::new(transport, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(label, &identity);
    // The `network:` line of the identification is left out: the radio was just reset, so it
    // always says "no network"; the line below replaces it.
    for line in &lines[..3] {
        println!("{line}");
    }

    let (network, foreign) = backup::start(&mut radio, data, &mut fill)?;
    println!("{}", network_line(&network, foreign));

    // Ctrl-C during the start: joining is never opened.
    if stop.load(Ordering::SeqCst) {
        return Ok(true);
    }
    radio
        .permit_join(seconds)
        .map_err(|e| format!("radio: {e}"))?;
    println!("pairing: open for {seconds} s");

    let deadline = Instant::now() + Duration::from_secs(u64::from(seconds));
    let mut closed = false;
    let result = (|| -> Result<bool, String> {
        loop {
            if stop.load(Ordering::SeqCst) {
                return Ok(true);
            }
            if !closed && Instant::now() >= deadline {
                radio.close_joining().map_err(|e| format!("radio: {e}"))?;
                closed = true;
            }
            // After the time is over, the identifications and reporting setups in progress
            // finish, at most `CLOSE_GRACE_SECONDS` later.
            if closed && (!radio.pairing_busy() || Instant::now() >= deadline + close_grace()) {
                // The time cap cut the setups in progress short: say which.
                for eui64 in radio.setups_in_progress() {
                    println!("{}", solite_core::radio::cut_short_line(&eui64));
                }
                return Ok(false);
            }
            let events = radio.poll_pairing().map_err(|e| format!("radio: {e}"))?;
            let activity = radio.take_activity();
            let routes = radio.take_routes();
            let now = serve::now_ms();
            {
                let mut guard = solite_core::devices::lock(&home);
                // The announces first: a device's power source is known when it is added.
                for item in &activity {
                    guard.devices.apply_activity(item, now);
                }
                for event in &events {
                    println!("{}", event.line());
                    for note in guard.devices.apply(event, now, &mut fill)? {
                        println!("{note}");
                    }
                }
                // After the events: a device is in the store once it was identified.
                for update in &routes {
                    guard.devices.set_route(update);
                }
            }
            saver.tick(&home, now, false)?;
        }
    })();
    // Joining is never left open: closed on Ctrl-C and on an error too (best effort).
    if !closed {
        let _ = radio.close_joining();
    }
    println!("pairing: closed");
    let saved = saver.tick(&home, serve::now_ms(), true);
    result.and_then(|stopped| saved.map(|()| stopped))
}

/// Resumes the radio's network, without forming one, and shows the measures it receives until
/// Ctrl-C. `Ok(true)` when Ctrl-C stopped it.
fn listen<T: RadioTransport>(
    transport: T,
    label: &str,
    data: &Path,
    stop: &AtomicBool,
) -> Result<bool, String> {
    let mut radio = Radio::new(transport, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(label, &identity);
    // The `network:` line of the identification is left out, as with `--pair`.
    for line in &lines[..3] {
        println!("{line}");
    }
    let network = radio.listen().map_err(|e| format!("radio: {e}"))?;
    let foreign = !backup::matches(data, &network);
    println!("{}", network_line(&network, foreign));
    println!("listening");
    while !stop.load(Ordering::SeqCst) {
        for event in radio.poll().map_err(|e| format!("radio: {e}"))? {
            for line in event.lines() {
                println!("{line}");
            }
        }
    }
    Ok(true)
}

/// What the host does once it has a radio.
enum Task {
    Identify,
    Pair(u8),
    Listen,
    Serve {
        port: u16,
        verbose: bool,
        ui: PathBuf,
    },
}

/// The simulated radio of a scenario file: its memory is `sim-<scenario name>.json` in the data
/// folder.
fn simulate(file: &str, data: &Path, speed: u32) -> Result<SimTransport<RealClock>, String> {
    let scenario = Scenario::load(Path::new(file))?;
    let name = Path::new(file)
        .file_stem()
        .map_or_else(|| "scenario".into(), |stem| stem.to_string_lossy());
    let memory = data.join(format!("sim-{name}.json"));
    SimTransport::for_scenario(&scenario, Some(&memory), speed, RealClock::new())
}

/// Runs a task on a radio. `Ok(true)` when Ctrl-C stopped it.
fn run<T: RadioTransport>(
    transport: T,
    label: &str,
    task: &Task,
    data: &Path,
    stop: &AtomicBool,
) -> Result<bool, String> {
    match task {
        Task::Identify => {
            for line in identify(transport, label)? {
                println!("{line}");
            }
            Ok(false)
        }
        Task::Pair(seconds) => pair(transport, label, *seconds, data, stop),
        Task::Listen => listen(transport, label, data, stop),
        Task::Serve { port, verbose, ui } => {
            serve::serve(transport, label, *port, *verbose, data, ui, stop)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(args) => args,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    logger::install();
    println!("{}", solite_core::banner());
    if let Some(command) = &args.update {
        return match update_cli::run(command) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(2)
            }
        };
    }
    let Some(source) = args.source else {
        return ExitCode::SUCCESS;
    };
    let task = match (args.pair, args.listen, args.serve) {
        (Some(seconds), _, _) => Task::Pair(seconds),
        (None, true, _) => Task::Listen,
        (None, false, Some(port)) => Task::Serve {
            port,
            verbose: args.verbose,
            ui: args
                .ui
                .clone()
                .unwrap_or_else(|| PathBuf::from(static_files::DEFAULT_UI_DIR)),
        },
        (None, false, None) => Task::Identify,
    };
    // Before the radio is touched: the data folder must be writable (listening writes nothing).
    if matches!(task, Task::Pair(_) | Task::Serve { .. })
        && let Err(message) = backup::check_writable(&args.data)
    {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    // A restore the last run left is finished first (spec 035, R6): before the home is read and
    // before the radio is started.
    if !matches!(task, Task::Identify) {
        backup_files::apply_pending(&args.data);
    }
    // The home must be readable before the radio is touched: a `devices.json`, `zones.json` or
    // `equipments.json` that does not parse stops the host, and is never overwritten.
    if matches!(task, Task::Pair(_) | Task::Serve { .. })
        && let Err(message) = FileStore::new(&args.data).load_home(serve::now_ms())
    {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let stop = Arc::new(AtomicBool::new(false));
    // Identifying is short and never stopped by Ctrl-C: the default behaviour ends it.
    if !matches!(task, Task::Identify) {
        let handler_stop = Arc::clone(&stop);
        let handler = move || {
            handler_stop.store(true, Ordering::SeqCst);
            // The HTTP thread of `--serve` waits for a request: wake it so it ends.
            serve::unblock();
        };
        if let Err(e) = ctrlc::set_handler(handler) {
            eprintln!("cannot watch for Ctrl-C: {e}");
            return ExitCode::from(1);
        }
    }
    let result = match &source {
        Source::Tcp(address) => connect(address).and_then(|transport| {
            run(
                transport,
                &format!("tcp://{address}"),
                &task,
                &args.data,
                &stop,
            )
        }),
        Source::Sim(file) => simulate(file, &args.data, args.speed)
            .and_then(|transport| run(transport, &format!("sim {file}"), &task, &args.data, &stop)),
    };
    match result {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(EXIT_INTERRUPTED),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}
