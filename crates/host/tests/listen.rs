//! `--pair` (reporting and measures) and `--listen` against a fake radio listening on a local TCP
//! port, speaking real ASH and EZSP, with the fake SNZB-02 that reports.

#[path = "../../core/tests/common/mod.rs"]
mod common;

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use common::{FakeDevice, FakeNcp};

/// Serves one connection with the fake radio and gives the radio back when it closes.
fn serve(ncp: FakeNcp) -> (SocketAddr, JoinHandle<FakeNcp>) {
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

/// A fresh data folder name, not created.
fn data_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mini-sowel-listen-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .args(args)
        .output()
        .expect("the host program starts")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8 output")
}

fn lines(output: &Output) -> Vec<String> {
    text(&output.stdout).lines().map(str::to_string).collect()
}

fn sensor_radio() -> FakeNcp {
    let mut ncp = FakeNcp::new();
    ncp.device = Some(FakeDevice::sensor());
    ncp
}

const EUI: &str = "0x00124B0012345678";

#[test]
fn pair_with_the_fake_sensor_shows_the_reporting_line_then_the_measure_lines() {
    let (address, handle) = serve(sensor_radio());
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let output = run(&[
        "--radio",
        &url,
        "--pair",
        "1",
        "--data",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let out = lines(&output);
    let device = out
        .iter()
        .position(|l| l == &format!("device: {EUI} eWeLink TH01"))
        .expect("the device line");
    assert_eq!(
        out[device + 1..],
        [
            format!("reporting: {EUI} temperature, humidity, battery"),
            format!("{EUI} temperature 19.52 °C"),
            format!("{EUI} humidity 54.1 %"),
            format!("{EUI} battery 100 %"),
            "pairing: closed".to_string(),
        ]
    );
    handle.join().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_second_run_with_listen_resumes_the_network_and_shows_the_reports_until_ctrl_c_exits_130() {
    let dir = data_dir();
    let dir_text = dir.to_str().unwrap();
    let (address, handle) = serve(sensor_radio());
    let url = format!("tcp://{address}");
    let first = run(&["--radio", &url, "--pair", "1", "--data", dir_text]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let network = lines(&first)
        .into_iter()
        .find(|l| l.starts_with("network: formed"))
        .unwrap()
        .replacen("formed", "resumed", 1);
    let ncp = handle.join().unwrap();
    let earlier = ncp.commands.len();

    let (address, handle) = serve(ncp);
    let url = format!("tcp://{address}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .args(["--radio", &url, "--listen", "--data", dir_text])
        .stdout(Stdio::piped())
        .spawn()
        .expect("the host program starts");
    // The lines are read on another thread, so that a host that stays silent fails the test
    // after a few seconds instead of blocking it.
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let (sender, receiver) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut shown = Vec::new();
    while shown.len() < 9 {
        match receiver.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => shown.push(line),
            Err(_) => {
                let _ = child.kill();
                panic!("the host stopped or stayed silent: {shown:?}");
            }
        }
    }
    let signal = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(signal.success());
    assert_eq!(child.wait().unwrap().code(), Some(130));

    assert_eq!(shown[0], mini_sowel_core::banner());
    assert_eq!(shown[4], network);
    assert_eq!(
        shown[5..],
        [
            "listening".to_string(),
            format!("{EUI} temperature 19.52 °C"),
            format!("{EUI} humidity 54.1 %"),
            format!("{EUI} battery 100 %"),
        ]
    );
    // It resumed: it did not form a network, did not open joining.
    let ncp = handle.join().unwrap();
    let later = &ncp.commands[earlier..];
    assert!(!later.iter().any(|c| c.0 == 0x1E || c.0 == 0x22));
    assert!(later.iter().any(|c| c.0 == 0x17));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn listen_on_a_fake_radio_with_no_network_says_pair_a_device_first_and_exits_1() {
    let (address, handle) = serve(FakeNcp::new());
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let output = run(&["--radio", &url, "--listen", "--data", dir.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr).trim(),
        "radio: no network, pair a device first"
    );
    assert!(!text(&output.stdout).contains("listening"));
    let ncp = handle.join().unwrap();
    assert!(!ncp.commands.iter().any(|c| c.0 == 0x1E));
    assert!(!dir.exists(), "listening writes nothing");
}

#[test]
fn listen_with_pair_or_without_a_radio_prints_the_usage_and_exits_2() {
    for args in [
        &["--radio", "tcp://127.0.0.1:1", "--listen", "--pair"][..],
        &["--radio", "tcp://127.0.0.1:1", "--pair", "5", "--listen"],
        &["--listen"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(text(&output.stderr).contains("--listen"), "{args:?}");
    }
}
