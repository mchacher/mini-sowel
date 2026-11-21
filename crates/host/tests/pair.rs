//! `--pair` against a fake radio listening on a local TCP port, speaking real ASH and EZSP.

#[path = "../../core/tests/common/mod.rs"]
mod common;

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use common::{FakeDevice, FakeNcp, StoredNetwork};

/// Serves one connection with the fake radio and gives the radio back when it closes, so a second
/// run can find what the first one left in it.
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

/// A fresh data folder name, not created.
fn data_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mini-sowel-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

fn run(args: &[&str]) -> (Output, Duration) {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .args(args)
        .output()
        .expect("the host program starts");
    (output, start.elapsed())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8 output")
}

fn pair_args<'a>(url: &'a str, seconds: &'a str, dir: &'a str) -> Vec<&'a str> {
    vec!["--radio", url, "--pair", seconds, "--data", dir]
}

fn lines(output: &Output) -> Vec<String> {
    text(&output.stdout).lines().map(str::to_string).collect()
}

fn sensor_radio() -> FakeNcp {
    let mut ncp = FakeNcp::new();
    ncp.device = Some(FakeDevice::sensor());
    ncp
}

/// The network line's PAN and extended PAN, after checking its shape.
fn network_ids(line: &str, state: &str) -> (String, String) {
    let prefix = format!("network: {state}, channel 15, PAN 0x");
    let rest = line
        .strip_prefix(&prefix)
        .unwrap_or_else(|| panic!("{line}"));
    let (pan, extended) = rest.split_once(", extended PAN 0x").expect(line);
    assert_eq!(pan.len(), 4, "{line}");
    assert_eq!(extended.len(), 16, "{line}");
    assert!(
        pan.chars()
            .chain(extended.chars())
            .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)),
        "{line}"
    );
    (pan.to_string(), extended.to_string())
}

#[test]
fn pair_with_one_device_shows_the_lines_of_the_spec_writes_a_private_backup_and_exits_0() {
    let (address, handle) = serve(sensor_radio());
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let dir_text = dir.to_str().unwrap();
    let (output, elapsed) = run(&pair_args(&url, "1", dir_text));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));

    let out = lines(&output);
    assert_eq!(out.len(), 13, "{out:?}");
    assert_eq!(out[0], mini_sowel_core::banner());
    assert_eq!(out[1], format!("radio: {url}"));
    assert_eq!(out[2], "ezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0");
    assert_eq!(out[3], "radio address: 0x8C6FB9FFFE123456");
    network_ids(&out[4], "formed");
    assert_eq!(out[5], "pairing: open for 1 s");
    assert_eq!(out[6], "joined: 0x00124B0012345678 (0x5C3D)");
    assert_eq!(out[7], "device: 0x00124B0012345678 eWeLink TH01");
    assert_eq!(
        out[8],
        "reporting: 0x00124B0012345678 temperature, humidity, battery"
    );
    assert_eq!(out[9], "0x00124B0012345678 temperature 19.52 °C");
    assert_eq!(out[10], "0x00124B0012345678 humidity 54.1 %");
    assert_eq!(out[11], "0x00124B0012345678 battery 100 %");
    assert_eq!(out[12], "pairing: closed");
    assert!(elapsed < Duration::from_secs(8), "{elapsed:?}");

    // The backup: mode 0600, the network's keys, and none of them on the screen.
    let file = dir.join("network.json");
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let json = std::fs::read_to_string(&file).unwrap();
    let ncp = handle.join().unwrap();
    let (network_key, trust_center_key) = ncp.keys.expect("the radio received its keys");
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    assert!(json.contains(&hex(&network_key)), "{json}");
    assert!(json.contains(&hex(&trust_center_key)), "{json}");
    let screen = text(&output.stdout) + &text(&output.stderr);
    assert!(!screen.contains(&hex(&network_key)));
    assert!(!screen.contains(&hex(&trust_center_key)));
    // The backup says the same network as the screen.
    let (pan, extended) = network_ids(&out[4], "formed");
    assert!(json.contains(&format!("\"panId\": \"0x{pan}\"")), "{json}");
    assert!(
        json.contains(&format!("\"extendedPanId\": \"0x{extended}\"")),
        "{json}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pair_twice_on_the_same_radio_resumes_the_network_and_leaves_the_backup_unchanged() {
    let dir = data_dir();
    let dir_text = dir.to_str().unwrap();

    let (address, handle) = serve(sensor_radio());
    let url = format!("tcp://{address}");
    let (first, _) = run(&pair_args(&url, "1", dir_text));
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let ncp = handle.join().unwrap();
    let backup = std::fs::read(dir.join("network.json")).unwrap();

    let (address, handle) = serve(ncp);
    let url = format!("tcp://{address}");
    let (second, _) = run(&pair_args(&url, "1", dir_text));
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    let first_line = lines(&first)[4].clone();
    let second_line = lines(&second)[4].clone();
    let formed = network_ids(&first_line, "formed");
    let resumed = network_ids(&second_line, "resumed");
    assert_eq!(formed, resumed, "the same network");
    assert_eq!(std::fs::read(dir.join("network.json")).unwrap(), backup);

    // The radio was asked to form once, and its endpoint added once.
    let ncp = handle.join().unwrap();
    let count = |id: u16| ncp.commands.iter().filter(|c| c.0 == id).count();
    assert_eq!(count(0x1E), 1);
    assert_eq!(count(0x02), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_coordinator_network_without_a_backup_is_resumed_as_made_by_another_program() {
    let mut ncp = FakeNcp::new();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [1, 2, 3, 4, 5, 6, 7, 8],
        pan_id: 0x4321,
        channel: 20,
        node_type: 1,
    });
    let (address, handle) = serve(ncp);
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let (output, _) = run(&pair_args(&url, "1", dir.to_str().unwrap()));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        lines(&output)[4],
        "network: resumed (made by another program), channel 20, PAN 0x4321, \
         extended PAN 0x0807060504030201"
    );
    // Used, never destroyed, never backed up.
    let ncp = handle.join().unwrap();
    assert!(!ncp.commands.iter().any(|c| c.0 == 0x1E || c.0 == 0x20));
    assert!(!dir.join("network.json").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_data_folder_that_cannot_be_written_gives_cannot_write_exit_1_and_the_radio_is_not_touched() {
    // The parent of the data folder is a file.
    let file = data_dir();
    std::fs::write(&file, b"not a folder").unwrap();
    let dir = file.join("data");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("tcp://{}", listener.local_addr().unwrap());
    let (output, _) = run(&pair_args(&url, "1", dir.to_str().unwrap()));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr).trim(),
        format!("cannot write {}", dir.display())
    );
    // Nobody connected: no byte reached the radio.
    assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
    std::fs::remove_file(&file).unwrap();
}

#[test]
fn ctrl_c_during_pairing_closes_joining_and_exits_130() {
    let (address, handle) = serve(FakeNcp::new());
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let mut child = Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .args(pair_args(&url, "30", dir.to_str().unwrap()))
        .stdout(Stdio::piped())
        .spawn()
        .expect("the host program starts");
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            stdout.read_line(&mut line).unwrap() > 0,
            "the host stopped before pairing opened"
        );
        if line.starts_with("pairing: open for 30 s") {
            break;
        }
    }
    let signal = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(signal.success());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(130));

    let ncp = handle.join().unwrap();
    // After the last permitJoining 30: permitJoining 0, transient keys cleared, rejoins only.
    let open = ncp
        .commands
        .iter()
        .rposition(|c| c.0 == 0x22 && c.1 == vec![30])
        .expect("pairing was opened");
    assert_eq!(
        ncp.commands[open + 1..].to_vec(),
        vec![(0x22, vec![0]), (0x6B, vec![]), (0x55, vec![0, 0x02])]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pair_with_zero_a_word_or_too_long_or_without_a_radio_prints_the_usage_and_exits_2() {
    for args in [
        &["--radio", "tcp://127.0.0.1:1", "--pair", "0"][..],
        &["--radio", "tcp://127.0.0.1:1", "--pair", "abc"],
        &["--radio", "tcp://127.0.0.1:1", "--pair", "255"],
        &["--pair", "5"],
        &["--radio", "tcp://127.0.0.1:1", "--data"],
    ] {
        let (output, _) = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(text(&output.stderr).contains("--pair"), "{args:?}");
    }
}

#[test]
fn without_pair_the_host_only_identifies_and_does_not_start_the_network() {
    let (address, handle) = serve(FakeNcp::new());
    let url = format!("tcp://{address}");
    let (output, _) = run(&["--radio", &url]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(lines(&output).len(), 5);
    assert_eq!(lines(&output)[4], "network: no network");
    let ncp = handle.join().unwrap();
    assert!(!ncp.commands.iter().any(|c| c.0 == 0x17 || c.0 == 0x1E));
    assert!(!PathBuf::from("mini-sowel-data").exists());
}

#[test]
fn a_step_failing_after_the_form_leaves_the_backup_written_and_the_error_shown() {
    let mut ncp = FakeNcp::new();
    ncp.refuse_concentrator = true;
    let (address, handle) = serve(ncp);
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let (output, _) = run(&pair_args(&url, "1", dir.to_str().unwrap()));
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("radio: the radio refused setConcentrator"),
        "{}",
        text(&output.stderr)
    );
    // The keys the radio holds are in the backup.
    let json = std::fs::read_to_string(dir.join("network.json")).unwrap();
    let ncp = handle.join().unwrap();
    let (network_key, _) = ncp.keys.unwrap();
    let hex: String = network_key.iter().map(|b| format!("{b:02X}")).collect();
    assert!(json.contains(&hex));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_backup_that_cannot_be_written_after_the_form_is_the_first_error_and_the_start_stops() {
    let (address, handle) = serve(FakeNcp::new());
    let url = format!("tcp://{address}");
    let dir = data_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // network.json.tmp is a folder: the write fails after the folder check passed.
    std::fs::create_dir(dir.join("network.json.tmp")).unwrap();
    let (output, _) = run(&pair_args(&url, "1", dir.to_str().unwrap()));
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(
        stderr.trim().ends_with(&format!(
            "cannot write {}",
            dir.join("network.json").display()
        )),
        "{stderr}"
    );
    assert!(!stderr.contains("radio:"));
    let ncp = handle.join().unwrap();
    assert!(
        !ncp.commands.iter().any(|c| c.0 == 0x28),
        "the start went on"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_backup_of_another_network_is_kept_renamed_with_its_pan_id_before_the_new_one_is_written() {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let old = "{\n  \"panId\": \"0x4321\",\n  \"extendedPanId\": \"0x0102030405060708\",\n  \
               \"channel\": 20\n}\n";
    std::fs::write(dir.join("network.json"), old).unwrap();
    let (address, handle) = serve(FakeNcp::new());
    let url = format!("tcp://{address}");
    let (output, _) = run(&pair_args(&url, "1", dir.to_str().unwrap()));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    handle.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("network-4321.json")).unwrap(),
        old
    );
    let new = std::fs::read_to_string(dir.join("network.json")).unwrap();
    assert!(new.contains("networkKey"), "{new}");
    assert!(!new.contains("0x4321"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn warnings_of_the_core_are_shown_on_stderr() {
    let mut ncp = FakeNcp::new();
    ncp.overflow_flag = true;
    let (address, handle) = serve(ncp);
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let (output, _) = run(&pair_args(&url, "1", dir.to_str().unwrap()));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(
        text(&output.stderr).contains("warn: the radio reports an overflow"),
        "{}",
        text(&output.stderr)
    );
    handle.join().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_c_during_the_start_exits_130_without_opening_joining() {
    let mut ncp = FakeNcp::new();
    ncp.network_up_delay_ms = 2_000; // the start takes a while
    let (address, handle) = serve(ncp);
    let url = format!("tcp://{address}");
    let dir = data_dir();
    let mut child = Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .args(pair_args(&url, "30", dir.to_str().unwrap()))
        .stdout(Stdio::piped())
        .spawn()
        .expect("the host program starts");
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(stdout.read_line(&mut line).unwrap() > 0);
        if line.starts_with("radio address:") {
            break;
        }
    }
    let signal = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(signal.success());
    assert_eq!(child.wait().unwrap().code(), Some(130));
    let ncp = handle.join().unwrap();
    assert!(
        ncp.commands.iter().any(|c| c.0 == 0x1E || c.0 == 0x17),
        "it did start"
    );
    assert!(
        !ncp.commands.iter().any(|c| c.0 == 0x22),
        "joining was opened"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
