//! mini-Sowel on the Mac.

mod radio_tcp;

use std::process::ExitCode;
use std::time::Instant;

use mini_sowel_core::radio::{Radio, identity_lines};

use radio_tcp::TcpTransport;

const USAGE: &str = "usage: mini-sowel-host [--radio tcp://<address>:<port>]";

/// Reads the command line: the radio's `host:port`, if `--radio` is given.
fn parse_args(args: &[String]) -> Result<Option<String>, String> {
    match args {
        [] => Ok(None),
        [flag, value] if flag == "--radio" => {
            let address = value
                .strip_prefix("tcp://")
                .ok_or_else(|| USAGE.to_string())?;
            let (host, port) = address.rsplit_once(':').ok_or_else(|| USAGE.to_string())?;
            if host.is_empty() || port.parse::<u16>().is_err() {
                return Err(USAGE.to_string());
            }
            Ok(Some(address.to_string()))
        }
        _ => Err(USAGE.to_string()),
    }
}

/// Connects to the radio and returns the lines to print.
fn identify(address: &str) -> Result<[String; 4], String> {
    let transport = TcpTransport::connect(address)
        .map_err(|reason| format!("radio: cannot connect to {address}: {reason}"))?;
    let start = Instant::now();
    let clock = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let identity = Radio::new(transport, clock)
        .identify()
        .map_err(|e| format!("radio: {e}"))?;
    Ok(identity_lines(&format!("tcp://{address}"), &identity))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let address = match parse_args(&args) {
        Ok(address) => address,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    println!("{}", mini_sowel_core::banner());
    let Some(address) = address else {
        return ExitCode::SUCCESS;
    };
    match identify(&address) {
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
    }
}
