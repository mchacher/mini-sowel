//! `--recipe-script` (spec 041, R4.1): the script recipes the host loads before the instances
//! start, the line it prints for each, and the cases that stop it with exit status 2. The garage
//! served through a script is in `serve.rs`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const GARAGE: &str = "../../scenarios/garage.json";
const MOTION_LIGHT_SCRIPT: &str = "../../recipes/scripts/motion-light.js";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh folder for the data and the scripts of one test.
fn folder() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "solite-recipe-script-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn script(dir: &Path, name: &str, text: &str) -> String {
    let file = dir.join(name);
    std::fs::write(&file, text).unwrap();
    file.to_str().unwrap().to_string()
}

fn run(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args(["--sim", GARAGE, "--serve", "18099", "--data"])
        .arg(dir.join("data"))
        .args(args)
        .output()
        .expect("the host program starts")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn a_missing_file_stops_the_host_with_the_reason_and_exit_2() {
    let dir = folder();
    let missing = dir.join("nothing.js");
    let output = run(&["--recipe-script", missing.to_str().unwrap()], &dir);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let text = stderr(&output);
    assert!(
        text.contains("--recipe-script") && text.contains("nothing.js: cannot read it"),
        "{text}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("serving:"));
}

#[test]
fn a_refused_script_stops_the_host_with_script_recipe_invalid_and_exit_2() {
    let dir = folder();
    let cases = [
        ("syntax.js", "recipe({ api: 1, id: 'a', "),
        ("none.js", "var x = 1;"),
        (
            "api2.js",
            "recipe({ api: 2, id: 'a', name: 'A', slots: {}, start: function () {} });",
        ),
        // No recipe is built in (spec 046): the id `motion-light` loads like any other, as
        // `a_valid_script_is_loaded_before_serving...` shows with the real script.
        (
            "nostart.js",
            "recipe({ api: 1, id: 'a', name: 'A', slots: {} });",
        ),
    ];
    for (name, text) in cases {
        let file = script(&dir, name, text);
        let output = run(&["--recipe-script", &file], &dir);
        assert_eq!(output.status.code(), Some(2), "{name}: {}", stderr(&output));
        assert!(
            stderr(&output).contains("Script recipe invalid: "),
            "{name}: {}",
            stderr(&output)
        );
    }
    // Too large: 16 KB + 1.
    let big = script(&dir, "big.js", &format!("// {}\n", "x".repeat(16 * 1024)));
    let output = run(&["--recipe-script", &big], &dir);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Script recipe invalid: too large"));
}

#[test]
fn the_same_script_twice_is_refused_by_its_loaded_id() {
    let dir = folder();
    let output = run(
        &[
            "--recipe-script",
            MOTION_LIGHT_SCRIPT,
            "--recipe-script",
            MOTION_LIGHT_SCRIPT,
        ],
        &dir,
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("Script recipe invalid: "));
}

#[test]
fn a_third_script_is_refused_and_so_is_the_flag_without_serve() {
    let dir = folder();
    let files: Vec<String> = (0..3)
        .map(|n| {
            script(
                &dir,
                &format!("s{n}.js"),
                &format!(
                    "recipe({{ api: 1, id: 's{n}', name: 'S', slots: {{}}, start: function () {{}} }});"
                ),
            )
        })
        .collect();
    let mut args = Vec::new();
    for file in &files {
        args.extend(["--recipe-script", file.as_str()]);
    }
    let output = run(&args, &dir);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("at most 2"), "{}", stderr(&output));

    let output = Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args([
            "--sim",
            GARAGE,
            "--listen",
            "--recipe-script",
            MOTION_LIGHT_SCRIPT,
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).starts_with("--recipe-script needs --serve"));
    let output = Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args(["--sim", GARAGE, "--recipe-script", MOTION_LIGHT_SCRIPT])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_valid_script_is_loaded_before_serving_and_its_line_is_printed_once_each() {
    let dir = folder();
    let other = script(
        &dir,
        "plain.js",
        "recipe({ api: 1, id: 'plain', name: 'Plain', slots: {}, start: function () {} });",
    );
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args(["--sim", GARAGE, "--serve", &free])
        .arg("--data")
        .arg(dir.join("data"))
        .args([
            "--recipe-script",
            MOTION_LIGHT_SCRIPT,
            "--recipe-script",
            &other,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut shown = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        let Some(Ok(line)) = lines.next() else { break };
        let done = line.starts_with("serving:");
        shown.push(line);
        if done {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let scripts: Vec<&String> = shown
        .iter()
        .filter(|l| l.starts_with("recipes: script "))
        .collect();
    assert_eq!(scripts.len(), 2, "{shown:?}");
    assert!(
        scripts[0].starts_with(&format!(
            "recipes: script motion-light 1.1.2 from {MOTION_LIGHT_SCRIPT} ("
        )),
        "{}",
        scripts[0]
    );
    assert!(
        scripts[0].ends_with(" KB in its runtime)"),
        "{}",
        scripts[0]
    );
    assert!(scripts[0].contains(" KB of source, "), "{}", scripts[0]);
    assert!(
        scripts[1].starts_with("recipes: script plain - from "),
        "{}",
        scripts[1]
    );
    // Before the home is served.
    let line_at = shown
        .iter()
        .position(|l| l.starts_with("recipes: script "))
        .unwrap();
    let serving_at = shown
        .iter()
        .position(|l| l.starts_with("serving:"))
        .unwrap();
    assert!(line_at < serving_at, "{shown:?}");
}
