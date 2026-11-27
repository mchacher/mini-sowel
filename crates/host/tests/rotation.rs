//! The real chain of a key rotation on the Mac (spec 040, R3). The host's update route installs the
//! bytes of a signed file's image into a slot file; the host does not run the image it stores and
//! its own binary is over the 4 MB slot, so the chain is checked with two builds of the host and a
//! small image. Key A is the box's key before a bridging release, key B the key after it. The host
//! is built twice (each in its own target folder, with `SOLITE_UPDATE_PUBLIC_KEY` set to a public
//! key made here by `--update-keygen`: no key is written in the repository), which is why the test
//! is slow and `#[ignore]`d (run by the full validate and the nightly CI).
//!
//! 1. host A accepts a file signed by A and refuses one signed by B (a release signed with the new
//!    key does not reach a box that skipped the bridge);
//! 2. host B accepts a file signed by B and refuses one signed by A (the old key no longer reaches
//!    a box that took the bridge);
//! 3. the bridge's own file, made by `scripts/release.sh` in bridge mode (a fake builder standing
//!    for the radio build: the old key signs, the image carries the new public key) is accepted by
//!    host A, which is what an old box receives, and refused by host B.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

const GARAGE: &str = "../../scenarios/garage.json";
const PASSWORD: &str = "test-password-1234";
const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/release.sh");
/// Newer than the running host (0.1.0), as every update must be.
const VERSION: &str = "0.2.0";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "solite-rotation-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A running `--serve` on the simulated garage, signed in.
struct Serving {
    child: Child,
    port: u16,
    lines: Receiver<String>,
    token: String,
}

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(binary: &Path, data: &Path) -> Serving {
    for _ in 0..5 {
        if let Some(serving) = try_serve(binary, data) {
            return serving;
        }
    }
    panic!("the host could not listen on a free port in five tries");
}

fn try_serve(binary: &Path, data: &Path) -> Option<Serving> {
    let port = free_port();
    let mut child = Command::new(binary)
        .args([
            "--sim",
            GARAGE,
            "--serve",
            &port.to_string(),
            "--speed",
            "10",
            "--data",
        ])
        .arg(data)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the host program starts");
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut serving = Serving {
        child,
        port,
        lines,
        token: String::new(),
    };
    let mut shown = Vec::new();
    loop {
        match serving.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => {
                let ready = line.starts_with("serving:");
                shown.push(line);
                if ready {
                    serving.sign_in();
                    return Some(serving);
                }
            }
            Err(error) => {
                let _ = serving.child.kill();
                let _ = serving.child.wait();
                let mut stderr = String::new();
                if let Some(mut pipe) = serving.child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                if error == mpsc::RecvTimeoutError::Disconnected
                    && stderr.starts_with("serve: cannot listen on ")
                {
                    return None;
                }
                panic!("the host stopped or stayed silent: {shown:?}, {stderr}");
            }
        }
    }
}

impl Serving {
    fn sign_in(&mut self) {
        let (_, body) = self.request("GET", "/api/v1/auth/status", b"", "application/json", false);
        let fresh = serde_json::from_str::<Value>(&body).unwrap()["setupRequired"] == true;
        let (route, payload) = if fresh {
            (
                "setup",
                json!({"username": "admin", "password": PASSWORD, "displayName": "Admin"}),
            )
        } else {
            ("login", json!({"username": "admin", "password": PASSWORD}))
        };
        let (status, body) = self.request(
            "POST",
            &format!("/api/v1/auth/{route}"),
            payload.to_string().as_bytes(),
            "application/json",
            false,
        );
        assert!(status == 200 || status == 201, "{body}");
        self.token = serde_json::from_str::<Value>(&body).unwrap()["accessToken"]
            .as_str()
            .unwrap()
            .to_string();
    }

    /// One request, the connection read to its end: the status and the body.
    fn request(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        content_type: &str,
        with_token: bool,
    ) -> (u16, String) {
        let auth = if with_token {
            format!("Authorization: Bearer {}\r\n", self.token)
        } else {
            String::new()
        };
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: {content_type}\r\n{auth}Content-Length: {}\r\n\r\n",
            self.port,
            body.len()
        );
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP answer");
        (
            head.split(' ').nth(1).unwrap().parse().unwrap(),
            body.to_string(),
        )
    }
}

fn plain_binary() -> &'static str {
    env!("CARGO_BIN_EXE_solite-host")
}

/// A host program built with a key's public half, and the key's file.
struct KeyedHost {
    binary: PathBuf,
    key_file: PathBuf,
    public: String,
}

/// Makes a key pair (`--update-keygen`, in a temporary folder) and builds the host a second time
/// with its public key, in the target folder `target_name` of this test run.
fn keyed_host(target_name: &str) -> KeyedHost {
    let dir = temp(target_name);
    let key_file = dir.join("signing.update-key");
    let output = Command::new(plain_binary())
        .arg("--update-keygen")
        .arg(&key_file)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let public = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("public key: "))
        .expect("the public key line")
        .to_string();
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join(target_name);
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = Command::new(cargo)
        .args(["build", "-p", "solite-host", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .env("SOLITE_UPDATE_PUBLIC_KEY", &public)
        .status()
        .unwrap();
    assert!(
        built.success(),
        "the host with the {target_name} did not build"
    );
    KeyedHost {
        binary: target.join("debug/solite-host"),
        key_file,
        public,
    }
}

/// A byte string that is not all the same, small: the host's slot holds what it is given.
fn image(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 13 + i / 97) % 256).unwrap())
        .collect()
}

/// Signs `image` as `version` with `signer`'s key, through `--update-sign`.
fn sign(signer: &KeyedHost, image_bytes: &[u8], version: &str) -> Vec<u8> {
    let dir = temp("sign");
    fs::write(dir.join("image.bin"), image_bytes).unwrap();
    let output = Command::new(plain_binary())
        .arg("--update-sign")
        .arg(dir.join("image.bin"))
        .args(["--version", version, "--key"])
        .arg(&signer.key_file)
        .arg("--out")
        .arg(dir.join("image.sup"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    fs::read(dir.join("image.sup")).unwrap()
}

/// Uploads `file` to a fresh host (its own data folder). The answer's status and body, and the
/// data folder.
fn upload_to(host: &KeyedHost, file: &[u8]) -> (u16, String, PathBuf) {
    let data = temp("data");
    let serving = serve(&host.binary, &data);
    let (status, body) = serving.request(
        "POST",
        "/api/v1/system/update",
        file,
        "application/octet-stream",
        true,
    );
    (status, body, data)
}

#[track_caller]
fn accepted(host: &KeyedHost, file: &[u8], image_bytes: &[u8]) {
    let (status, body, data) = upload_to(host, file);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["success"],
        true
    );
    // The slot holds exactly the file's image.
    assert_eq!(
        fs::read(data.join("update/slot-1.bin")).unwrap(),
        image_bytes
    );
}

#[track_caller]
fn refused_for_its_signature(host: &KeyedHost, file: &[u8]) {
    let (status, body, data) = upload_to(host, file);
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("Invalid signature"), "{body}");
    assert!(!data.join("update/boot.json").exists());
    assert!(!data.join("update/slot-1.bin").exists());
}

/// The bridge's own file: `scripts/release.sh` in bridge mode, the fake builder writing a small
/// image that carries the new public key, signed with the old key (`old`), the next key being
/// `new`. Returns the `.sup`.
fn bridge_sup(old: &KeyedHost, new: &KeyedHost) -> Vec<u8> {
    let root = temp("bridge");
    for sub in [
        "tree/firmware",
        "tree/ui",
        "tree/docs",
        "tree/release",
        "tmp",
    ] {
        fs::create_dir_all(root.join(sub)).unwrap();
    }
    fs::write(
        root.join("tree/Cargo.toml"),
        format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{VERSION}\"\n"),
    )
    .unwrap();
    fs::write(
        root.join("tree/firmware/Cargo.toml"),
        format!("[package]\nname = \"solite-firmware\"\nversion = \"{VERSION}\"\n"),
    )
    .unwrap();
    fs::write(
        root.join("tree/ui/package.json"),
        format!("{{\n  \"version\": \"{VERSION}\"\n}}\n"),
    )
    .unwrap();
    fs::write(
        root.join("tree/docs/release-notes.md"),
        format!("## {VERSION}\n\nA bridging release.\n"),
    )
    .unwrap();
    fs::write(
        root.join("tree/release/update-key.bridge"),
        format!("version {VERSION}\nprevious {}\n", old.public),
    )
    .unwrap();
    let public_file = root.join("update-key.pub");
    fs::write(&public_file, format!("{}\n", new.public)).unwrap();
    // The recipe key's public half (spec 042 R1.5): the script refuses to build without one; any
    // 64 hexadecimal characters do for this fake build.
    let recipe_file = root.join("recipe-key.pub");
    fs::write(&recipe_file, format!("{}\n", "ab".repeat(32))).unwrap();
    let builder = root.join("builder.sh");
    fs::write(
        &builder,
        "#!/bin/sh\nprintf 'bridge image for %s, key %s' \"$1\" \"$2\" >\"$3/app.bin\"\nprintf 'flash' >\"$3/flash.bin\"\n",
    )
    .unwrap();
    let out = root.join("out");
    let run = Command::new("sh")
        .arg(SCRIPT)
        .arg(VERSION)
        .arg("--key-file")
        .arg(&old.key_file)
        .arg("--next-key-file")
        .arg(&new.key_file)
        .arg("--public-key-file")
        .arg(&public_file)
        .arg("--recipe-key-file")
        .arg(&recipe_file)
        .arg("--out")
        .arg(&out)
        .env("SOLITE_HOST_BIN", plain_binary())
        .env("SOLITE_RELEASE_BUILDER", &builder)
        .env("SOLITE_RELEASE_TREE", root.join("tree"))
        .env("TMPDIR", root.join("tmp"))
        .env_remove("GITHUB_ACTIONS")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    fs::read(out.join(format!("solite-{VERSION}.sup"))).unwrap()
}

#[test]
#[ignore = "slow: builds the host twice"]
fn a_key_rotation_through_the_real_server_follows_the_bridge() {
    let a = keyed_host("rotation-host-a");
    let b = keyed_host("rotation-host-b");
    assert_ne!(a.public, b.public);
    let bytes = image(16 * 1024);

    // 1. The box before the bridge: key A in, key B out.
    accepted(&a, &sign(&a, &bytes, VERSION), &bytes);
    refused_for_its_signature(&a, &sign(&b, &bytes, VERSION));

    // 2. The box after the bridge: key B in, key A out.
    accepted(&b, &sign(&b, &bytes, VERSION), &bytes);
    refused_for_its_signature(&b, &sign(&a, &bytes, VERSION));

    // 3. The bridge's file (signed by A, its image carries B): an old box takes it, a box that
    //    trusts B does not.
    let bridge = bridge_sup(&a, &b);
    let image_of_bridge = bridge[128..].to_vec();
    assert!(
        String::from_utf8_lossy(&image_of_bridge).contains(&b.public),
        "the bridge's image carries the new public key"
    );
    accepted(&a, &bridge, &image_of_bridge);
    refused_for_its_signature(&b, &bridge);
}
