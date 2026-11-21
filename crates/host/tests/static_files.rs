//! `--serve` serves the interface's files: content types, caching, security headers, the path
//! checks as the host sees them, the start line when the interface is not built, and `--ui`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const GARAGE: &str = "../../scenarios/garage.json";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A folder that is removed when the test ends.
struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(kind: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "solite-static-{kind}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn host() -> Command {
    Command::new(env!("CARGO_BIN_EXE_solite-host"))
}

/// A host program that is killed and reaped when the test ends, however it ends.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Answer {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

struct Serving {
    _child: Reaped,
    port: u16,
    shown: Vec<String>,
    _data: TempDir,
}

/// The interface's folder, with an `index.html`, a script, a style, a svg, a font and an unknown file.
fn built_ui() -> TempDir {
    let ui = temp_dir("ui");
    std::fs::create_dir_all(ui.join("assets")).unwrap();
    std::fs::write(ui.join("index.html"), "<!doctype html><title>x</title>").unwrap();
    for (name, content) in [
        ("index-abc.js", "console.log(1)"),
        ("index-abc.css", "a{}"),
        ("logo.svg", "<svg/>"),
        ("font.woff2", "wOF2"),
        ("data.bin", "xx"),
    ] {
        std::fs::write(ui.join("assets").join(name), content).unwrap();
    }
    ui
}

/// `--serve` on the garage scenario; `ui` is passed to `--ui` when given. `lines` come until
/// the `serving:` line.
fn serve(ui: Option<&Path>) -> Serving {
    let data = temp_dir("data");
    // The network is formed once (`--pair`), as the serve tests do.
    let paired = host()
        .env("SOLITE_CLOSE_GRACE_SECONDS", "1")
        .args(["--sim", GARAGE, "--pair", "2", "--speed", "100", "--data"])
        .arg(&*data)
        .output()
        .expect("the host program starts");
    assert!(paired.status.success(), "{paired:?}");
    let port = free_port();
    let mut command = host();
    command
        .args([
            "--sim",
            GARAGE,
            "--serve",
            &port.to_string(),
            "--speed",
            "100",
        ])
        .arg("--data")
        .arg(&*data);
    if let Some(ui) = ui {
        command.arg("--ui").arg(ui);
    }
    let mut child = Reaped(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the host program starts"),
    );
    let stdout = BufReader::new(child.0.stdout.take().unwrap());
    let (sender, lines): (_, Receiver<String>) = mpsc::channel();
    thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut shown = Vec::new();
    loop {
        let line = lines
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("the host stopped or stayed silent: {shown:?}"));
        let ready = line.starts_with("serving:");
        shown.push(line);
        if ready {
            break;
        }
    }
    Serving {
        _child: child,
        port,
        shown,
        _data: data,
    }
}

impl Serving {
    fn request(&self, method: &str, path: &str, host: &str) -> Answer {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let at = bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("an HTTP answer");
        let head = String::from_utf8_lossy(&bytes[..at]).into_owned();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        Answer {
            status,
            head,
            body: bytes[at + 4..].to_vec(),
        }
    }

    fn get(&self, path: &str) -> Answer {
        self.request("GET", path, &format!("127.0.0.1:{}", self.port))
    }
}

#[test]
fn the_index_is_served_with_its_bytes_its_type_and_no_cache() {
    let ui = built_ui();
    let serving = serve(Some(&ui));
    let answer = serving.get("/");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.header("Content-Type").as_deref(), Some("text/html"));
    assert_eq!(answer.header("Cache-Control").as_deref(), Some("no-cache"));
    assert_eq!(answer.body, std::fs::read(ui.join("index.html")).unwrap());
    assert!(!serving.shown.iter().any(|l| l.starts_with("interface:")));
}

#[test]
fn the_assets_have_their_content_types_and_are_immutable() {
    let ui = built_ui();
    let serving = serve(Some(&ui));
    for (name, kind) in [
        ("index-abc.js", "application/javascript"),
        ("index-abc.css", "text/css"),
        ("logo.svg", "image/svg+xml"),
        ("font.woff2", "font/woff2"),
        ("data.bin", "application/octet-stream"),
    ] {
        let answer = serving.get(&format!("/assets/{name}"));
        assert_eq!(answer.status, 200, "{name}");
        assert_eq!(
            answer.header("Content-Type").as_deref(),
            Some(kind),
            "{name}"
        );
        assert_eq!(
            answer.header("Cache-Control").as_deref(),
            Some("public, max-age=31536000, immutable"),
            "{name}"
        );
        assert_eq!(
            answer.body,
            std::fs::read(ui.join("assets").join(name)).unwrap()
        );
    }
}

#[test]
fn a_path_out_of_the_folder_and_a_missing_asset_are_404() {
    let ui = built_ui();
    std::fs::write(ui.parent().unwrap().join("secret.txt"), "no").ok();
    let serving = serve(Some(&ui));
    for path in [
        "/assets/../Cargo.toml",
        "/assets/%2e%2e/x",
        "/assets/a\\b",
        "/assets/missing.js",
    ] {
        let answer = serving.get(path);
        assert_eq!(answer.status, 404, "{path}");
        assert!(answer.text().contains("Not found"), "{path}");
    }
}

#[test]
fn without_a_built_interface_the_api_still_serves_and_the_root_says_to_build() {
    let serving = serve(None);
    assert!(
        serving
            .shown
            .contains(&"interface: ui/dist not found, run npm run build in ui/".to_string()),
        "{:?}",
        serving.shown
    );
    let answer = serving.get("/");
    assert_eq!(answer.status, 404);
    assert_eq!(
        answer.text(),
        r#"{"error":"Interface not built: run npm run build in ui/"}"#
    );
    assert_eq!(serving.get("/api/v1/devices").status, 200);
}

#[test]
fn another_host_is_refused_on_the_interface_too() {
    let ui = built_ui();
    let serving = serve(Some(&ui));
    assert_eq!(serving.request("GET", "/", "evil.example").status, 403);
    assert_eq!(
        serving
            .request("POST", "/", &format!("127.0.0.1:{}", serving.port))
            .status,
        405
    );
}

#[test]
fn the_ui_option_names_the_folder_the_files_come_from() {
    let ui = built_ui();
    std::fs::write(ui.join("index.html"), "from the option").unwrap();
    let serving = serve(Some(&ui));
    assert_eq!(serving.get("/").text(), "from the option");
}

#[test]
fn every_static_answer_carries_the_security_headers() {
    let ui = built_ui();
    let serving = serve(Some(&ui));
    let host = format!("127.0.0.1:{}", serving.port);
    let mut answers = vec![];
    for path in [
        "/",
        "/assets/index-abc.js",
        "/assets/missing.js",
        "/assets/%2e%2e/x",
    ] {
        answers.push((path, serving.get(path)));
    }
    answers.push(("POST /", serving.request("POST", "/", &host)));
    assert_eq!(answers[3].1.status, 404);
    assert_eq!(answers[4].1.status, 405);
    for (path, answer) in answers {
        assert_eq!(
            answer.header("X-Frame-Options").as_deref(),
            Some("DENY"),
            "{path}"
        );
        assert_eq!(
            answer.header("Content-Security-Policy").as_deref(),
            Some("frame-ancestors 'none'"),
            "{path}"
        );
        assert_eq!(
            answer.header("X-Content-Type-Options").as_deref(),
            Some("nosniff"),
            "{path}"
        );
    }
}

#[test]
fn a_folder_without_an_index_gives_the_start_line_as_given_and_a_404_root() {
    let empty = temp_dir("empty");
    let serving = serve(Some(&empty));
    let line = format!(
        "interface: {} not found, run npm run build in ui/",
        empty.display()
    );
    assert!(serving.shown.contains(&line), "{:?}", serving.shown);
    assert_eq!(serving.get("/").status, 404);
}

#[test]
fn the_ui_option_without_serve_is_the_usage_error_and_exit_2() {
    let output = host()
        .args(["--sim", GARAGE, "--listen", "--ui", "somewhere"])
        .output()
        .expect("the host program starts");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}

#[test]
fn a_big_asset_is_sent_whole_with_its_size_as_length() {
    let ui = built_ui();
    let big: Vec<u8> = (0..1_048_576u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(ui.join("assets/big.bin"), &big).unwrap();
    let serving = serve(Some(&ui));
    let answer = serving.get("/assets/big.bin");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.header("Content-Length").as_deref(), Some("1048576"));
    assert_eq!(answer.body, big);
}

/// A symlink of the folder, even one that points inside it, is not served; neither is one that
/// points out of it.
#[cfg(unix)]
#[test]
fn a_symlink_in_the_interface_folder_is_404() {
    let ui = built_ui();
    let outside = temp_dir("outside");
    std::fs::write(outside.join("secret.txt"), "top secret").unwrap();
    std::os::unix::fs::symlink(outside.join("secret.txt"), ui.join("assets/leak.js")).unwrap();
    std::os::unix::fs::symlink(ui.join("assets/index-abc.js"), ui.join("assets/alias.js")).unwrap();
    std::fs::remove_file(ui.join("index.html")).unwrap();
    std::os::unix::fs::symlink(outside.join("secret.txt"), ui.join("index.html")).unwrap();
    let serving = serve(Some(&ui));
    for path in ["/assets/leak.js", "/assets/alias.js", "/"] {
        let answer = serving.get(path);
        assert_eq!(answer.status, 404, "{path}");
        assert!(!answer.text().contains("top secret"), "{path}");
    }
    assert_eq!(serving.get("/assets/index-abc.js").status, 200);
}
