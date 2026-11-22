//! `--serve` starts the network (spec 017, R3) against a fake radio on a local TCP port: the
//! errors of the start are the texts of `--pair`, with exit 1 and nothing served; a coordinator
//! network the host has no backup of is resumed, never destroyed, and serving goes on.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use solite_sim::{Ncp, StoredNetwork};

/// Serves one connection with the fake radio and gives the radio back when it closes, so a second
/// run can find what the first one left in it.
fn serve(ncp: Ncp) -> (SocketAddr, JoinHandle<Ncp>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut ncp = ncp;
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let start = Instant::now();
        let mut buf = [0u8; 512];
        loop {
            let mut answer = match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => ncp.on_bytes(&buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    Vec::new()
                }
                Err(_) => break,
            };
            // Time passes: callbacks that are due go out.
            answer.extend(ncp.on_tick(u64::try_from(start.elapsed().as_millis()).unwrap()));
            if !answer.is_empty() && stream.write_all(&answer).is_err() {
                break;
            }
        }
        ncp
    });
    (address, handle)
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn data_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "solite-serve-start-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn stored(node_type: u8) -> Ncp {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [1, 2, 3, 4, 5, 6, 7, 8],
        pan_id: 0x4321,
        channel: 20,
        node_type,
    });
    ncp
}

fn serve_command(address: SocketAddr, dir: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_solite-host"));
    command
        .args([
            "--radio",
            &format!("tcp://{address}"),
            "--serve",
            &free_port().to_string(),
            "--data",
        ])
        .arg(dir);
    command
}

#[test]
fn a_radio_that_is_not_a_coordinator_stops_serve_with_the_error_and_exit_1() {
    let (address, _handle) = serve(stored(2));
    let dir = data_dir();
    let output = serve_command(address, &dir).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "radio: the radio is not a coordinator"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("serving:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "slow (10 s, the production wait for the network): sh scripts/validate.sh runs it"]
fn a_network_that_does_not_come_up_stops_serve_with_the_error_and_exit_1() {
    let mut ncp = Ncp::recording();
    ncp.no_network_up = true;
    let (address, _handle) = serve(ncp);
    let dir = data_dir();
    let output = serve_command(address, &dir).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "radio: network did not start"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("serving:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_coordinator_network_without_a_backup_is_resumed_named_and_never_destroyed_while_serving() {
    let (address, handle) = serve(stored(1));
    let dir = data_dir();
    let mut child = serve_command(address, &dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let start = Instant::now();
    let mut shown = Vec::new();
    while let Some(Ok(line)) = lines.next() {
        let ready = line.starts_with("serving:");
        shown.push(line);
        if ready || start.elapsed() > Duration::from_secs(20) {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        shown.iter().any(|l| l
            == "network: resumed (made by another program), channel 20, PAN 0x4321, \
                extended PAN 0x0807060504030201"),
        "{shown:?}"
    );
    assert!(shown.last().is_some_and(|l| l.starts_with("serving:")));
    let ncp = handle.join().unwrap();
    assert!(!ncp.commands.iter().any(|c| c.0 == 0x1E || c.0 == 0x20));
    assert!(!dir.join("network.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
