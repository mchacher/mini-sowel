//! mini-Sowel on the Mac.

mod backup;
mod logger;
mod radio_tcp;
mod random;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use mini_sowel_core::radio::{NetworkSettings, Radio, RadioError, identity_lines, network_line};

use radio_tcp::TcpTransport;
use random::SystemRandom;

const USAGE: &str = "usage: mini-sowel-host [--radio tcp://<address>:<port> \
[--pair [seconds] | --listen] [--data <dir>]]";
/// Pairing time when `--pair` has no number.
const DEFAULT_PAIR_SECONDS: u8 = 180;
const DEFAULT_DATA_DIR: &str = "mini-sowel-data";
/// After the pairing time is over, identifications and reporting setups in progress may go on
/// this long, in seconds.
const CLOSE_GRACE_SECONDS: u64 = 70;
/// What a shell reports for a program stopped by Ctrl-C.
const EXIT_INTERRUPTED: u8 = 130;

#[derive(Debug, PartialEq, Eq)]
struct Args {
    /// The radio's `host:port`.
    address: Option<String>,
    /// Pairing time in seconds, if `--pair` is given.
    pair: Option<u8>,
    /// Show the measures of the paired devices, without pairing.
    listen: bool,
    data: PathBuf,
}

/// Reads the command line.
fn parse_args(args: &[String]) -> Result<Args, String> {
    let usage = || USAGE.to_string();
    let mut parsed = Args {
        address: None,
        pair: None,
        listen: false,
        data: PathBuf::from(DEFAULT_DATA_DIR),
    };
    let mut rest = args.iter().peekable();
    while let Some(flag) = rest.next() {
        match flag.as_str() {
            "--radio" => {
                let value = rest.next().ok_or_else(usage)?;
                let address = value.strip_prefix("tcp://").ok_or_else(usage)?;
                let (host, port) = address.rsplit_once(':').ok_or_else(usage)?;
                if host.is_empty() || port.parse::<u16>().is_err() {
                    return Err(usage());
                }
                parsed.address = Some(address.to_string());
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
            "--data" => parsed.data = PathBuf::from(rest.next().ok_or_else(usage)?),
            _ => return Err(usage()),
        }
    }
    if (parsed.pair.is_some() || parsed.listen) && parsed.address.is_none() {
        return Err(usage());
    }
    if parsed.pair.is_some() && parsed.listen {
        return Err(usage());
    }
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

/// Connects to the radio and returns the lines to print.
fn identify(address: &str) -> Result<[String; 4], String> {
    let identity = Radio::new(connect(address)?, clock())
        .identify()
        .map_err(|e| format!("radio: {e}"))?;
    Ok(identity_lines(&format!("tcp://{address}"), &identity))
}

/// Starts the radio's network, opens pairing and shows what joins. `Ok(true)` when Ctrl-C
/// stopped it.
fn pair(address: &str, seconds: u8, data: &Path, stop: &AtomicBool) -> Result<bool, String> {
    let mut random = SystemRandom::open()?;
    let mut radio = Radio::new(connect(address)?, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(&format!("tcp://{address}"), &identity);
    // The `network:` line of the identification is left out: the radio was just reset, so it
    // always says "no network"; the line below replaces it.
    for line in &lines[..3] {
        println!("{line}");
    }

    let network = radio
        .start_network(
            &NetworkSettings::default(),
            &mut |buffer| random.fill(buffer),
            // The backup is written the moment the radio accepts the network, before any
            // later step that can fail: the keys must not be lost.
            &mut |network| match &network.keys {
                Some(keys) => backup::write(data, network, keys),
                None => Ok(()),
            },
        )
        .map_err(|e| match e {
            // The backup's own message, as it is.
            RadioError::OnFormed(message) => message,
            other => format!("radio: {other}"),
        })?;
    let foreign = network.keys.is_none() && !backup::matches(data, &network);
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
            if closed
                && (!radio.pairing_busy()
                    || Instant::now() >= deadline + Duration::from_secs(CLOSE_GRACE_SECONDS))
            {
                return Ok(false);
            }
            for event in radio.poll_pairing().map_err(|e| format!("radio: {e}"))? {
                println!("{}", event.line());
            }
        }
    })();
    // Joining is never left open: closed on Ctrl-C and on an error too (best effort).
    if !closed {
        let _ = radio.close_joining();
    }
    println!("pairing: closed");
    result
}

/// Resumes the radio's network, without forming one, and shows the measures it receives until
/// Ctrl-C. `Ok(true)` when Ctrl-C stopped it.
fn listen(address: &str, data: &Path, stop: &AtomicBool) -> Result<bool, String> {
    let mut radio = Radio::new(connect(address)?, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(&format!("tcp://{address}"), &identity);
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
    println!("{}", mini_sowel_core::banner());
    let Some(address) = args.address else {
        return ExitCode::SUCCESS;
    };
    let Some(seconds) = args.pair.or(args.listen.then_some(0)) else {
        return match identify(&address) {
            Ok(lines) => {
                for line in lines {
                    println!("{line}");
                }
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        };
    };
    // Before the radio is touched: the data folder must be writable (listening writes nothing).
    if !args.listen
        && let Err(message) = backup::check_writable(&args.data)
    {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let stop = Arc::new(AtomicBool::new(false));
    let handler_stop = Arc::clone(&stop);
    if let Err(e) = ctrlc::set_handler(move || handler_stop.store(true, Ordering::SeqCst)) {
        eprintln!("cannot watch for Ctrl-C: {e}");
        return ExitCode::from(1);
    }
    let result = if args.listen {
        listen(&address, &args.data, &stop)
    } else {
        pair(&address, seconds, &args.data, &stop)
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
