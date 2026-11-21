//! `--sim`: the host program on the simulated radio, from a scenario file.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

const SNZB02: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/snzb02.json");
const EUI: &str = "0x00124B0012345678";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh data folder name, not created.
fn data_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "mini-sowel-sim-host-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}

fn host() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
}

fn run(args: &[&str], cwd: Option<&Path>) -> Output {
    let mut command = host();
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().expect("the host program starts")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8 output")
}

fn lines(output: &Output) -> Vec<String> {
    text(&output.stdout).lines().map(str::to_string).collect()
}

#[test]
fn pair_on_the_snzb02_scenario_shows_the_device_its_reporting_and_its_measures() {
    let dir = data_dir();
    let dir_text = dir.to_str().unwrap();
    // Speed 20: the join (2 s) comes at once, the measures (every 30 s) every second and a half,
    // after the reporting is set up.
    let output = run(
        &[
            "--sim", SNZB02, "--pair", "5", "--speed", "20", "--data", dir_text,
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let out = lines(&output);
    assert_eq!(out[0], mini_sowel_core::banner());
    assert_eq!(out[1], format!("radio: sim {SNZB02}"));
    assert!(out[2].starts_with("ezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0"));
    assert_eq!(out[3], "radio address: 0x8C6FB9FFFE123456");
    assert!(out[4].starts_with("network: formed, channel 15"), "{out:?}");
    assert_eq!(out[5], "pairing: open for 5 s");
    assert!(out[6].starts_with(&format!("joined: {EUI} (0x")), "{out:?}");
    assert_eq!(out[7], format!("device: {EUI} eWeLink TH01"));
    assert_eq!(
        out[8],
        format!("reporting: {EUI} temperature, humidity, battery")
    );
    assert!(
        out.contains(&format!("{EUI} temperature 19.5 °C")),
        "{out:?}"
    );
    assert!(out.contains(&format!("{EUI} humidity 54.5 %")), "{out:?}");
    assert!(out.contains(&format!("{EUI} battery 100 %")), "{out:?}");
    assert_eq!(out.last().unwrap(), "pairing: closed");
    // The radio's memory and the host's backup are in the data folder.
    assert!(dir.join("sim-snzb02.json").is_file());
    assert!(dir.join("network.json").is_file());

    // `--listen` on the same scenario and data resumes the network and shows measures.
    let mut child = host()
        .args([
            "--sim", SNZB02, "--listen", "--speed", "100", "--data", dir_text,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("the host program starts");
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
    while !shown.iter().any(|l: &String| l.contains("temperature")) {
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
    assert_eq!(shown[1], format!("radio: sim {SNZB02}"));
    assert!(
        shown[4].starts_with("network: resumed, channel 15"),
        "{shown:?}"
    );
    assert_eq!(shown[5], "listening");
    assert_eq!(shown[6], format!("{EUI} temperature 19.5 °C"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn sim_with_radio_speed_without_sim_or_speed_out_of_range_prints_the_usage_and_exits_2() {
    let cases: [&[&str]; 9] = [
        &["--sim", SNZB02, "--radio", "tcp://127.0.0.1:1"],
        &["--radio", "tcp://127.0.0.1:1", "--sim", SNZB02],
        &["--radio", "tcp://127.0.0.1:1", "--speed", "10"],
        &["--speed", "10"],
        &["--sim", SNZB02, "--speed", "0"],
        &["--sim", SNZB02, "--speed", "101"],
        &["--sim", SNZB02, "--speed", "1.5"],
        &["--sim", SNZB02, "--speed", "fast"],
        &["--sim"],
    ];
    for args in cases {
        let output = run(args, None);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let usage = text(&output.stderr);
        assert!(
            usage.contains("--radio") && usage.contains("--sim"),
            "{usage}"
        );
        assert!(text(&output.stdout).is_empty(), "{args:?}");
    }
}

#[test]
fn a_missing_scenario_file_is_a_one_line_error_and_exit_1() {
    let missing = data_dir().join("nothing.json");
    let file = missing.to_str().unwrap();
    let output = run(&["--sim", file], None);
    assert_eq!(output.status.code(), Some(1));
    let error = text(&output.stderr);
    assert_eq!(error.lines().count(), 1, "{error}");
    assert!(error.starts_with(&format!("scenario: {file}: ")), "{error}");
}

#[test]
fn sim_without_data_writes_under_mini_sowel_data_sim_and_never_touches_the_real_backup() {
    let cwd = data_dir();
    std::fs::create_dir_all(&cwd).unwrap();
    let output = run(
        &["--sim", SNZB02, "--pair", "2", "--speed", "100"],
        Some(&cwd),
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let sim = cwd.join("mini-sowel-data/sim");
    assert!(sim.join("network.json").is_file());
    assert!(sim.join("sim-snzb02.json").is_file());
    assert!(
        !cwd.join("mini-sowel-data/network.json").exists(),
        "the real radio's backup is never touched"
    );
    std::fs::remove_dir_all(&cwd).unwrap();
}

#[test]
fn a_formed_network_in_the_scenario_is_resumed_as_made_by_another_program() {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let scenario = dir.join("formed.json");
    let formed = std::fs::read_to_string(SNZB02)
        .unwrap()
        .replace("\"none\"", "\"formed\"");
    std::fs::write(&scenario, formed).unwrap();
    let data = dir.join("data");
    let output = run(
        &[
            "--sim",
            scenario.to_str().unwrap(),
            "--pair",
            "1",
            "--speed",
            "100",
            "--data",
            data.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let out = lines(&output);
    assert_eq!(
        out[4],
        "network: resumed (made by another program), channel 15, PAN 0x5A5A, \
         extended PAN 0x5A5A5A5A5A5A5A5A"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

const GARAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/garage.json");

#[test]
fn pair_on_the_garage_scenario_enrolls_the_motion_sensor_and_says_so() {
    let dir = data_dir();
    let output = run(
        &[
            "--sim",
            GARAGE,
            "--pair",
            "8",
            "--speed",
            "20",
            "--data",
            dir.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let out = lines(&output);
    assert!(
        out.contains(&"reporting: 0x00124B0012345679 battery, enrolled (motion)".to_string()),
        "{out:#?}"
    );
    // The sensor's motion reaches the host once it is enrolled.
    assert!(out.contains(&"0x00124B0012345679 alarm true".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}
