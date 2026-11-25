//! `scripts/box.sh` (spec 020) run with fake tools: `esptool.py`, `espefuse.py` and
//! `gen_esp32part.py` are shell scripts written into a temporary folder, put first on the `PATH`;
//! each logs its arguments and answers from environment variables. The prompt is answered through a
//! pseudo-terminal (the script reads `/dev/tty`). No test touches a real serial port or a real box,
//! and every run checks the fakes' log: no eFuse burn, ever.
//!
//! Also here: only `firmware/src/radio_uart.rs` names the radio's pins, GPIO12 and GPIO15 (its
//! bootloader and reset lines, spec 021 R2.3) and GPIO13 and GPIO17 (its UART).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/box.sh");
const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const FLASH_BYTES: usize = 16 * 1024 * 1024;
const MAC: &str = "7c:df:a1:3a:bc:de";
const MAC_NAME: &str = "7cdfa13abcde";
const MAC_TAIL: &str = "3abcde";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

const FAKE_ESPTOOL: &str = r#"#!/bin/sh
echo "esptool.py $*" >> "$FAKE_LOG"
echo "via:${FAKE_VIA:-path}" >> "$FAKE_LOG"
cmd=
for a in "$@"; do
  case "$a" in
    flash_id | read_flash | write_flash | erase_region | elf2image) cmd=$a; break ;;
  esac
done
last=
for a in "$@"; do last=$a; done
case "$cmd" in
  flash_id)
    if [ -n "$FAKE_NO_ANSWER" ]; then
      echo "A fatal error occurred: Failed to connect to ESP32" >&2
      exit 2
    fi
    echo "esptool.py v4.14.0"
    echo "Connecting....."
    echo "Chip is ESP32-D0WDR2-V3 (revision ${FAKE_REV:-v3.1})"
    echo "Features: WiFi, BT, Dual Core, 240MHz, Embedded PSRAM"
    echo "MAC: ${FAKE_MAC:-7c:df:a1:3a:bc:de}"
    echo "Manufacturer: 20"
    echo "Detected flash size: ${FAKE_FLASH:-16MB}"
    ;;
  read_flash)
    n=$(cat "$FAKE_STATE/reads" 2>/dev/null || echo 0)
    n=$((n + 1))
    echo "$n" > "$FAKE_STATE/reads"
    case "${FAKE_READ:-ok}" in
      ok) cp "$FAKE_IMAGE" "$last" ;;
      differ) if [ "$n" -eq 1 ]; then cp "$FAKE_IMAGE" "$last"; else cp "$FAKE_IMAGE2" "$last"; fi ;;
      short) head -c 1000 "$FAKE_IMAGE" > "$last" ;;
      uniform) head -c 16777216 /dev/zero > "$last" ;;
    esac
    # A permissive file, as a tool under a loose umask would leave it: the script's chmod must fix it.
    chmod 666 "$last"
    ;;
  write_flash)
    [ -n "$FAKE_WRITE_FAIL" ] && { echo "A fatal error occurred: write failed" >&2; exit 2; }
    echo "Hash of data verified."
    ;;
  erase_region) echo "Erase completed successfully" ;;
  elf2image)
    out=
    prev=
    for a in "$@"; do
      [ "$prev" = "-o" ] && out=$a
      prev=$a
    done
    head -c "${FAKE_APP_SIZE:-1000}" /dev/zero > "$out"
    ;;
esac
exit 0
"#;

const FAKE_ESPEFUSE: &str = r#"#!/bin/sh
echo "espefuse.py $*" >> "$FAKE_LOG"
echo "via:${FAKE_VIA:-path}" >> "$FAKE_LOG"
echo "ABS_DONE_0 (BLOCK0) Secure boot V1 is enabled for bootloader image = ${FAKE_ABS0:-False} R/W (0b0)"
echo "ABS_DONE_1 (BLOCK2) Secure boot V2 is enabled for bootloader image = ${FAKE_ABS1:-False} R/W (0b0)"
echo "FLASH_CRYPT_CNT (BLOCK0) Flash encryption mode counter = ${FAKE_CRYPT:-0} R/W (0b0000000)"
exit 0
"#;

const FAKE_GEN: &str = r##"#!/bin/sh
echo "gen_esp32part.py $*" >> "$FAKE_LOG"
echo "via:${FAKE_VIA:-path}" >> "$FAKE_LOG"
if [ $# -eq 2 ]; then
  echo TABLE > "$2"
  exit 0
fi
echo "# ESP-IDF Partition Table"
echo "# Name, Type, SubType, Offset, Size, Flags"
echo "nvs,data,nvs,0x9000,24K,"
echo "otadata,data,ota,0xf000,8K,"
echo "phy_init,data,phy,0x11000,4K,"
echo "ota_0,app,ota_0,0x20000,4M,"
echo "ota_1,app,ota_1,0x420000,4M,"
echo "storage,data,fat,0x820000,8064K,"
"##;

/// Stands in for ESP-IDF's Python: `python -m esptool ...` runs the fake tool, a script runs
/// through `sh`; both mark the log `via:embuild`.
const FAKE_PYTHON: &str = r#"#!/bin/sh
FAKE_VIA=embuild
export FAKE_VIA
if [ "$1" = "-m" ]; then
  m=$2
  shift 2
  exec "$FAKE_TOOLS/$m.py" "$@"
fi
exec sh "$@"
"#;

const FAKE_CAPTURE: &str = "#!/bin/sh\ncat \"$FAKE_SERIAL_LOG\"\n";

/// Runs the script in a pseudo-terminal (mode `pty`: the answer is typed when the prompt shows) or
/// with no controlling terminal and the answer piped on standard input (mode `notty`).
const HELPER: &str = r#"
import os, pty, sys
mode, answer = sys.argv[1], sys.argv[2]
cmd = sys.argv[3:]
if mode == "notty":
    r, w = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.setsid()
        os.dup2(r, 0)
        os.close(w)
        os.execvp(cmd[0], cmd)
    os.write(w, (answer + "\n").encode())
    os.close(w)
    _, status = os.waitpid(pid, 0)
    sys.exit(os.waitstatus_to_exitcode(status))
pid, fd = pty.fork()
if pid == 0:
    os.execvp(cmd[0], cmd)
buf = b""
sent = False
while True:
    try:
        data = os.read(fd, 4096)
    except OSError:
        break
    if not data:
        break
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()
    buf += data
    if not sent and b"to go on:" in buf:
        os.write(fd, (answer + "\n").encode())
        sent = True
_, status = os.waitpid(pid, 0)
sys.exit(os.waitstatus_to_exitcode(status))
"#;

struct Out {
    code: i32,
    text: String,
}

#[derive(Clone, Copy)]
enum Tty {
    /// The answer is typed on a terminal when the script asks.
    Pty,
    /// No terminal at all; the answer is piped on standard input.
    NoTty,
}

struct Env {
    root: PathBuf,
    extra: Vec<(String, String)>,
    path_has_tools: bool,
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_exec(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// 16 MB of bytes that are neither uniform nor repeating: a backup stand-in.
fn noise(seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    let mut bytes = Vec::with_capacity(FLASH_BYTES);
    while bytes.len() < FLASH_BYTES {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes.truncate(FLASH_BYTES);
    bytes
}

fn sha256(path: &Path) -> String {
    let output = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .or_else(|_| Command::new("sha256sum").arg(path).output())
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

impl Env {
    fn new() -> Env {
        let root = std::env::temp_dir().join(format!(
            "solite-box-script-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        for dir in [
            "tools",
            "ports",
            "state",
            "tmp",
            "out",
            "build",
            "radio-build",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        write_exec(&root.join("tools/esptool.py"), FAKE_ESPTOOL);
        write_exec(&root.join("tools/espefuse.py"), FAKE_ESPEFUSE);
        write_exec(&root.join("tools/gen_esp32part.py"), FAKE_GEN);
        write_exec(&root.join("capture.sh"), FAKE_CAPTURE);
        fs::write(root.join("ports/usbserial-1"), "").unwrap();
        fs::write(root.join("image.bin"), noise(1)).unwrap();
        fs::write(root.join("log.txt"), "").unwrap();
        // The fake build of the firmware: the bootloader, and the application.
        for build in ["build", "radio-build"] {
            let boot = root
                .join(build)
                .join("build/esp-idf-sys-abc/out/build/bootloader");
            fs::create_dir_all(&boot).unwrap();
            fs::write(boot.join("bootloader.bin"), vec![0u8; 100]).unwrap();
        }
        // The radio build's ESP-IDF settings: the UART interrupt in internal RAM.
        fs::write(
            root.join("radio-build/build/esp-idf-sys-abc/out/sdkconfig"),
            "CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000\nCONFIG_UART_ISR_IN_IRAM=y\n",
        )
        .unwrap();
        let env = Env {
            root,
            extra: Vec::new(),
            path_has_tools: true,
        };
        env.set_build(Build::Box);
        env.set_radio_build(Build::Radio);
        env
    }

    fn with(mut self, key: &str, value: &str) -> Env {
        self.extra.push((key.to_string(), value.to_string()));
        self
    }

    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    fn port(&self) -> String {
        self.path("ports/usbserial-1").display().to_string()
    }

    fn set_build(&self, build: Build) {
        fs::write(self.path("build/solite-firmware"), build.text()).unwrap();
    }

    /// The application in the radio image's own build folder (`target/radio`).
    fn set_radio_build(&self, build: Build) {
        fs::write(self.path("radio-build/solite-firmware"), build.text()).unwrap();
    }

    /// A backup file made for the fake box, with its `.sha256`; returns its path and its SHA-256.
    fn backup(&self, mac_name: &str) -> (PathBuf, String) {
        let file = self.path(&format!("out/dongle-max-{mac_name}-2026-11-22.bin"));
        fs::copy(self.path("image.bin"), &file).unwrap();
        let sha = sha256(&file);
        fs::write(
            file.with_extension("bin.sha256"),
            format!("{sha}  {}\n", file.file_name().unwrap().to_string_lossy()),
        )
        .unwrap();
        (file, sha)
    }

    /// The hardware sheet whose backup table holds one row.
    fn sheet(&self, sha: &str, restored: &str) -> PathBuf {
        let path = self.path("hardware.md");
        fs::write(
            &path,
            format!(
                "# Hardware\n\n| Date | Chip revision | Firmware version | SHA-256 of the backup | Restore proven |\n| ---- | ---- | ---- | ---- | ---- |\n| 2026-11-22 | v3.1 | 1.2.3 | {sha} | {restored} |\n"
            ),
        )
        .unwrap();
        path
    }

    /// The fake serial output the capture command prints.
    fn serial(&self, text: &str) {
        fs::write(self.path("serial.log"), text).unwrap();
    }

    fn run(&self, args: &[&str], tty: Tty, answer: &str) -> Out {
        self.run_in(&self.root, args, tty, answer)
    }

    /// Run from a folder of the environment, as a caller who gives relative paths.
    fn go_in(&self, dir: &str, args: &[&str]) -> Out {
        self.run_in(&self.path(dir), args, Tty::Pty, MAC_TAIL)
    }

    fn run_in(&self, dir: &Path, args: &[&str], tty: Tty, answer: &str) -> Out {
        let mut cmd = Command::new("/usr/bin/python3");
        cmd.arg("-I").arg("-c").arg(HELPER);
        cmd.arg(match tty {
            Tty::Pty => "pty",
            Tty::NoTty => "notty",
        });
        cmd.arg(answer);
        cmd.arg("/bin/sh").arg(SCRIPT).args(args);
        let tools = self.path("tools");
        let path = if self.path_has_tools {
            format!("{}:/usr/bin:/bin", tools.display())
        } else {
            "/usr/bin:/bin".to_string()
        };
        cmd.current_dir(dir)
            .env_clear()
            .env("PATH", path)
            .env("TMPDIR", self.path("tmp"))
            .env("FAKE_LOG", self.path("log.txt"))
            .env("FAKE_STATE", self.path("state"))
            .env("FAKE_IMAGE", self.path("image.bin"))
            .env("FAKE_IMAGE2", self.path("image2.bin"))
            .env("FAKE_TOOLS", &tools)
            .env("FAKE_SERIAL_LOG", self.path("serial.log"))
            .env("SOLITE_TARGET_DIR", self.path("build"))
            .env("SOLITE_RADIO_TARGET_DIR", self.path("radio-build"))
            .env(
                "SOLITE_HARDWARE_MD",
                self.path("hardware.md").display().to_string(),
            )
            .env(
                "SOLITE_PORT_PATTERNS",
                format!("{}/ports/usbserial*", self.root.display()),
            )
            .env("SOLITE_SERIAL_CAPTURE", self.path("capture.sh"))
            .env("SOLITE_REPLUG_SECONDS", "3")
            .env("SOLITE_ESPRESSIF", self.path("espressif"));
        for (k, v) in &self.extra {
            cmd.env(k, v);
        }
        let output = cmd.output().unwrap();
        let log = self.fake_log();
        assert!(!log.contains("burn"), "an eFuse burn was sent:\n{log}");
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Out {
            code: output.status.code().unwrap_or(-1),
            text,
        }
    }

    /// The common case: a terminal, the right answer unless said otherwise.
    fn go(&self, args: &[&str]) -> Out {
        self.run(args, Tty::Pty, MAC_TAIL)
    }

    fn fake_log(&self) -> String {
        fs::read_to_string(self.path("log.txt")).unwrap()
    }

    /// True when a write or a read of the flash reached a fake tool.
    fn wrote(&self) -> bool {
        let log = self.fake_log();
        log.contains("write_flash") || log.contains("erase_region")
    }

    fn read_flash_sent(&self) -> bool {
        self.fake_log().contains("read_flash")
    }
}

#[derive(Clone, Copy)]
enum Build {
    Box,
    Sim,
    Measuring,
    /// The radio image: its application holds the markers `radio: uart ` and `solite-ui `.
    Radio,
    /// A radio build without the embedded interface (spec 023 R7.3).
    RadioWithoutUi,
}

impl Build {
    fn text(self) -> &'static str {
        match self {
            Build::Box => "solite-firmware: storage\n",
            Build::Sim => "solite-firmware: scenario: started\n",
            Build::Measuring => "solite-firmware: scenario: started saves: written to flash\n",
            Build::Radio => "solite-firmware: radio: uart  baud solite-ui 4 files, 54 KB\n",
            Build::RadioWithoutUi => "solite-firmware: radio: uart  baud\n",
        }
    }
}

fn args_of<'a>(port: &'a str, rest: &[&'a str]) -> Vec<&'a str> {
    let mut v = vec![rest[0], "--port", port];
    v.extend_from_slice(&rest[1..]);
    v
}

// ---- identify ----------------------------------------------------------------------------

#[test]
fn identify_prints_five_lines_and_sends_only_reads() {
    let env = Env::new();
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 0, "{}", out.text);
    for line in [
        "chip: ESP32-D0WDR2-V3, revision v3.1",
        &format!("mac: {MAC}"),
        "flash: 16 MB",
        "secure boot: off",
        "flash encryption: off",
    ] {
        assert!(out.text.contains(line), "{line} missing in {}", out.text);
    }
    let log = env.fake_log();
    assert!(log.contains("flash_id") && log.contains("summary"));
    assert!(!env.wrote() && !env.read_flash_sent());
}

#[test]
fn identify_exits_2_with_no_port_or_with_two() {
    let env = Env::new();
    fs::remove_file(env.path("ports/usbserial-1")).unwrap();
    let out = env.go(&["identify"]);
    assert_eq!(out.code, 2, "{}", out.text);
    assert!(out.text.contains("no USB serial device"));

    fs::write(env.path("ports/usbserial-1"), "").unwrap();
    fs::write(env.path("ports/usbserial-2"), "").unwrap();
    let out = env.go(&["identify"]);
    assert_eq!(out.code, 2, "{}", out.text);
    assert!(out.text.contains("usbserial-1") && out.text.contains("usbserial-2"));
    assert!(env.fake_log().is_empty(), "no tool is run without a port");
}

#[test]
fn identify_exits_4_when_the_box_does_not_answer() {
    let env = Env::new().with("FAKE_NO_ANSWER", "1");
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 4, "{}", out.text);
    assert!(out.text.contains("hold the button while plugging"));
    assert!(out.text.contains("6638"));
}

#[test]
fn identify_stops_on_secure_boot() {
    let env = Env::new().with("FAKE_ABS0", "True");
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(out.text.contains("box: STOP, secure boot is on"));
    assert!(out.text.contains("nothing is written to this box"));
}

#[test]
fn identify_stops_on_flash_encryption() {
    let env = Env::new().with("FAKE_CRYPT", "1");
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(out.text.contains("box: STOP, flash encryption is on"));
}

#[test]
fn identify_stops_on_an_old_chip_or_a_small_flash() {
    let env = Env::new().with("FAKE_REV", "v1.0");
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(out.text.contains("revision v1.0"), "{}", out.text);

    let env = Env::new().with("FAKE_FLASH", "4MB");
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(out.text.contains("the flash is 4 MB, not 16 MB"));
}

#[test]
fn the_tools_come_from_the_path_else_from_esp_idfs_environment() {
    let env = Env::new();
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 0, "{}", out.text);
    let log = env.fake_log();
    assert!(log.contains("via:path") && !log.contains("via:embuild"));

    // None on the PATH, a fake ESP-IDF tree in its place.
    let mut env = Env::new();
    env.path_has_tools = false;
    let bin = env.path("espressif/python_env/idf5.5_py3.9_env/bin");
    fs::create_dir_all(&bin).unwrap();
    write_exec(&bin.join("python"), FAKE_PYTHON);
    fs::create_dir_all(env.path("espressif/esp-idf/v5.5.5/components/partition_table")).unwrap();
    let out = env.go(&args_of(&env.port(), &["identify"]));
    assert_eq!(out.code, 0, "{}", out.text);
    let log = env.fake_log();
    assert!(
        log.contains("via:embuild") && !log.contains("via:path"),
        "{log}"
    );
}

// ---- backup ------------------------------------------------------------------------------

fn out_dir(env: &Env) -> String {
    env.path("out").display().to_string()
}

#[test]
fn backup_writes_the_named_file_its_hash_and_prints_the_row() {
    for answer in ["3ABCDE", MAC_TAIL] {
        let env = Env::new();
        let out_arg = out_dir(&env);
        let out = env.run(
            &args_of(&env.port(), &["backup", "--out", &out_arg]),
            Tty::Pty,
            answer,
        );
        assert_eq!(out.code, 0, "{}", out.text);
        let date = String::from_utf8(Command::new("date").arg("+%F").output().unwrap().stdout)
            .unwrap()
            .trim()
            .to_string();
        let file = env.path(&format!("out/dongle-max-{MAC_NAME}-{date}.bin"));
        assert!(file.is_file(), "{}", out.text);
        let sha = sha256(&file);
        let sidecar = file.with_extension("bin.sha256");
        assert!(fs::read_to_string(&sidecar).unwrap().starts_with(&sha));
        for path in [&file, &sidecar] {
            let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{}", path.display());
        }
        assert!(
            !env.path(&format!("out/dongle-max-{MAC_NAME}-{date}.bin.read2"))
                .exists()
        );
        assert!(out.text.contains(&format!("| {date} | v3.1 | | {sha} | |")));
        assert!(!out.text.contains("box: backup NOT valid"));
        let reads = env.fake_log().matches("read_flash 0x0 0x1000000").count();
        assert_eq!(reads, 2);
    }
}

#[test]
fn backup_refuses_an_answer_piped_on_standard_input() {
    let env = Env::new();
    let out_arg = out_dir(&env);
    let out = env.run(
        &args_of(&env.port(), &["backup", "--out", &out_arg]),
        Tty::NoTty,
        MAC_TAIL,
    );
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("box: no terminal to confirm, nothing done")
    );
    assert!(!env.read_flash_sent());
}

#[test]
fn backup_stops_on_a_wrong_answer() {
    let env = Env::new();
    let out_arg = out_dir(&env);
    let out = env.run(
        &args_of(&env.port(), &["backup", "--out", &out_arg]),
        Tty::Pty,
        "ffffff",
    );
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("box: not confirmed, nothing done"));
    assert!(!env.read_flash_sent());
}

#[test]
fn backup_is_not_valid_when_the_second_read_differs() {
    let env = Env::new().with("FAKE_READ", "differ");
    fs::write(env.path("image2.bin"), noise(2)).unwrap();
    let out_arg = out_dir(&env);
    let out = env.go(&args_of(&env.port(), &["backup", "--out", &out_arg]));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("box: backup NOT valid, nothing to record")
    );
    let names: Vec<String> = fs::read_dir(env.path("out"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(names.iter().any(|n| n.ends_with(".bin")), "{names:?}");
    assert!(names.iter().any(|n| n.ends_with(".read2")), "{names:?}");
    assert!(!names.iter().any(|n| n.ends_with(".sha256")), "{names:?}");
    // Both files are private, though the tool left them readable by everyone.
    for entry in fs::read_dir(env.path("out")).unwrap() {
        let path = entry.unwrap().path();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{}", path.display());
    }
}

#[test]
fn backup_dry_run_sends_no_read_flash() {
    let env = Env::new();
    let out_arg = out_dir(&env);
    let out = env.go(&args_of(
        &env.port(),
        &["backup", "--out", &out_arg, "--dry-run"],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(!out.text.contains("to go on:"), "no prompt on a dry run");
    assert!(!env.read_flash_sent() && !env.wrote());
}

#[test]
fn restore_refuses_a_backup_with_no_sha256_file() {
    let env = Env::new();
    let (file, _) = env.backup(MAC_NAME);
    fs::remove_file(file.with_extension("bin.sha256")).unwrap();
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains(".sha256 is missing"), "{}", out.text);
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn relative_paths_are_the_callers_not_the_repositorys() {
    // restore <file>, from the folder that holds it.
    let env = Env::new();
    let (file, _) = env.backup(MAC_NAME);
    let name = file.file_name().unwrap().to_string_lossy().to_string();
    let out = env.go_in(
        "out",
        &args_of(&env.port(), &["restore", &name, "--dry-run"]),
    );
    assert_eq!(out.code, 0, "{}", out.text);

    // backup --out <relative folder>: the file lands there.
    let env = Env::new();
    let out = env.go_in(".", &args_of(&env.port(), &["backup", "--out", "out"]));
    assert_eq!(out.code, 0, "{}", out.text);
    let found = fs::read_dir(env.path("out"))
        .unwrap()
        .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".bin"));
    assert!(found, "{}", out.text);

    // log --from-file <relative>.
    let env = Env::new();
    fs::write(env.path("box.log"), box_log()).unwrap();
    let out = env.go_in(
        ".",
        &args_of(
            &env.port(),
            &["log", "--image", "box", "--from-file", "box.log"],
        ),
    );
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn backup_is_not_valid_when_the_read_is_short_or_uniform() {
    for mode in ["short", "uniform"] {
        let env = Env::new().with("FAKE_READ", mode);
        let out_arg = out_dir(&env);
        let out = env.go(&args_of(&env.port(), &["backup", "--out", &out_arg]));
        assert_eq!(out.code, 1, "{mode}: {}", out.text);
        assert!(out.text.contains("box: backup NOT valid"), "{mode}");
        assert!(!fs::read_dir(env.path("out")).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".sha256")
        }));
    }
}

#[test]
fn backup_refuses_a_missing_out_a_folder_in_the_repository_or_an_existing_file() {
    let env = Env::new();
    let out = env.go(&args_of(&env.port(), &["backup"]));
    assert_eq!(out.code, 2, "{}", out.text);

    let repo_folder = format!("{REPO}/docs");
    let out = env.go(&args_of(&env.port(), &["backup", "--out", &repo_folder]));
    assert_eq!(out.code, 2, "{}", out.text);
    assert!(out.text.contains("inside the repository"));

    let date = String::from_utf8(Command::new("date").arg("+%F").output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    fs::write(
        env.path(&format!("out/dongle-max-{MAC_NAME}-{date}.bin")),
        "x",
    )
    .unwrap();
    let out_arg = out_dir(&env);
    let out = env.go(&args_of(&env.port(), &["backup", "--out", &out_arg]));
    assert_eq!(out.code, 2, "{}", out.text);
    assert!(out.text.contains("already exists"));
    assert!(!env.read_flash_sent());
}

// ---- restore -----------------------------------------------------------------------------

#[test]
fn restore_writes_the_backup_with_the_keep_flags() {
    let env = Env::new();
    let (file, _) = env.backup(MAC_NAME);
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 0, "{}", out.text);
    let log = env.fake_log();
    assert!(
        log.contains(&format!(
            "write_flash --flash_mode keep --flash_freq keep --flash_size keep 0x0 {f}"
        )),
        "{log}"
    );
}

#[test]
fn restore_stops_on_an_empty_or_a_wrong_answer() {
    for answer in ["", "000000"] {
        let env = Env::new();
        let (file, _) = env.backup(MAC_NAME);
        let f = file.display().to_string();
        let out = env.run(&args_of(&env.port(), &["restore", &f]), Tty::Pty, answer);
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(out.text.contains("box: not confirmed, nothing done"));
        assert!(!env.wrote());
    }
}

#[test]
fn restore_dry_run_prints_the_plan_and_does_nothing() {
    let env = Env::new();
    let (file, _) = env.backup(MAC_NAME);
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f, "--dry-run"]));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("write ") && out.text.contains("at 0x0"));
    assert!(!out.text.contains("to go on:"), "no prompt on a dry run");
    assert!(!env.wrote());
}

#[test]
fn restore_refuses_an_altered_file_another_box_or_a_short_file() {
    // The SHA-256 differs from the .sha256.
    let env = Env::new();
    let (file, _) = env.backup(MAC_NAME);
    let mut bytes = fs::read(&file).unwrap();
    bytes[10] ^= 0xff;
    fs::write(&file, bytes).unwrap();
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("altered"));
    assert!(!out.text.contains("to go on:") && !env.wrote());

    // The MAC of the name is another box's.
    let env = Env::new();
    let (file, _) = env.backup("112233445566");
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("another box"));
    assert!(!out.text.contains("to go on:") && !env.wrote());

    // Not 16 MB.
    let env = Env::new();
    let (file, _) = env.backup(MAC_NAME);
    fs::write(&file, vec![1u8; 1000]).unwrap();
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn restore_prints_the_two_ways_out_when_the_write_fails() {
    let env = Env::new().with("FAKE_WRITE_FAIL", "1");
    let (file, _) = env.backup(MAC_NAME);
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("--baud 115200"));
    assert!(out.text.contains("Sonoff's web flasher"));
}

#[test]
fn restore_stops_on_an_unfit_box_before_the_prompt() {
    let env = Env::new().with("FAKE_ABS0", "True");
    let (file, _) = env.backup(MAC_NAME);
    let f = file.display().to_string();
    let out = env.go(&args_of(&env.port(), &["restore", &f]));
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

// ---- flash -------------------------------------------------------------------------------

/// A box with a backup, a sheet that lists it, and the image built.
fn ready(image: &str) -> (Env, String, String) {
    let env = Env::new();
    let (file, sha) = env.backup(MAC_NAME);
    env.sheet(&sha, "2026-11-22");
    let _ = image;
    let f = file.display().to_string();
    (env, f, sha)
}

#[test]
fn flash_erases_the_settings_and_writes_the_three_parts() {
    let (env, f, _) = ready("box");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    let log = env.fake_log();
    for region in [
        "erase_region 0x9000 0x6000",
        "erase_region 0xf000 0x2000",
        "erase_region 0x820000 0x7e0000",
    ] {
        assert!(log.contains(region), "{region} missing in\n{log}");
    }
    let write = log
        .lines()
        .find(|l| l.contains("write_flash"))
        .expect("write_flash sent");
    assert!(write.contains("0x1000 ") && write.contains("bootloader.bin"));
    assert!(write.contains("0x8000 ") && write.contains("partition-table.bin"));
    assert!(write.contains("0x20000 ") && write.contains("app.bin"));
    assert!(
        log.find("erase_region").unwrap() < log.find("write_flash").unwrap(),
        "the erase comes first"
    );
    assert!(log.contains("elf2image --flash_mode dio --flash_size 16MB"));
}

#[test]
fn flash_refuses_a_backup_that_is_not_proven() {
    // No --backup: allowed (the backup is optional), the plan says so.
    let (env, _, _) = ready("box");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--dry-run"],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("no backup given") && !env.wrote());

    // Its SHA-256 is in no row.
    let (env, f, _) = ready("box");
    env.sheet(&"0".repeat(64), "2026-11-22");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("in no row"));
    assert!(!out.text.contains("to go on:") && !env.wrote());

    // Its row has no restore date: empty, or anything that is not a date.
    for cell in ["", "no", "pending", "—", "2026-11"] {
        let (env, f, sha) = ready("box");
        env.sheet(&sha, cell);
        let out = env.go(&args_of(
            &env.port(),
            &["flash", "--image", "box", "--backup", &f],
        ));
        assert_eq!(out.code, 1, "cell {cell:?}: {}", out.text);
        assert!(out.text.contains("no restore date"), "cell {cell:?}");
        assert!(!out.text.contains("to go on:") && !env.wrote());
    }

    // Its MAC is another box's.
    let env = Env::new();
    let (file, sha) = env.backup("112233445566");
    env.sheet(&sha, "2026-11-22");
    let f = file.display().to_string();
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("another box"));
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn flash_dry_run_prints_the_regions_and_does_nothing() {
    let (env, f, _) = ready("box");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f, "--dry-run"],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    for text in [
        "erase nvs: 0x9000, 24576 bytes",
        "erase otadata: 0xf000, 8192 bytes",
        "erase storage: 0x820000, 8257536 bytes",
        "bootloader at 0x1000",
        "partition table at 0x8000",
        "application at 0x20000",
    ] {
        assert!(out.text.contains(text), "{text} missing in {}", out.text);
    }
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn flash_stops_on_an_unfit_box_before_the_prompt() {
    let (env, f, _) = ready("box");
    let env = env.with("FAKE_CRYPT", "3");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 3, "{}", out.text);
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn flash_refuses_the_wrong_image_and_names_the_build() {
    // `sim` with the image QEMU runs (no "written to flash").
    let (env, f, _) = ready("sim");
    env.set_build(Build::Sim);
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "sim", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("cargo build --release --features flash-saves")
    );
    assert!(!out.text.contains("to go on:") && !env.wrote());

    // `box` with a sim build.
    for build in [Build::Sim, Build::Measuring] {
        let (env, f, _) = ready("box");
        env.set_build(build);
        let out = env.go(&args_of(
            &env.port(),
            &["flash", "--image", "box", "--backup", &f],
        ));
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(out.text.contains("cargo build --release"));
        assert!(!out.text.contains("to go on:") && !env.wrote());
    }

    // The right pair goes through.
    let (env, f, _) = ready("sim");
    env.set_build(Build::Measuring);
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "sim", "--backup", &f],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn flash_refuses_a_missing_application_or_one_larger_than_the_slot() {
    let (env, f, _) = ready("box");
    fs::remove_file(env.path("build/solite-firmware")).unwrap();
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("not found"));
    assert!(!env.wrote());

    // 4 MB is the slot: one byte more is refused.
    let (env, f, _) = ready("box");
    let env = env.with("FAKE_APP_SIZE", &(4 * 1024 * 1024 + 1).to_string());
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("larger than the ota_0 slot"));
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn flash_asks_no_question_and_writes_at_once() {
    // Decision of the user, 2026-11-22: `flash` asks nothing more than the command itself.
    let (env, f, _) = ready("box");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(!out.text.contains("to go on:"), "{}", out.text);
    assert!(env.wrote());
}

// ---- log ---------------------------------------------------------------------------------

fn box_log() -> String {
    format!(
        "ets Jun  8 2016 00:22:57\nrst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\n\
         I (400) solite: solite {VERSION}\n\
         I (410) solite: memory: internal 245 KB free (largest block 110 KB), psram 2039 KB free of 2048 KB\n\
         I (420) solite: storage: formatted (does not mount)\n\
         I (421) solite: storage: /data, 12 KB used of 7936 KB\n\
         I (430) solite: network: wifi (setting auto), no link started\n\
         I (431) solite: radio: none\n"
    )
}

fn sim_log(peak: u32, internal_min: u32, reported: u32, saves: bool) -> String {
    let mut text = box_log().replace("radio: none\n", "seed: realistic home written to /data\n");
    text.push_str("I (500) solite: scenario: started\n");
    text.push_str(&format!(
        "I (900) solite: engine: 50 devices ({reported} reported), 10 zones, 50 equipments, 3 recipes; \
         heap 170 KB (peak {peak} KB); internal 221 KB free (min {internal_min} KB); \
         psram 1985 KB free (min 1916 KB)\n"
    ));
    if saves {
        text.push_str(
            "I (901) solite: saves: 49 written to flash, longest 410 ms (devices.json, 94 KB), total 8200 ms\n",
        );
    }
    text
}

fn log_from(env: &Env, text: &str, extra: &[&str]) -> Out {
    let file = env.path("captured.log");
    fs::write(&file, text).unwrap();
    let f = file.display().to_string();
    let mut args = vec!["log", "--from-file", &f];
    args.extend_from_slice(extra);
    env.go(&args)
}

#[test]
fn log_checks_the_box_image_from_a_file() {
    let env = Env::new();
    let out = log_from(&env, &box_log(), &["--image", "box"]);
    assert_eq!(out.code, 0, "{}", out.text);
    for line in [
        &*format!("solite {VERSION}"),
        "memory: internal 245 KB free",
        "storage: /data, 12 KB used",
        "network: wifi",
        "radio: none",
    ] {
        assert!(out.text.contains(line), "{line} missing in {}", out.text);
    }
    assert!(env.fake_log().is_empty(), "a file needs no tool");
}

#[test]
fn log_checks_the_measuring_image_and_prints_its_engine_and_saves_lines() {
    let env = Env::new();
    let out = log_from(&env, &sim_log(312, 107, 50, true), &["--image", "sim"]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("engine: 50 devices (50 reported)"));
    assert!(
        out.text
            .contains("saves: 49 written to flash, longest 410 ms")
    );
}

#[test]
fn log_fails_on_a_reset_after_the_banner_or_a_panic() {
    let env = Env::new();
    let looping = format!(
        "{}rst:0xc (SW_CPU_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\n",
        box_log()
    );
    let out = log_from(&env, &looping, &["--image", "box"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("last 40 lines"));
    assert!(out.text.contains("rst:0xc"));

    let panic = format!(
        "{}Guru Meditation Error: Core 0 panic'ed (LoadProhibited)\n",
        box_log()
    );
    let out = log_from(&env, &panic, &["--image", "box"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("Guru Meditation Error"));
}

#[test]
fn log_does_not_count_the_resets_before_the_banner() {
    let env = Env::new();
    let text = format!(
        "rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\nrst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\n{}",
        box_log()
    );
    let out = log_from(&env, &text, &["--image", "box"]);
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn log_expect_kept_fails_on_a_formatted_partition() {
    let env = Env::new();
    let out = log_from(&env, &box_log(), &["--image", "box", "--expect-kept"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("storage: formatted"));

    let kept = box_log().replace("storage: formatted (does not mount)\n", "");
    let out = log_from(&env, &kept, &["--image", "box", "--expect-kept"]);
    assert_eq!(out.code, 0, "{}", out.text);

    let out = log_from(&env, &box_log(), &["--image", "box"]);
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn log_raw_writes_the_capture_and_checks_nothing() {
    let env = Env::new();
    env.serial("this is not a solite boot\n");
    let out = env.go(&args_of(&env.port(), &["log", "--raw"]));
    assert_eq!(out.code, 0, "{}", out.text);
    let logs: Vec<PathBuf> = fs::read_dir(env.path("tmp/solite-box"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("serial-")
        })
        .collect();
    assert_eq!(logs.len(), 1, "{}", out.text);
    assert_eq!(
        fs::read_to_string(&logs[0]).unwrap(),
        "this is not a solite boot\n"
    );
}

#[test]
fn log_replug_waits_for_the_port_to_come_back() {
    let env = Env::new();
    env.serial(&box_log());
    let port = env.path("ports/usbserial-1");
    fs::remove_file(&port).unwrap();
    let late = port.clone();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        fs::write(late, "").unwrap();
    });
    let p = port.display().to_string();
    let out = env.go(&["log", "--image", "box", "--port", &p, "--replug"]);
    handle.join().unwrap();
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("radio: none"));

    // The port never comes back within the time allowed.
    let env = Env::new();
    env.serial(&box_log());
    let port = env.path("ports/usbserial-1");
    fs::remove_file(&port).unwrap();
    let p = port.display().to_string();
    let out = env.go(&["log", "--image", "box", "--port", &p, "--replug"]);
    assert_eq!(out.code, 4, "{}", out.text);
}

#[test]
fn log_fails_when_the_psram_is_not_found() {
    let env = Env::new();
    let text = box_log().replace("psram 2039 KB free of 2048 KB", "psram none");
    let out = log_from(&env, &text, &["--image", "box"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("PSRAM"));
}

#[test]
fn log_fails_on_a_limit_of_the_measuring_image_and_names_the_figure() {
    let env = Env::new();
    let out = log_from(&env, &sim_log(351, 107, 50, true), &["--image", "sim"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("351 KB"));

    let out = log_from(&env, &sim_log(312, 63, 50, true), &["--image", "sim"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("63 KB"));

    let out = log_from(&env, &sim_log(312, 107, 49, true), &["--image", "sim"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("only 49 of the 50 devices"));

    let out = log_from(&env, &sim_log(312, 107, 50, false), &["--image", "sim"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("saves:"));
}

#[test]
fn log_fails_on_a_missing_line_and_names_it() {
    let env = Env::new();
    let text = box_log().replace(
        "I (430) solite: network: wifi (setting auto), no link started\n",
        "",
    );
    let out = log_from(&env, &text, &["--image", "box"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("network line"));
}

// ---- the radio image ---------------------------------------------------------------------

const RADIO_BUILD_TEXT: &str = "--features radio";

#[test]
fn flash_radio_reads_the_radio_build_and_leaves_the_box_unreset() {
    let (env, f, _) = ready("radio");
    // The box's own folder holds a box image: the radio image is read from its own folder.
    env.set_build(Build::Box);
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "radio", "--backup", &f],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("log --image radio"));
    let log = env.fake_log();
    let write = log
        .lines()
        .find(|l| l.contains("write_flash"))
        .expect("write_flash sent");
    assert!(write.contains("--after no_reset"), "{write}");
    assert!(write.contains("app.bin"));
    // The erases leave the box in download mode too: no boot of an older image in between.
    for erase in log.lines().filter(|l| l.contains("erase_region")) {
        assert!(erase.contains("--after no_reset"), "{erase}");
    }
    // The box and sim images still reset the box.
    let (env, f, _) = ready("box");
    env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f],
    ));
    assert!(!env.fake_log().contains("no_reset"));
}

#[test]
fn flash_radio_refuses_a_build_without_the_radio_marker_and_names_the_command() {
    for build in [Build::Sim, Build::Box, Build::Measuring] {
        let (env, f, _) = ready("radio");
        env.set_radio_build(build);
        let out = env.go(&args_of(
            &env.port(),
            &["flash", "--image", "radio", "--backup", &f],
        ));
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(out.text.contains(RADIO_BUILD_TEXT), "{}", out.text);
        assert!(out.text.contains("target/radio"), "{}", out.text);
        assert!(!out.text.contains("to go on:") && !env.wrote());
    }
}

#[test]
fn flash_radio_refuses_a_build_made_without_the_uart_interrupt_setting() {
    for sdkconfig in [
        "CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000\n# CONFIG_UART_ISR_IN_IRAM is not set\n",
        "CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000\n",
    ] {
        let (env, f, _) = ready("radio");
        fs::write(
            env.path("radio-build/build/esp-idf-sys-abc/out/sdkconfig"),
            sdkconfig,
        )
        .unwrap();
        let out = env.go(&args_of(
            &env.port(),
            &["flash", "--image", "radio", "--backup", &f],
        ));
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(out.text.contains("UART_ISR_IN_IRAM"), "{}", out.text);
        assert!(
            out.text.contains("sdkconfig.radio.defaults"),
            "{}",
            out.text
        );
        assert!(!out.text.contains("to go on:") && !env.wrote());
    }
    // No sdkconfig at all is refused too.
    let (env, f, _) = ready("radio");
    fs::remove_file(env.path("radio-build/build/esp-idf-sys-abc/out/sdkconfig")).unwrap();
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "radio", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

#[test]
fn flash_radio_says_before_the_prompt_that_it_erases_storage_and_the_dry_run_too() {
    let (env, f, _) = ready("radio");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "radio", "--backup", &f, "--dry-run"],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("zigbee.json"), "{}", out.text);
    assert!(out.text.contains("destroys"), "{}", out.text);
    assert!(out.text.contains("paired devices"), "{}", out.text);
    assert!(!env.wrote());
    // With the prompt: the warning comes before it.
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "radio", "--backup", &f],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("destroys"), "{}", out.text);
    // The box image does not carry that warning.
    let (env, f, _) = ready("box");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "box", "--backup", &f, "--dry-run"],
    ));
    assert!(!out.text.contains("zigbee.json"), "{}", out.text);
}

#[test]
fn flash_box_and_sim_refuse_a_radio_build_before_the_prompt() {
    for image in ["box", "sim"] {
        let (env, f, _) = ready(image);
        env.set_build(Build::Radio);
        let out = env.go(&args_of(
            &env.port(),
            &["flash", "--image", image, "--backup", &f],
        ));
        assert_eq!(out.code, 1, "{}", out.text);
        assert!(out.text.contains("radio image"), "{}", out.text);
        assert!(!out.text.contains("to go on:") && !env.wrote());
    }
}

#[test]
fn an_image_other_than_box_sim_or_radio_is_a_usage_error_naming_the_three() {
    let (env, f, _) = ready("box");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "wifi", "--backup", &f],
    ));
    assert_eq!(out.code, 2, "{}", out.text);
    for name in ["box", "sim", "radio"] {
        assert!(out.text.contains(name), "{name} missing in {}", out.text);
    }
    assert!(!env.wrote());
}

/// A radio image's log with Wi-Fi (spec 023): its `network:` line has no `no link started`.
fn radio_log() -> String {
    box_log().replace(
        "I (430) solite: network: wifi (setting auto), no link started\n\
         I (431) solite: radio: none\n",
        "I (430) solite: network: wifi (setting auto)\n\
         I (431) solite: solite-ui 4 files, 54 KB\n\
         I (432) solite: console: ready\n\
         I (433) solite: wifi: settings read, password: 12 characters\n\
         I (434) solite: wifi: connecting\n\
         I (440) solite: radio: uart 115200 baud\n\
         I (460) solite: ezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0\n\
         I (461) solite: radio address: 0x8C6FB9FFFE123456\n\
         I (900) solite: network: resumed (made by another program), channel 15, PAN 0x1A2B, extended PAN 0xDDDDDDDDDDDDDDDD\n\
         I (901) solite: pairing: open for 180 s\n\
         I (3200) solite: wifi: connected, address 192.168.1.20, name solite-3abcde, channel 6, rssi -61 dBm\n\
         I (3300) solite: serving: http://192.168.1.20/\n\
         I (3900) solite: clock: synced, 2026-11-22T10:00:00.000Z\n\
         I (13300) solite: memory-network: internal 150 KB free (largest block 80 KB), psram 1800 KB free of 2048 KB\n\
         I (240500) solite: serial: 115200 baud, rx buffer 4096 B, highest 212 B, full 0 times\n\
         I (240501) solite: heap: internal 120 KB free (min 95 KB), largest block 60 KB, psram 1900 KB free (min 1850 KB), largest block 1800 KB, engine stack 20 KB unused of 32 KB\n",
    )
}

#[test]
fn log_checks_a_good_radio_log_and_prints_its_radio_lines() {
    let env = Env::new();
    let out = log_from(&env, &radio_log(), &["--image", "radio"]);
    assert_eq!(out.code, 0, "{}", out.text);
    for line in [
        "radio: uart 115200 baud",
        "ezsp: protocol 13",
        "radio address: 0x8C6FB9FFFE123456",
        "network: resumed",
        "pairing: open for 180 s",
        "serial: 115200 baud, rx buffer 4096 B",
        "heap: internal 120 KB free (min 95 KB), largest block 60 KB, psram 1900 KB free (min 1850 KB), largest block 1800 KB",
    ] {
        assert!(out.text.contains(line), "{line} missing in {}", out.text);
    }
}

#[test]
fn log_radio_fails_when_the_radio_answers_at_no_speed() {
    let env = Env::new();
    let text = radio_log().replace(
        "radio: uart 115200 baud",
        "radio: no answer at any baud rate (115200, 460800, 921600)",
    );
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("no speed"), "{}", out.text);
    assert!(out.text.contains("last 40 lines"), "{}", out.text);
}

#[test]
fn log_radio_fails_when_the_radio_was_restarted_too_often() {
    let env = Env::new();
    let mut text = radio_log();
    text.push_str("W (250000) solite: radio: stopped after 3 restarts in 10 min: link lost\n");
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("restarted too often"), "{}", out.text);
}

#[test]
fn log_radio_fails_on_a_task_watchdog() {
    let env = Env::new();
    let mut text = radio_log();
    text.push_str("E (250000) task_wdt: Task watchdog got triggered.\n");
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("task_wdt"), "{}", out.text);
}

#[test]
fn log_radio_fails_and_names_a_refused_pairing_window() {
    let env = Env::new();
    let mut text = radio_log();
    text.push_str(
        "I (902) solite: pairing: failed: the radio refused permitJoining (status 0x70)\n",
    );
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("refused to open joining"), "{}", out.text);
    assert!(out.text.contains("permitJoining"), "{}", out.text);
}

#[test]
fn log_radio_is_green_when_the_window_is_not_opened_at_boot_and_prints_the_reason() {
    let env = Env::new();
    let text = radio_log().replace(
        "pairing: open for 180 s",
        "pairing: not opened at boot (reset: crash)",
    );
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text
            .contains("pairing: not opened at boot (reset: crash)")
    );
}

#[test]
fn log_radio_counts_a_second_banner_as_a_reboot_even_without_a_rst_line() {
    let env = Env::new();
    let mut text = radio_log();
    text.push_str(&format!("I (250000) solite: solite {VERSION}\n"));
    assert!(!text.contains("rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)\nI (250"));
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("rebooted"), "{}", out.text);
}

#[test]
fn log_radio_fails_under_the_internal_ram_reserve_and_names_it() {
    let env = Env::new();
    let low = radio_log().replace("(min 95 KB)", "(min 63 KB)");
    let out = log_from(&env, &low, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("64 KB"), "{}", out.text);
    // 64 KB is the reserve: it passes.
    let edge = radio_log().replace("(min 95 KB)", "(min 64 KB)");
    let out = log_from(&env, &edge, &["--image", "radio"]);
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn log_radio_without_a_serial_line_names_it_and_the_seconds_option() {
    let env = Env::new();
    let text: String = radio_log()
        .lines()
        .filter(|l| !l.contains("serial: 115200 baud"))
        .map(|l| format!("{l}\n"))
        .collect();
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("serial line"), "{}", out.text);
    assert!(out.text.contains("--seconds"), "{}", out.text);
}

#[test]
fn log_radio_fails_on_a_panic_after_the_banner() {
    let env = Env::new();
    let mut text = radio_log();
    text.push_str("Guru Meditation Error: Core 1 panic'ed (LoadProhibited)\n");
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("panicked"), "{}", out.text);
}

#[test]
fn log_radio_from_the_port_shows_the_output_and_checks_it() {
    let env = Env::new();
    env.serial(&radio_log());
    let out = env.go(&args_of(&env.port(), &["log", "--image", "radio"]));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("capturing 300 s"), "{}", out.text);
    assert!(out.text.contains("pairing: open for 180 s"), "{}", out.text);
}

#[test]
fn log_box_and_sim_keep_their_checks_and_their_default_times() {
    let env = Env::new();
    env.serial(&box_log());
    let out = env.go(&args_of(&env.port(), &["log", "--image", "box"]));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("capturing 60 s"), "{}", out.text);
    // A radio log is not a box log: `radio: none` is missing.
    let out = log_from(&env, &radio_log(), &["--image", "box"]);
    assert_eq!(out.code, 1, "{}", out.text);
}

// ---- the radio's pins --------------------------------------------------------------------

/// The radio's pins: its bootloader and reset lines, and its UART's two lines.
const RADIO_PINS: [&str; 4] = ["12", "13", "15", "17"];

/// True when the text names one of the radio's pins in any spelling (`GPIO12`, `Gpio12`, `gpio12`,
/// `GPIO_NUM_12`).
fn names_the_radios_pins(text: &str) -> bool {
    let lower = text.to_lowercase();
    RADIO_PINS
        .iter()
        .any(|n| lower.contains(&format!("gpio{n}")) || lower.contains(&format!("gpio_num_{n}")))
}

#[test]
fn the_pin_check_finds_the_radio_pins() {
    assert!(names_the_radios_pins("let p = pins.gpio12;"));
    assert!(names_the_radios_pins("Gpio15"));
    assert!(names_the_radios_pins("// GPIO12 is a strapping pin"));
    assert!(names_the_radios_pins("GPIO_NUM_12"));
    assert!(names_the_radios_pins("gpio_num_15"));
    assert!(names_the_radios_pins("GPIO13 and GPIO17 are the UART"));
    assert!(!names_the_radios_pins("let p = pins.gpio14;"));
}

/// Every `.rs` file under `dir`, at any depth.
fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

/// True when the text passes a radio pin as a bare number to a GPIO call: `gpio_set_level(15, 0)`,
/// `gpio_config` masks like `1u64 << 12`, `PinDriver::output(.. 12 ..)`, `.output(13)`.
fn passes_a_bare_radio_pin(text: &str) -> bool {
    text.lines().any(|line| {
        let lower = line.to_lowercase();
        let gpio_call = ["gpio_", "pindriver", ".output(", ".input(", "uart_set_pin"]
            .iter()
            .any(|call| lower.contains(call));
        let shift = RADIO_PINS
            .iter()
            .any(|n| lower.replace(' ', "").contains(&format!("<<{n}")));
        let number = lower
            .split(|c: char| !c.is_ascii_digit())
            .any(|word| RADIO_PINS.contains(&word));
        (gpio_call && number) || shift
    })
}

#[test]
fn the_pin_check_finds_a_bare_numeric_pin_in_a_gpio_call() {
    assert!(passes_a_bare_radio_pin("gpio_set_level(15, 0);"));
    assert!(passes_a_bare_radio_pin(
        "let p = PinDriver::output(pins.x, 12)?;"
    ));
    assert!(passes_a_bare_radio_pin("uart_set_pin(1, 17, 13, -1, -1)"));
    assert!(passes_a_bare_radio_pin("pin_bit_mask: 1u64 << 12,"));
    assert!(!passes_a_bare_radio_pin("gpio_set_level(14, 0);"));
    assert!(!passes_a_bare_radio_pin("let stack = 32 * 1024; // 15 min"));
}

/// The files of the tree that name a radio pin and are not `radio_uart.rs`.
fn pin_offenders(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(dir, &mut files);
    files
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|n| n != "radio_uart.rs"))
        .filter(|path| {
            let text = fs::read_to_string(path).unwrap();
            names_the_radios_pins(&text) || passes_a_bare_radio_pin(&text)
        })
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_pin_check_reports_a_file_other_than_radio_uart_naming_a_pin() {
    let dir = std::env::temp_dir().join(format!("solite-pins-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(
        dir.join("radio_uart.rs"),
        "const PIN_RESET: i32 = 15; // GPIO15\n",
    )
    .unwrap();
    fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    assert!(
        pin_offenders(&dir).is_empty(),
        "radio_uart.rs may name them"
    );
    fs::write(dir.join("nested/other.rs"), "// drives GPIO15 too\n").unwrap();
    assert_eq!(pin_offenders(&dir), ["other.rs"]);
    fs::remove_file(dir.join("nested/other.rs")).unwrap();
    fs::write(
        dir.join("nested/bare.rs"),
        "unsafe { gpio_set_level(12, 0) };\n",
    )
    .unwrap();
    assert_eq!(pin_offenders(&dir), ["bare.rs"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn only_radio_uart_names_the_radios_pins_in_the_firmware() {
    let dir = Path::new(REPO).join("firmware/src");
    let uart = fs::read_to_string(dir.join("radio_uart.rs")).unwrap();
    let lower = uart.to_lowercase();
    for pin in RADIO_PINS {
        assert!(
            lower.contains(&format!("gpio{pin}")),
            "radio_uart.rs does not name GPIO{pin}"
        );
    }
    let offenders = pin_offenders(&dir);
    assert!(
        offenders.is_empty(),
        "{offenders:?} name the radio's pins: only radio_uart.rs may"
    );
}

// ---- flash --keep-storage and the ownership line (spec 022 R8) ----------------------------

#[test]
fn keep_storage_plans_to_erase_nvs_and_otadata_only_and_says_so() {
    let (env, f, _) = ready("radio");
    let out = env.go(&args_of(
        &env.port(),
        &[
            "flash",
            "--image",
            "radio",
            "--backup",
            &f,
            "--keep-storage",
            "--dry-run",
        ],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("erase nvs: 0x9000"), "{}", out.text);
    assert!(out.text.contains("erase otadata: 0xf000"), "{}", out.text);
    assert!(!out.text.contains("erase storage"), "{}", out.text);
    assert!(
        out.text
            .contains("keep storage (the home, the network's backup and the Wi-Fi settings)"),
        "{}",
        out.text
    );
    assert!(
        !out.text.contains("zigbee.json"),
        "no warning: {}",
        out.text
    );
    assert!(
        out.text
            .contains("the box drives this radio's network itself"),
        "{}",
        out.text
    );
    assert!(!env.wrote());
}

#[test]
fn keep_storage_confirmed_sends_two_erases_never_storages_and_then_the_write() {
    let (env, f, _) = ready("radio");
    let out = env.go(&args_of(
        &env.port(),
        &[
            "flash",
            "--image",
            "radio",
            "--backup",
            &f,
            "--keep-storage",
        ],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    let log = env.fake_log();
    let erases: Vec<&str> = log.lines().filter(|l| l.contains("erase_region")).collect();
    assert_eq!(erases.len(), 2, "{log}");
    assert!(
        erases[0].contains("0x9000") && erases[1].contains("0xf000"),
        "{erases:?}"
    );
    assert!(
        !log.contains("0x820000"),
        "storage's offset is never sent:\n{log}"
    );
    let last_erase = log.rfind("erase_region").unwrap();
    assert!(
        log.rfind("write_flash").unwrap() > last_erase,
        "the write comes last"
    );
}

#[test]
fn the_radio_flash_without_keep_storage_is_as_before_and_points_to_the_option() {
    let (env, f, _) = ready("radio");
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "radio", "--backup", &f, "--dry-run"],
    ));
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(out.text.contains("erase storage: 0x820000"), "{}", out.text);
    let warning = out
        .text
        .lines()
        .find(|l| l.contains("WARNING"))
        .expect("the warning");
    assert!(
        warning
            .trim_end()
            .ends_with("(use --keep-storage to keep them)"),
        "{warning}"
    );
    assert!(
        out.text
            .contains("the box drives this radio's network itself"),
        "{}",
        out.text
    );
}

#[test]
fn keep_storage_is_refused_with_the_box_and_sim_images() {
    for image in ["sim", "box"] {
        let (env, f, _) = ready(image);
        let out = env.go(&args_of(
            &env.port(),
            &["flash", "--image", image, "--backup", &f, "--keep-storage"],
        ));
        assert_eq!(out.code, 2, "{image}: {}", out.text);
        assert!(
            out.text
                .contains("--keep-storage is for the radio image: the sim image seeds /data"),
            "{}",
            out.text
        );
        assert!(!env.wrote());
    }
}

// ---- wifi (spec 022 R9) ---------------------------------------------------------------------

/// The fake console: logs its arguments and environment, keeps the line it reads on its standard
/// input, then prints the answers of `$FAKE_CONSOLE_ANSWER` (and stays alive `$FAKE_CONSOLE_STAY`
/// seconds, as a box that goes on running).
const FAKE_CONSOLE: &str = r#"#!/bin/sh
echo "$*" >> "$FAKE_CONSOLE_ARGS"
env >> "$FAKE_CONSOLE_ENV"
read -r line
printf '%s\n' "$line" >> "$FAKE_CONSOLE_LINES"
[ -n "$FAKE_CONSOLE_SILENT" ] && { sleep 8; exit 0; }
sleep "${FAKE_CONSOLE_DELAY:-0}"
cat "$FAKE_CONSOLE_ANSWER"
sleep "${FAKE_CONSOLE_STAY:-0}"
"#;

/// Types the name and the password on a pseudo-terminal when the prompts show, and reports in a
/// file: whether the terminal's echo was off at the password prompt, and on again afterwards.
/// `ctrlc` sends Ctrl-C at the password prompt instead of the password.
const WIFI_HELPER: &str = r#"
import os, pty, sys, termios, select, time
name, password, mode, report = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
cmd = sys.argv[5:]
pid, fd = pty.fork()
if pid == 0:
    os.execvp(cmd[0], cmd)
buf = b""
stage = 0
echo_off = None
status = None
ctrlc_sent = 0
while True:
    ready, _, _ = select.select([fd], [], [], 0.5 if stage == 2 and mode == "ctrlc" else 30)
    if not ready:
        if stage == 2 and mode == "ctrlc" and ctrlc_sent < 40:
            done, st = os.waitpid(pid, os.WNOHANG)
            if done:
                status = st
                break
            # A Ctrl-C that lands before the script's `read` starts is held by the shell until
            # `read` ends (on a loaded machine the first one may): press it again.
            os.write(fd, b"\x03")
            ctrlc_sent += 1
            continue
        break
    try:
        data = os.read(fd, 4096)
    except OSError:
        break
    if not data:
        break
    sys.stdout.buffer.write(data)
    sys.stdout.buffer.flush()
    buf += data
    if stage == 0 and b"network name:" in buf:
        os.write(fd, (name + "\n").encode())
        stage = 1
    elif stage == 1 and b"not shown): " in buf:
        echo_off = not (termios.tcgetattr(fd)[3] & termios.ECHO)
        if mode == "ctrlc":
            # A user presses Ctrl-C well after the prompt shows: a signal that lands before the
            # script's `read` has started would only be handled after it.
            time.sleep(0.5)
            os.write(fd, b"\x03")
        else:
            os.write(fd, (password + "\n").encode())
        stage = 2
# A bounded wait: a script that never ends fails the test instead of holding it forever.
deadline = time.time() + 20
while status is None:
    done, st = os.waitpid(pid, os.WNOHANG)
    if done:
        status = st
    elif time.time() > deadline:
        os.kill(pid, 9)
        _, status = os.waitpid(pid, 0)
    else:
        time.sleep(0.1)
try:
    after = "on" if termios.tcgetattr(fd)[3] & termios.ECHO else "off"
except Exception:
    after = "unknown"
with open(report, "w") as f:
    f.write("echo_off_at_prompt=%s\necho_after=%s\n" % (echo_off, after))
sys.exit(os.waitstatus_to_exitcode(status))
"#;

const SSID: &str = "Zz-Distinct-Net";
/// A password with a quote and a backslash, to check the JSON escaping.
const PASSWORD: &str = r#"Pw"9\distinct-pass"#;

struct WifiRun {
    code: i32,
    text: String,
    report: String,
}

impl Env {
    /// Runs `box.sh wifi` with the fake console; `answer` is what the console prints.
    fn wifi(
        &self,
        extra_args: &[&str],
        answer: &str,
        mode: &str,
        extra_env: &[(&str, &str)],
    ) -> WifiRun {
        self.console_command("wifi", extra_args, answer, mode, extra_env)
    }

    /// Runs a `box.sh` sub-command that talks to the console (`wifi`, `pair`) with the fake one.
    fn console_command(
        &self,
        command: &str,
        extra_args: &[&str],
        answer: &str,
        mode: &str,
        extra_env: &[(&str, &str)],
    ) -> WifiRun {
        write_exec(&self.path("console.sh"), FAKE_CONSOLE);
        fs::write(self.path("console-answer.txt"), answer).unwrap();
        let report = self.path("report.txt");
        let _ = fs::remove_file(&report);
        let mut cmd = Command::new("/usr/bin/python3");
        cmd.arg("-I").arg("-c").arg(WIFI_HELPER);
        cmd.arg(SSID).arg(PASSWORD).arg(mode).arg(&report);
        cmd.arg("/bin/sh").arg(SCRIPT).arg(command).args(extra_args);
        cmd.current_dir(&self.root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", self.path("tmp"))
            .env(
                "SOLITE_PORT_PATTERNS",
                format!("{}/ports/usbserial*", self.root.display()),
            )
            .env("SOLITE_SERIAL_CONSOLE", self.path("console.sh"))
            .env("SOLITE_WIFI_WAIT_SECONDS", "3")
            .env("FAKE_CONSOLE_ARGS", self.path("console-args.txt"))
            .env("FAKE_CONSOLE_ENV", self.path("console-env.txt"))
            .env("FAKE_CONSOLE_LINES", self.path("console-lines.txt"))
            .env("FAKE_CONSOLE_ANSWER", self.path("console-answer.txt"));
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let output = cmd.output().unwrap();
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        WifiRun {
            code: output.status.code().unwrap_or(-1),
            text,
            report: fs::read_to_string(&report).unwrap_or_default(),
        }
    }

    /// The lines the fake console received on its standard input.
    fn console_lines(&self) -> Vec<String> {
        fs::read_to_string(self.path("console-lines.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

const CONNECTED: &str = "wifi: settings saved\nwifi: connected, address 192.168.1.20, name solite-a1b2c3, channel 6, rssi -61 dBm\n";

/// The text after the password prompt: what the script printed once the password was typed.
fn after_password(text: &str) -> &str {
    text.split("not shown): ").nth(1).unwrap_or("")
}

#[test]
fn wifi_sends_one_json_line_and_prints_the_connection() {
    let env = Env::new();
    let run = env.wifi(&[], CONNECTED, "typed", &[]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text.contains("box: Wi-Fi connected, address 192.168.1.20, name solite-a1b2c3; open http://192.168.1.20/"),
        "{}",
        run.text
    );
    let lines = env.console_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let json = lines[0].strip_prefix("wifi-set ").expect("a wifi-set line");
    let value: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
    assert_eq!(value["ssid"], SSID);
    assert_eq!(value["password"], PASSWORD, "quote and backslash escaped");
}

#[test]
fn wifi_never_shows_logs_or_keeps_the_password_or_the_name() {
    let env = Env::new();
    let run = env.wifi(&[], CONNECTED, "typed", &[]);
    assert_eq!(run.code, 0, "{}", run.text);
    // The terminal echoes the name as it is typed; after the password prompt nothing shows either.
    assert!(!run.text.contains(PASSWORD), "{}", run.text);
    assert!(!after_password(&run.text).contains(SSID), "{}", run.text);
    // Not in the fake's arguments or environment, nor in any file of the work folder.
    for file in ["console-args.txt", "console-env.txt"] {
        let text = fs::read_to_string(env.path(file)).unwrap();
        assert!(!text.contains(PASSWORD) && !text.contains(SSID), "{file}");
    }
    fn scan(dir: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, found);
            } else if let Ok(text) = fs::read_to_string(&path)
                && (text.contains(PASSWORD) || text.contains(SSID))
            {
                found.push(path.display().to_string());
            }
        }
    }
    let mut found = Vec::new();
    scan(&env.path("tmp"), &mut found);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn wifi_does_not_echo_the_password_and_restores_the_echo() {
    let env = Env::new();
    let run = env.wifi(&[], CONNECTED, "typed", &[]);
    assert!(
        run.report.contains("echo_off_at_prompt=True"),
        "{}",
        run.report
    );
    assert!(!run.text.contains(PASSWORD), "{}", run.text);
    assert!(!run.report.contains("echo_after=off"), "{}", run.report);
}

#[test]
fn wifi_ctrl_c_at_the_password_prompt_restores_the_echo_and_sends_nothing() {
    let env = Env::new();
    let run = env.wifi(&[], CONNECTED, "ctrlc", &[]);
    assert_ne!(run.code, 0, "{}", run.text);
    assert!(
        run.report.contains("echo_off_at_prompt=True"),
        "{}",
        run.report
    );
    assert!(!run.report.contains("echo_after=off"), "{}", run.report);
    assert!(env.console_lines().is_empty(), "nothing was sent");
    assert!(
        !env.path("console-args.txt").exists(),
        "the console was not even started"
    );
}

#[test]
fn wifi_with_no_terminal_asks_nothing_and_calls_nothing() {
    let env = Env::new();
    write_exec(&env.path("console.sh"), FAKE_CONSOLE);
    let mut cmd = Command::new("/usr/bin/python3");
    cmd.arg("-I")
        .arg("-c")
        .arg(HELPER)
        .arg("notty")
        .arg(format!("{SSID}\n{PASSWORD}"));
    cmd.arg("/bin/sh").arg(SCRIPT).arg("wifi");
    let output = cmd
        .current_dir(&env.root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", env.path("tmp"))
        .env(
            "SOLITE_PORT_PATTERNS",
            format!("{}/ports/usbserial*", env.root.display()),
        )
        .env("SOLITE_SERIAL_CONSOLE", env.path("console.sh"))
        .env("FAKE_CONSOLE_ARGS", env.path("console-args.txt"))
        .env("FAKE_CONSOLE_ENV", env.path("console-env.txt"))
        .env("FAKE_CONSOLE_LINES", env.path("console-lines.txt"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("box: no terminal to ask for the Wi-Fi settings, nothing done"),
        "{text}"
    );
    assert!(
        !env.path("console-args.txt").exists(),
        "the console was not called"
    );
}

#[test]
fn wifi_prints_a_refusal_and_exits_1() {
    let env = Env::new();
    let run = env.wifi(
        &[],
        "wifi: settings refused (invalid password)\n",
        "typed",
        &[],
    );
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains("box: wifi: settings refused (invalid password)"),
        "{}",
        run.text
    );
}

#[test]
fn wifi_with_a_silent_console_says_so_and_exits_4() {
    let env = Env::new();
    // The answer window is 15 s; 1 s here keeps the test fast.
    let run = env.wifi(
        &[],
        "",
        "typed",
        &[
            ("FAKE_CONSOLE_SILENT", "1"),
            ("SOLITE_WIFI_ANSWER_SECONDS", "1"),
        ],
    );
    assert_eq!(run.code, 4, "{}", run.text);
    assert!(
        run.text
            .contains("box: no answer from Solite's console on ")
            && run.text.contains("is the radio image running?"),
        "{}",
        run.text
    );
}

#[test]
fn wifi_saved_but_never_connected_prints_the_last_wifi_line_and_exits_1() {
    let env = Env::new();
    let answer =
        "wifi: settings saved\nwifi: not connected (wrong password or refused), retry in 1 s\n";
    let run = env.wifi(&[], answer, "typed", &[("FAKE_CONSOLE_STAY", "6")]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains("box: wifi: not connected (wrong password or refused), retry in 1 s"),
        "{}",
        run.text
    );
}

#[test]
fn wifi_forget_sends_wifi_forget_and_exits_0() {
    let env = Env::new();
    let run = env.wifi(&["--forget"], "wifi: settings removed\n", "typed", &[]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(env.console_lines(), ["wifi-forget"]);
    assert!(
        !run.text.contains("network name"),
        "no prompt for --forget: {}",
        run.text
    );
}

#[test]
fn pair_sends_pair_and_exits_0_when_the_window_opens() {
    let env = Env::new();
    let run = env.console_command("pair", &[], "pairing: open for 180 s\n", "typed", &[]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(env.console_lines(), ["pair"]);
    assert!(
        run.text.contains("box: pairing is open (open for 180 s)"),
        "{}",
        run.text
    );
    assert!(
        !run.text.contains("network name"),
        "no prompt: {}",
        run.text
    );
}

#[test]
fn pair_with_a_silent_console_says_so_and_exits_4() {
    let env = Env::new();
    let run = env.console_command(
        "pair",
        &[],
        "",
        "typed",
        &[
            ("FAKE_CONSOLE_SILENT", "1"),
            ("SOLITE_WIFI_ANSWER_SECONDS", "1"),
        ],
    );
    assert_eq!(run.code, 4, "{}", run.text);
    // The line sent is checked by the test above: the fake console may not have written it yet
    // when the 1 s window ends.
    assert!(
        run.text
            .contains("box: no answer from Solite's console on "),
        "{}",
        run.text
    );
}

#[test]
fn pair_taken_by_the_console_but_never_opened_names_the_engine_and_exits_1() {
    let env = Env::new();
    let run = env.console_command(
        "pair",
        &[],
        "pairing: asked from the console\n",
        "typed",
        // 3 s for the answer, not 1: under `validate.sh`'s parallel load the fake console's shell
        // took more than 1 s to answer (spec 024's run, twice).
        &[
            ("SOLITE_WIFI_ANSWER_SECONDS", "3"),
            ("FAKE_CONSOLE_STAY", "5"),
        ],
    );
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains("the console took the line but the engine opened no pairing window"),
        "{}",
        run.text
    );
    assert!(!run.text.contains("no answer from"), "{}", run.text);
}

#[test]
fn pair_prints_a_failure_and_exits_1() {
    let env = Env::new();
    let run = env.console_command(
        "pair",
        &[],
        "pairing: failed: the radio refused\n",
        "typed",
        &[],
    );
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains("box: pairing: failed: the radio refused"),
        "{}",
        run.text
    );
}

// ---- The radio image with Wi-Fi (spec 023) ------------------------------------------------

#[test]
fn flash_radio_refuses_a_build_without_the_embedded_interface() {
    let (env, f, _) = ready("radio");
    env.set_radio_build(Build::RadioWithoutUi);
    let out = env.go(&args_of(
        &env.port(),
        &["flash", "--image", "radio", "--backup", &f],
    ));
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("solite-ui"), "{}", out.text);
    assert!(out.text.contains(RADIO_BUILD_TEXT), "{}", out.text);
    assert!(out.text.contains("npm run build"), "{}", out.text);
    assert!(!out.text.contains("to go on:") && !env.wrote());
}

/// The radio log without the lines that contain any of `drop`.
fn radio_log_without(drop: &[&str]) -> String {
    radio_log()
        .lines()
        .filter(|l| !drop.iter().any(|d| l.contains(d)))
        .map(|l| format!("{l}\n"))
        .collect()
}

#[test]
fn log_radio_with_wifi_passes_and_prints_its_four_network_lines() {
    let env = Env::new();
    let out = log_from(&env, &radio_log(), &["--image", "radio"]);
    assert_eq!(out.code, 0, "{}", out.text);
    for line in [
        "network: wifi (setting auto)",
        "wifi: connected, address 192.168.1.20, name solite-3abcde, channel 6, rssi -61 dBm",
        "serving: http://192.168.1.20/",
        "clock: synced, 2026-11-22T10:00:00.000Z",
        "memory-network: internal 150 KB free",
    ] {
        assert!(out.text.contains(line), "{line} missing in {}", out.text);
    }
}

#[test]
fn log_radio_with_the_ethernet_setting_needs_no_wifi_line() {
    let env = Env::new();
    let text = radio_log_without(&["wifi:", "serving:", "clock:", "memory-network:"]).replace(
        "network: wifi (setting auto)\n",
        "network: ethernet (setting ethernet), not in this image, no link started\n",
    );
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text
            .contains("network: ethernet (setting ethernet), not in this image, no link started"),
        "{}",
        out.text
    );
}

#[test]
fn log_radio_without_settings_passes_and_prints_the_wifi_line() {
    let env = Env::new();
    let text = radio_log_without(&["wifi:", "serving:", "clock:", "memory-network:"]).replace(
        "I (432) solite: console: ready\n",
        "I (432) solite: console: ready\n\
         W (433) solite: wifi: no settings (/data/wifi.json missing), no link started: sh scripts/box.sh wifi\n",
    );
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text
            .contains("wifi: no settings (/data/wifi.json missing), no link started"),
        "{}",
        out.text
    );
}

#[test]
fn log_radio_fails_when_no_serving_line_follows_the_connection() {
    let env = Env::new();
    let out = log_from(
        &env,
        &radio_log_without(&["serving:"]),
        &["--image", "radio"],
    );
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("the log is missing the serving line (serving: http://...)"),
        "{}",
        out.text
    );
    // A serving line before the connection does not count.
    let early = radio_log_without(&["serving:"]).replace(
        "I (434) solite: wifi: connecting\n",
        "I (434) solite: wifi: connecting\nI (435) solite: serving: http://192.168.1.20/\n",
    );
    let out = log_from(&env, &early, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
}

#[test]
fn log_radio_fails_and_names_refused_settings() {
    let env = Env::new();
    let text = radio_log_without(&["wifi:", "serving:", "clock:", "memory-network:"]).replace(
        "I (432) solite: console: ready\n",
        "I (432) solite: console: ready\n\
         W (433) solite: wifi: settings refused (invalid), no link started\n",
    );
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text.contains(
            "box: the Wi-Fi settings were refused: W (433) solite: wifi: settings refused (invalid)"
        ),
        "{}",
        out.text
    );
}

#[test]
fn log_radio_fails_when_the_wifi_line_is_missing() {
    let env = Env::new();
    let text = radio_log_without(&["wifi:", "serving:", "clock:", "memory-network:"]);
    let out = log_from(&env, &text, &["--image", "radio"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text.contains("the Wi-Fi line (wifi: ...)"),
        "{}",
        out.text
    );
}

#[test]
fn log_box_and_sim_still_need_no_link_started() {
    let env = Env::new();
    let started = box_log().replace(
        "network: wifi (setting auto), no link started",
        "network: wifi (setting auto)",
    );
    let out = log_from(&env, &started, &["--image", "box"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("the network line"), "{}", out.text);
    let sim = sim_log(170, 100, 50, true).replace(
        "network: wifi (setting auto), no link started",
        "network: wifi (setting auto)",
    );
    let out = log_from(&env, &sim, &["--image", "sim"]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(out.text.contains("the network line"), "{}", out.text);
}

#[test]
fn wifi_waits_15_s_for_the_console_and_reads_its_log_lines() {
    let env = Env::new();
    // The console's lines come through ESP-IDF's log, with its prefix.
    let answer = "I (52340) solite_firmware::console: wifi: settings saved\n\
                  I (4100) solite_firmware::wifi: wifi: connected, address 192.168.1.20, name solite-a1b2c3, channel 6, rssi -61 dBm\n";
    let run = env.wifi(&[], answer, "typed", &[("FAKE_CONSOLE_DELAY", "6")]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text
            .contains("box: Wi-Fi connected, address 192.168.1.20, name solite-a1b2c3"),
        "{}",
        run.text
    );
}

// ---- Saves held for one test, and the flash-write budget (spec 025) ------------------------------

/// A console that stays up, reads every line and answers `saves hold` and `saves flush` with the
/// text of their files; with `FAKE_CONSOLE_SILENT` it answers nothing.
const FAKE_SAVES_CONSOLE: &str = r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$FAKE_CONSOLE_LINES"
  [ -n "$FAKE_CONSOLE_SILENT" ] && continue
  case "$line" in
    "saves hold") cat "$FAKE_SAVES_HOLD" ;;
    "saves flush") cat "$FAKE_SAVES_FLUSH" ;;
  esac
done
"#;

const HELD: &str = "I (1500) solite: flash: 1 writes since boot\n\
    I (1600) solite: saves: held, nothing is written until saves flush or a restart\n\
    I (9000) solite: serving: http://192.168.1.20/\n";
const FLUSHED: &str = "I (20000) solite: saves: flush asked\n\
    I (20100) solite: flash: 2 writes since boot\n\
    I (20200) solite: flash: 3 writes since boot\n\
    I (20300) solite: saves: flushed, 2 files written\n";

/// Runs `box.sh saves <args>` with the fake console answering `hold` and `flush`; the environment
/// is returned so the test can read what the console received.
fn saves(hold: &str, flush: &str, args: &[&str], extra: &[(&str, &str)]) -> (Env, Out) {
    let env = Env::new();
    let console = env.path("saves-console.sh");
    write_exec(&console, FAKE_SAVES_CONSOLE);
    fs::write(env.path("hold.txt"), hold).unwrap();
    fs::write(env.path("flush.txt"), flush).unwrap();
    let lines = env.path("console-lines.txt").display().to_string();
    let hold_file = env.path("hold.txt").display().to_string();
    let flush_file = env.path("flush.txt").display().to_string();
    let mut env = env
        .with("SOLITE_SERIAL_CONSOLE", &console.display().to_string())
        .with("FAKE_CONSOLE_LINES", &lines)
        .with("FAKE_SAVES_HOLD", &hold_file)
        .with("FAKE_SAVES_FLUSH", &flush_file)
        .with("SOLITE_WIFI_WAIT_SECONDS", "3")
        .with("SOLITE_SAVES_FLUSH_SECONDS", "3");
    for (k, v) in extra {
        env = env.with(k, v);
    }
    let mut all = vec!["saves"];
    all.extend_from_slice(args);
    let out = env.run(&all, Tty::NoTty, "");
    (env, out)
}

#[test]
fn saves_holds_runs_the_command_flushes_and_prints_the_count() {
    let env_probe = Env::new();
    let probe = env_probe.path("address.txt");
    let command = format!("echo \"$SOLITE_BOX_ADDRESS\" > {}", probe.display());
    let (env, out) = saves(HELD, FLUSHED, &["--", "sh", "-c", &command], &[]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert_eq!(env.console_lines(), ["saves hold", "saves flush"]);
    assert_eq!(fs::read_to_string(&probe).unwrap().trim(), "192.168.1.20");
    for line in [
        "box: saves held; serving at http://192.168.1.20/",
        "box: saves flushed (2 files written)",
        "box: flash: 3 writes since boot",
        "box: serial log: ",
    ] {
        assert!(out.text.contains(line), "{line}: {}", out.text);
    }
}

#[test]
fn saves_fails_over_its_budget_and_passes_at_it() {
    let (env, out) = saves(HELD, FLUSHED, &["--budget", "2", "--", "true"], &[]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("box: flash: 3 writes since boot, over the budget of 2"),
        "{}",
        out.text
    );
    assert_eq!(
        env.console_lines(),
        ["saves hold", "saves flush"],
        "flushed all the same"
    );
    let (_, out) = saves(HELD, FLUSHED, &["--budget", "3", "--", "true"], &[]);
    assert_eq!(out.code, 0, "{}", out.text);
}

#[test]
fn saves_no_flush_sends_only_the_hold_and_counts_the_boot_marker_read_before_it() {
    let (env, out) = saves(HELD, FLUSHED, &["--no-flush", "--", "true"], &[]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert_eq!(env.console_lines(), ["saves hold"]);
    assert!(
        out.text
            .contains("box: saves still held: the changes stay in RAM"),
        "{}",
        out.text
    );
    assert!(
        out.text.contains("box: flash: 1 writes since boot"),
        "{}",
        out.text
    );
}

#[test]
fn saves_with_a_silent_console_says_so_and_exits_4() {
    let (_, out) = saves(
        HELD,
        FLUSHED,
        &["--", "true"],
        &[
            ("FAKE_CONSOLE_SILENT", "1"),
            ("SOLITE_WIFI_ANSWER_SECONDS", "1"),
        ],
    );
    assert_eq!(out.code, 4, "{}", out.text);
    assert!(
        out.text
            .contains("box: no answer from Solite's console on "),
        "{}",
        out.text
    );
}

#[test]
fn saves_without_a_serving_line_runs_nothing_and_flushes_nothing() {
    let env_probe = Env::new();
    let probe = env_probe.path("ran.txt");
    let command = format!("touch {}", probe.display());
    let (env, out) = saves(
        "saves: held, nothing is written until saves flush or a restart\n",
        FLUSHED,
        &["--", "sh", "-c", &command],
        &[("SOLITE_WIFI_WAIT_SECONDS", "1")],
    );
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text.contains("box: the box serves nothing within 1 s"),
        "{}",
        out.text
    );
    assert!(!probe.exists(), "the command was not run");
    assert_eq!(env.console_lines(), ["saves hold"]);
}

#[test]
fn saves_flushes_even_when_the_command_fails_and_exits_1() {
    let (env, out) = saves(HELD, FLUSHED, &["--", "sh", "-c", "exit 3"], &[]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert_eq!(env.console_lines(), ["saves hold", "saves flush"]);
    assert!(
        out.text.contains("box: the command failed (exit 3)"),
        "{}",
        out.text
    );
    assert!(
        out.text.contains("box: saves flushed (2 files written)"),
        "{}",
        out.text
    );
}

#[test]
fn saves_with_a_command_that_cannot_start_still_flushes_and_exits_1() {
    let (env, out) = saves(HELD, FLUSHED, &["--", "/nonexistent/solite-command"], &[]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text.contains("box: cannot run the command"),
        "{}",
        out.text
    );
    assert!(
        out.text.contains("box: the command failed (exit 127)"),
        "{}",
        out.text
    );
    assert_eq!(env.console_lines(), ["saves hold", "saves flush"]);
}

#[test]
fn saves_with_a_flush_asked_but_never_done_names_the_engine_and_exits_1() {
    let (_, out) = saves(
        HELD,
        "saves: flush asked\n",
        &["--", "true"],
        &[("SOLITE_SAVES_FLUSH_SECONDS", "1")],
    );
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("box: the console took the line but the engine flushed nothing within 1 s"),
        "{}",
        out.text
    );
}

#[test]
fn saves_fails_when_the_box_restarted_during_the_session_and_sends_no_flush() {
    let restarted = format!("{HELD}I (300) solite: console: ready\n");
    let (env, out) = saves(&restarted, FLUSHED, &["--", "true"], &[]);
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("box: the box restarted during the session: the hold was lost"),
        "{}",
        out.text
    );
    assert_eq!(env.console_lines(), ["saves hold"]);
}

#[test]
fn saves_interrupted_stops_the_command_flushes_and_exits_130() {
    let (env, out) = saves(
        HELD,
        FLUSHED,
        &["--", "sh", "-c", "kill -TERM $PPID; sleep 5"],
        &[],
    );
    assert_eq!(out.code, 130, "{}", out.text);
    assert_eq!(env.console_lines(), ["saves hold", "saves flush"]);
}

#[test]
fn saves_with_neither_a_command_nor_a_terminal_does_nothing_and_exits_2() {
    let (env, out) = saves(HELD, FLUSHED, &[], &[]);
    assert_eq!(out.code, 2, "{}", out.text);
    assert!(
        out.text
            .contains("box: saves needs a command after -- or a terminal"),
        "{}",
        out.text
    );
    assert!(env.console_lines().is_empty(), "the console never started");
}

fn box_log_with_writes() -> String {
    format!(
        "{}I (900) solite: flash: 3 writes since boot\nI (950) solite: flash: 4 writes since boot\n",
        box_log()
    )
}

#[test]
fn log_budget_passes_at_the_budget_and_prints_the_count() {
    let env = Env::new();
    let out = log_from(
        &env,
        &box_log_with_writes(),
        &["--image", "box", "--budget", "4"],
    );
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text.contains("flash: 4 writes since boot"),
        "{}",
        out.text
    );
}

#[test]
fn log_budget_fails_over_the_budget() {
    let env = Env::new();
    let out = log_from(
        &env,
        &box_log_with_writes(),
        &["--image", "box", "--budget", "3"],
    );
    assert_eq!(out.code, 1, "{}", out.text);
    assert!(
        out.text
            .contains("box: flash: 4 writes since boot, over the budget of 3"),
        "{}",
        out.text
    );
}

#[test]
fn log_budget_counts_0_without_a_flash_line() {
    let env = Env::new();
    let out = log_from(&env, &box_log(), &["--image", "box", "--budget", "0"]);
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text.contains("flash: 0 writes since boot"),
        "{}",
        out.text
    );
}

#[test]
fn a_budget_that_is_not_a_whole_number_is_a_usage_error() {
    let env = Env::new();
    for budget in ["x", "-1", "2.5", ""] {
        let out = log_from(&env, &box_log(), &["--image", "box", "--budget", budget]);
        assert_eq!(out.code, 2, "{budget}: {}", out.text);
    }
}
