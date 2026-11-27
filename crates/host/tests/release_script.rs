//! `scripts/release.sh` (spec 038, R1 and R2): the checks, the files of a release and the care
//! taken with the signing key, run with a fake builder (a shell script standing for the radio
//! build), the Mac host as the signing tool, and a small tree holding the version files and the
//! release notes. No test builds the firmware or touches the network. Keys are made here with
//! `--update-keygen`, in the system's temporary folder (outside any git tree).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/release.sh");
const VERSION: &str = "0.2.0";
const TIME: &str = "2026-12-01T09:30:00Z";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The fake builder: what `own_builder` would write, from a few bytes. The `FAKE_*` variables
/// make it record what it sees or misbehave.
const BUILDER: &str = r#"#!/bin/sh
[ -z "${FAKE_MARK:-}" ] || echo started >"$FAKE_MARK"
[ -z "${FAKE_ARGS:-}" ] || echo "$1 $2 $3" >"$FAKE_ARGS"
[ -z "${FAKE_ENV_OUT:-}" ] || env >"$FAKE_ENV_OUT"
if [ -n "${FAKE_KEYDIR_OUT:-}" ]; then
  n=0
  for d in "$TMPDIR"/solite-release-key.*; do
    [ -d "$d" ] || continue
    n=$((n + 1))
    ls -A "$d"
  done >"$FAKE_KEYDIR_OUT.list"
  echo "dirs: $n" >"$FAKE_KEYDIR_OUT"
fi
[ -z "${FAKE_SLEEP:-}" ] || sleep "$FAKE_SLEEP"
if [ -n "${FAKE_CHILD:-}" ]; then
  sleep 47 &
  echo $! >"$FAKE_CHILD"
  wait
fi
if [ -n "${FAKE_FAIL:-}" ]; then
  echo "the fake build broke"
  exit 1
fi
printf 'fake application image for %s, key %s, recipe key %s' "$1" "$2" "${SOLITE_RECIPE_PUBLIC_KEY:-none}" >"$3/app.bin"
printf 'fake full flash image' >"$3/flash.bin"
"#;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "solite-release-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        for sub in ["tree/firmware", "tree/ui", "tree/docs", "keys", "tmp"] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        let fixture = Fixture { root };
        fixture.write_tree(VERSION, VERSION, VERSION);
        fixture.write_notes(&format!(
            "# Release notes\n\n## {VERSION} (2026-12-01)\n\nA first line.\n\n- one\n- two\n\n## 0.1.0\n\nOld.\n"
        ));
        fs::write(fixture.builder(), BUILDER).unwrap();
        let (key, public) = fixture.keygen("signing.key");
        assert_eq!(key, fixture.key());
        fs::write(fixture.public(), format!("{public}\n")).unwrap();
        // The recipe key's public half (spec 042 R1.5): another key.
        let (_, recipe_public) = fixture.keygen("recipe.key");
        fs::write(fixture.recipe_public(), format!("{recipe_public}\n")).unwrap();
        fixture
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.root.join(sub)
    }
    fn builder(&self) -> PathBuf {
        self.path("builder.sh")
    }
    fn key(&self) -> PathBuf {
        self.path("keys/signing.key")
    }
    fn public(&self) -> PathBuf {
        self.path("keys/update-key.pub")
    }
    fn recipe_public(&self) -> PathBuf {
        self.path("keys/recipe-key.pub")
    }
    fn out(&self) -> PathBuf {
        self.path("out")
    }
    fn tmp(&self) -> PathBuf {
        self.path("tmp")
    }

    fn write_tree(&self, workspace: &str, firmware: &str, ui: &str) {
        fs::write(
            self.path("tree/Cargo.toml"),
            format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{workspace}\"\nedition = \"2024\"\n"),
        )
        .unwrap();
        fs::write(
            self.path("tree/firmware/Cargo.toml"),
            format!("[package]\nname = \"solite-firmware\"\nversion = \"{firmware}\"\n\n[dependencies]\n"),
        )
        .unwrap();
        fs::write(
            self.path("tree/ui/package.json"),
            format!("{{\n  \"name\": \"ui\",\n  \"version\": \"{ui}\",\n  \"private\": true\n}}\n"),
        )
        .unwrap();
    }

    fn write_notes(&self, text: &str) {
        fs::write(self.path("tree/docs/release-notes.md"), text).unwrap();
    }

    /// A key made by `--update-keygen`: its file and its public key.
    fn keygen(&self, name: &str) -> (PathBuf, String) {
        let file = self.path("keys").join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_solite-host"))
            .arg("--update-keygen")
            .arg(&file)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let public = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|l| l.strip_prefix("public key: ").map(str::to_string))
            .expect("the public key line");
        (file, public)
    }

    fn key_hex(&self) -> String {
        fs::read_to_string(self.key()).unwrap().trim().to_string()
    }
    fn recipe_public_hex(&self) -> String {
        fs::read_to_string(self.recipe_public())
            .unwrap()
            .trim()
            .to_string()
    }
    fn public_hex(&self) -> String {
        fs::read_to_string(self.public())
            .unwrap()
            .trim()
            .to_string()
    }

    /// The script's command: the seams set, GitHub's variables removed, a temporary folder of
    /// its own (so the key, work and log folders can be counted).
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("sh");
        command
            .arg(SCRIPT)
            .args(args)
            .env("SOLITE_HOST_BIN", env!("CARGO_BIN_EXE_solite-host"))
            .env("SOLITE_RELEASE_BUILDER", self.builder())
            .env("SOLITE_RELEASE_TREE", self.path("tree"))
            .env("SOLITE_RELEASE_TIME", TIME)
            .env("TMPDIR", self.tmp())
            .env_remove("GITHUB_ACTIONS")
            .env_remove("GITHUB_REPOSITORY");
        command
    }

    /// The usual arguments: the version, the key file, the public key file, the output.
    fn args(&self) -> Vec<String> {
        [
            VERSION,
            "--key-file",
            self.key().to_str().unwrap(),
            "--public-key-file",
            self.public().to_str().unwrap(),
            "--recipe-key-file",
            self.recipe_public().to_str().unwrap(),
            "--out",
            self.out().to_str().unwrap(),
        ]
        .map(String::from)
        .to_vec()
    }

    fn run_with(&self, args: &[String], envs: &[(&str, &str)]) -> Out {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut command = self.command(&args);
        for (name, value) in envs {
            command.env(name, value);
        }
        Out::of(command.output().unwrap())
    }

    fn run(&self) -> Out {
        self.run_with(&self.args(), &[])
    }

    /// The usual arguments with extra ones at the end.
    fn run_plus(&self, extra: &[&str]) -> Out {
        let mut args = self.args();
        args.extend(extra.iter().map(|a| a.to_string()));
        self.run_with(&args, &[])
    }

    /// The usual arguments but for the key: `--key-env` naming a variable that holds the key.
    fn key_env_args(&self, name: &str) -> Vec<String> {
        let mut args = self.args();
        let at = args.iter().position(|a| a == "--key-file").unwrap();
        args[at] = "--key-env".into();
        args[at + 1] = name.into();
        args
    }

    /// The entries of the temporary folder that start with `prefix`.
    fn leftovers(&self, prefix: &str) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(self.tmp())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(prefix))
            .collect();
        found.sort();
        found
    }

    fn manifest(&self) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(self.out().join("manifest.json")).unwrap())
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Out {
    fn of(output: std::process::Output) -> Out {
        Out {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn all(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }

    #[track_caller]
    fn ok(&self) {
        assert_eq!(self.code, 0, "{}", self.all());
    }

    /// Exit 2, with `reason` in the messages.
    #[track_caller]
    fn refused(&self, reason: &str) {
        assert_eq!(self.code, 2, "{}", self.all());
        assert!(self.stderr.contains(reason), "{}", self.all());
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// True when the bytes of a file, or of a text, hold the needle.
fn holds(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.to_lowercase();
    String::from_utf8_lossy(haystack)
        .to_lowercase()
        .contains(&needle)
}

fn five_files() -> Vec<String> {
    [
        "SHA256SUMS".to_string(),
        "manifest.json".to_string(),
        "notes.md".to_string(),
        format!("solite-{VERSION}-flash.bin"),
        format!("solite-{VERSION}.sup"),
    ]
    .to_vec()
}

// ---------------------------------------------------------------------------------------------
// The output
// ---------------------------------------------------------------------------------------------

#[test]
fn a_release_with_a_key_file_writes_the_five_files_and_the_step_lines() {
    let fx = Fixture::new();
    let args = fx.dir_for_builder_args();
    let out = fx.run_with(&fx.args(), &[("FAKE_ARGS", args.to_str().unwrap())]);
    out.ok();
    assert_eq!(names(&fx.out()), five_files());
    let pub8 = &fx.public_hex()[..8];
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines.contains(&format!("release: version {VERSION} checked").as_str()));
    assert!(lines.contains(&format!("release: key matches the public key {pub8}").as_str()));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("release: radio image built (")
                && l.contains(" KB, ")
                && l.contains("% of the 4096 KB ota_0 slot)")),
        "{}",
        out.stdout
    );
    assert!(lines.contains(&format!("release: signed and verified (public key {pub8})").as_str()));
    assert_eq!(
        lines.last().copied(),
        Some(format!("release: {}", fx.out().display()).as_str())
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("release: builder ") && l.ends_with("(a test seam)"))
    );
    // The builder is given the version, the public key and its folder.
    let given = fs::read_to_string(&args).unwrap();
    assert!(
        given.starts_with(&format!("{VERSION} {} ", fx.public_hex())),
        "{given}"
    );
    // Nothing is left: no key, work or log folder.
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

#[test]
fn a_release_with_a_key_in_the_environment_writes_the_same_files_and_leaves_no_key_file() {
    let fx = Fixture::new();
    let args = fx.key_env_args("SOLITE_TEST_KEY");
    let out = fx.run_with(&args, &[("SOLITE_TEST_KEY", &fx.key_hex())]);
    out.ok();
    assert_eq!(names(&fx.out()), five_files());
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

#[test]
fn the_manifest_holds_the_fields_of_r2() {
    let fx = Fixture::new();
    fx.run_plus(&["--repo", "someone/solite-fork"]).ok();
    let manifest = fx.manifest();
    let sup = fs::read(fx.out().join(format!("solite-{VERSION}.sup"))).unwrap();
    let text = fs::read_to_string(fx.out().join("manifest.json")).unwrap();
    assert!(
        text.ends_with("}\n") && text.contains("\n  \"format\": 1,"),
        "{text}"
    );
    assert_eq!(manifest["format"], 1);
    assert_eq!(manifest["version"], VERSION);
    assert_eq!(manifest["releasedAt"], TIME);
    assert_eq!(manifest["notes"], "A first line.\n\n- one\n- two");
    assert!(manifest["minVersion"].is_null());
    let image = &manifest["image"];
    assert_eq!(image["name"], format!("solite-{VERSION}.sup"));
    assert_eq!(image["size"], sup.len());
    assert_eq!(
        image["sha256"],
        sha256_of(&fx.out().join(format!("solite-{VERSION}.sup")))
    );
    assert_eq!(
        image["url"],
        format!(
            "https://github.com/someone/solite-fork/releases/download/v{VERSION}/solite-{VERSION}.sup"
        )
    );
    assert_eq!(
        fs::read_to_string(fx.out().join("notes.md")).unwrap(),
        "A first line.\n\n- one\n- two\n"
    );
}

#[test]
fn the_repository_defaults_to_the_projects_one() {
    let fx = Fixture::new();
    fx.run().ok();
    let url = fx.manifest()["image"]["url"].as_str().unwrap().to_string();
    assert!(
        url.starts_with("https://github.com/mchacher/solite/releases/download/v"),
        "{url}"
    );
}

#[test]
fn min_version_is_written_and_must_be_lower_than_the_version() {
    let fx = Fixture::new();
    fx.run_plus(&["--min-version", "0.1.0"]).ok();
    assert_eq!(fx.manifest()["minVersion"], "0.1.0");
    for bad in ["0.2.0", "0.2.1", "1.0.0", "0.2", "abc", "0.1.0-rc1"] {
        let fx = Fixture::new();
        fx.run_plus(&["--min-version", bad])
            .refused("--min-version");
        assert!(!fx.out().exists(), "{bad}");
    }
}

fn sha256_of(file: &Path) -> String {
    let output = Command::new("shasum")
        .args(["-a", "256"])
        .arg(file)
        .output()
        .or_else(|_| Command::new("sha256sum").arg(file).output())
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn sha256sums_has_three_sorted_lines_that_the_checksum_tools_accept() {
    let fx = Fixture::new();
    fx.run().ok();
    let text = fs::read_to_string(fx.out().join("SHA256SUMS")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let expected = [
        "manifest.json".to_string(),
        format!("solite-{VERSION}-flash.bin"),
        format!("solite-{VERSION}.sup"),
    ];
    assert_eq!(lines.len(), 3, "{text}");
    for (line, name) in lines.iter().zip(&expected) {
        let (hash, file) = line.split_once("  ").expect("two spaces");
        assert_eq!(file, name);
        assert_eq!(hash, sha256_of(&fx.out().join(name)));
    }
    let checked = Command::new("shasum")
        .args(["-a", "256", "-c", "SHA256SUMS"])
        .current_dir(fx.out())
        .output()
        .or_else(|_| {
            Command::new("sha256sum")
                .args(["-c", "SHA256SUMS"])
                .current_dir(fx.out())
                .output()
        })
        .unwrap();
    assert!(checked.status.success(), "{checked:?}");
}

#[test]
fn the_sup_is_accepted_by_update_verify_and_holds_the_built_image() {
    let fx = Fixture::new();
    fx.run().ok();
    let sup = fx.out().join(format!("solite-{VERSION}.sup"));
    let output = Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .arg("--update-verify")
        .arg(&sup)
        .args(["--public-key", &fx.public_hex()])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(
        text.contains(&format!("version {VERSION}, "))
            && text.contains(&format!("key {}", &fx.public_hex()[..8])),
        "{text}"
    );
    let bytes = fs::read(&sup).unwrap();
    assert_eq!(
        &bytes[128..],
        format!(
            "fake application image for {VERSION}, key {}, recipe key {}",
            fx.public_hex(),
            fx.recipe_public_hex()
        )
        .as_bytes()
    );
    assert_eq!(
        fs::read(fx.out().join(format!("solite-{VERSION}-flash.bin"))).unwrap(),
        b"fake full flash image"
    );
}

#[test]
fn a_tag_is_accepted_as_the_version() {
    let fx = Fixture::new();
    let mut args = fx.args();
    args[0] = format!("v{VERSION}");
    fx.run_with(&args, &[]).ok();
    assert_eq!(names(&fx.out()), five_files());
    assert_eq!(fx.manifest()["version"], VERSION);
}

#[test]
fn the_keys_text_is_in_no_output_file_and_no_line_for_both_ways_of_giving_it() {
    let fx = Fixture::new();
    let key = fx.key_hex();
    let out = fx.run();
    out.ok();
    for name in names(&fx.out()) {
        let bytes = fs::read(fx.out().join(&name)).unwrap();
        assert!(!holds(&bytes, &key), "{name} holds the key");
    }
    assert!(!holds(out.all().as_bytes(), &key));

    let fx = Fixture::new();
    let key = fx.key_hex();
    let out = fx.run_with(
        &fx.key_env_args("SOLITE_TEST_KEY"),
        &[("SOLITE_TEST_KEY", &key)],
    );
    out.ok();
    for name in names(&fx.out()) {
        let bytes = fs::read(fx.out().join(&name)).unwrap();
        assert!(!holds(&bytes, &key), "{name} holds the key");
    }
    assert!(!holds(out.all().as_bytes(), &key));
}

#[test]
fn a_key_with_spaces_and_a_newline_and_a_public_key_with_a_newline_are_accepted() {
    let fx = Fixture::new();
    let key = fx.key_hex();
    fs::write(fx.key(), format!("  {key}  \n\n")).unwrap();
    fs::write(fx.public(), format!("{}\n\n", fx.public_hex())).unwrap();
    fx.run().ok();
    assert_eq!(names(&fx.out()), five_files());

    let fx = Fixture::new();
    let args = fx.key_env_args("SOLITE_TEST_KEY");
    fx.run_with(
        &args,
        &[("SOLITE_TEST_KEY", &format!("{} \n", fx.key_hex()))],
    )
    .ok();
}

#[test]
fn an_empty_output_folder_is_accepted() {
    let fx = Fixture::new();
    fs::create_dir_all(fx.out()).unwrap();
    fx.run().ok();
    assert_eq!(names(&fx.out()), five_files());
}

// ---------------------------------------------------------------------------------------------
// The refusals
// ---------------------------------------------------------------------------------------------

/// A refusal leaves nothing: the builder did not run, the output is absent, no folder is left.
#[track_caller]
fn nothing_built(fx: &Fixture, mark: &Path) {
    assert!(!mark.exists(), "the builder ran");
    assert!(!fx.out().exists(), "the output folder exists");
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

impl Fixture {
    fn mark(&self) -> PathBuf {
        self.path("builder-ran")
    }
    fn dir_for_builder_args(&self) -> PathBuf {
        self.path("builder-args")
    }
    fn run_marked(&self, args: &[String], envs: &[(&str, &str)]) -> Out {
        let mark = self.mark();
        let mut all = envs.to_vec();
        all.push(("FAKE_MARK", mark.to_str().unwrap()));
        self.run_with(args, &all)
    }
}

#[test]
fn a_version_that_is_not_the_trees_is_refused_naming_the_file() {
    for (which, trees) in [
        ("Cargo.toml", ["0.1.0", VERSION, VERSION]),
        ("firmware/Cargo.toml", [VERSION, "0.1.0", VERSION]),
        ("ui/package.json", [VERSION, VERSION, "0.1.0"]),
    ] {
        let fx = Fixture::new();
        fx.write_tree(trees[0], trees[1], trees[2]);
        let out = fx.run_marked(&fx.args(), &[]);
        out.refused(&format!("{which} says version 0.1.0, not {VERSION}"));
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn a_version_that_is_not_x_y_z_is_refused() {
    for bad in ["1.2", "0.2.0-rc1", "abc", "0.2.0.1", "70000.0.0", "-1.0.0"] {
        let fx = Fixture::new();
        let mut args = fx.args();
        args[0] = bad.to_string();
        let out = fx.run_marked(&args, &[]);
        assert_eq!(out.code, 2, "{bad}: {}", out.all());
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn a_release_without_a_key_option_is_refused() {
    let fx = Fixture::new();
    let args: Vec<String> = [
        VERSION,
        "--public-key-file",
        fx.public().to_str().unwrap(),
        "--out",
        fx.out().to_str().unwrap(),
    ]
    .map(String::from)
    .to_vec();
    fx.run_marked(&args, &[])
        .refused("a release is always signed: give --key-file <path> or --key-env <NAME>");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn key_env_naming_an_empty_or_unset_variable_is_refused() {
    let fx = Fixture::new();
    let args = fx.key_env_args("SOLITE_TEST_KEY");
    fx.run_marked(&args, &[("SOLITE_TEST_KEY", "")])
        .refused("the signing key is missing: SOLITE_TEST_KEY is empty");
    nothing_built(&fx, &fx.mark());
    fx.run_marked(&args, &[])
        .refused("the signing key is missing: SOLITE_TEST_KEY is empty");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn both_key_options_are_refused() {
    let fx = Fixture::new();
    let mut args = fx.args();
    args.extend(["--key-env".to_string(), "SOLITE_TEST_KEY".to_string()]);
    let out = fx.run_marked(&args, &[("SOLITE_TEST_KEY", &fx.key_hex())]);
    out.refused("not both");
    assert!(!holds(out.all().as_bytes(), &fx.key_hex()));
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_missing_key_file_is_refused() {
    let fx = Fixture::new();
    fs::remove_file(fx.key()).unwrap();
    fx.run_marked(&fx.args(), &[]).refused("signing.key");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_key_that_is_not_64_hex_characters_is_refused_without_printing_it() {
    let fx = Fixture::new();
    fs::write(fx.key(), "not-a-key-0123\n").unwrap();
    let out = fx.run_marked(&fx.args(), &[]);
    out.refused("64 hexadecimal");
    assert!(!out.all().contains("not-a-key-0123"));
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_missing_or_malformed_public_key_file_is_refused_naming_it() {
    let fx = Fixture::new();
    fs::remove_file(fx.public()).unwrap();
    fx.run_marked(&fx.args(), &[])
        .refused(fx.public().to_str().unwrap());
    nothing_built(&fx, &fx.mark());
    for bad in [&"ab".repeat(31)[..], "zz".repeat(32).as_str(), ""] {
        fs::write(fx.public(), format!("{bad}\n")).unwrap();
        let out = fx.run_marked(&fx.args(), &[]);
        out.refused(fx.public().to_str().unwrap());
        assert!(out.stderr.contains("docs/release.md"), "{}", out.all());
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn a_release_without_the_recipe_keys_public_file_is_refused_before_the_build() {
    // Spec 042 R1.5: no release until the recipe key is made and its public key committed.
    let fx = Fixture::new();
    fs::remove_file(fx.recipe_public()).unwrap();
    let out = fx.run_marked(&fx.args(), &[]);
    out.refused(fx.recipe_public().to_str().unwrap());
    assert!(
        out.stderr.contains("no release until the recipe key"),
        "{}",
        out.all()
    );
    nothing_built(&fx, &fx.mark());
    // A build-only run needs it as well: the image carries the key.
    let mut args = fx.args();
    let at = args.iter().position(|a| a == "--key-file").unwrap();
    args.drain(at..at + 2);
    args.push("--build-only".into());
    fx.run_marked(&args, &[])
        .refused(fx.recipe_public().to_str().unwrap());
    nothing_built(&fx, &fx.mark());
    // Malformed: refused naming the file.
    for bad in [&"ab".repeat(31)[..], "zz".repeat(32).as_str(), ""] {
        fs::write(fx.recipe_public(), format!("{bad}\n")).unwrap();
        let out = fx.run_marked(&fx.args(), &[]);
        out.refused(fx.recipe_public().to_str().unwrap());
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn the_builder_is_given_the_recipe_keys_public_half_in_its_environment() {
    let fx = Fixture::new();
    let env_out = fx.path("env.txt");
    let out = fx.run_with(&fx.args(), &[("FAKE_ENV_OUT", env_out.to_str().unwrap())]);
    out.ok();
    let seen = fs::read_to_string(&env_out).unwrap();
    let recipe_public = fs::read_to_string(fx.recipe_public()).unwrap();
    assert!(
        seen.lines()
            .any(|l| l == format!("SOLITE_RECIPE_PUBLIC_KEY={}", recipe_public.trim())),
        "{seen}"
    );
}

#[test]
fn a_key_that_does_not_match_the_public_key_stops_at_the_probe_before_the_build() {
    let fx = Fixture::new();
    let (_, other_public) = fx.keygen("other.key");
    fs::write(fx.public(), format!("{other_public}\n")).unwrap();
    let out = fx.run_marked(&fx.args(), &[]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.stderr.contains("does not match"), "{}", out.all());
    assert!(!out.stdout.contains("key matches"));
    assert!(!fx.mark().exists(), "the builder ran after a failed probe");
    assert!(!fx.out().exists());
    assert!(!holds(out.all().as_bytes(), &fx.key_hex()));
    // The failed run keeps its log (without the key); the key folder is gone.
    assert_eq!(fx.leftovers("solite-release-key"), Vec::<String>::new());
}

#[test]
fn missing_notes_are_refused_before_the_build_and_notes_file_replaces_them() {
    let fx = Fixture::new();
    fx.write_notes("# Release notes\n\n## 0.1.0\n\nOld.\n");
    let out = fx.run_marked(&fx.args(), &[]);
    out.refused(&format!(
        "no release notes for {VERSION} in docs/release-notes.md"
    ));
    nothing_built(&fx, &fx.mark());

    fx.write_notes(&format!("## {VERSION}\n\n\n"));
    fx.run_marked(&fx.args(), &[])
        .refused("no release notes for");
    nothing_built(&fx, &fx.mark());

    let notes = fx.path("my-notes.md");
    fs::write(&notes, "Custom text.\nSecond line.\n").unwrap();
    fx.run_plus(&["--notes", notes.to_str().unwrap()]).ok();
    assert_eq!(fx.manifest()["notes"], "Custom text.\nSecond line.");
    assert_eq!(
        fs::read_to_string(fx.out().join("notes.md")).unwrap(),
        "Custom text.\nSecond line.\n"
    );
}

#[test]
fn a_dated_heading_is_taken_and_a_longer_version_is_not() {
    let fx = Fixture::new();
    fx.write_notes(&format!(
        "## {VERSION}1\n\nWrong.\n\n## {VERSION} (2026-12-01)\n\nRight.\n\n## 0.1.0\n\nOld.\n"
    ));
    fx.run().ok();
    assert_eq!(fx.manifest()["notes"], "Right.");

    let fx = Fixture::new();
    fx.write_notes(&format!("## {VERSION}1\n\nWrong.\n"));
    fx.run().refused("no release notes for");
}

#[test]
fn a_non_empty_output_folder_is_refused_and_left_alone() {
    let fx = Fixture::new();
    fs::create_dir_all(fx.out()).unwrap();
    fs::write(fx.out().join("old.txt"), "previous run").unwrap();
    let out = fx.run_marked(&fx.args(), &[]);
    out.refused("not empty");
    assert_eq!(names(&fx.out()), ["old.txt"]);
    assert_eq!(
        fs::read_to_string(fx.out().join("old.txt")).unwrap(),
        "previous run"
    );
    assert!(!fx.mark().exists());
}

#[test]
fn a_key_file_inside_the_output_folder_or_a_git_working_tree_is_refused() {
    let fx = Fixture::new();
    let inside = fx.path("out/inner");
    fs::create_dir_all(&inside).unwrap();
    fs::copy(fx.key(), inside.join("signing.key")).unwrap();
    let mut args = fx.args();
    let at = args.iter().position(|a| a == "--key-file").unwrap();
    args[at + 1] = inside.join("signing.key").to_str().unwrap().into();
    fx.run_marked(&args, &[])
        .refused("inside the output folder");
    assert!(!fx.mark().exists());

    let fx = Fixture::new();
    let repo = fx.path("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::create_dir_all(repo.join("deep")).unwrap();
    fs::copy(fx.key(), repo.join("deep/signing.key")).unwrap();
    let mut args = fx.args();
    let at = args.iter().position(|a| a == "--key-file").unwrap();
    args[at + 1] = repo.join("deep/signing.key").to_str().unwrap().into();
    fx.run_marked(&args, &[]).refused("git working tree");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_key_env_name_that_is_not_a_variable_name_is_refused() {
    let fx = Fixture::new();
    let args = fx.key_env_args("A-B");
    fx.run_marked(&args, &[("A-B", &fx.key_hex())])
        .refused("not a variable name");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn the_builder_seam_is_refused_in_github_actions() {
    let fx = Fixture::new();
    let out = fx.run_marked(&fx.args(), &[("GITHUB_ACTIONS", "true")]);
    out.refused("test seam");
    nothing_built(&fx, &fx.mark());
}

// ---------------------------------------------------------------------------------------------
// The key's care and the failures
// ---------------------------------------------------------------------------------------------

#[test]
fn the_builder_never_sees_the_key_or_its_variable_and_the_key_folder_is_empty_meanwhile() {
    let fx = Fixture::new();
    let env_out = fx.path("builder-env");
    let keydir_out = fx.path("builder-keydir");
    let key = fx.key_hex();
    let args = fx.key_env_args("SOLITE_TEST_KEY");
    let out = fx.run_with(
        &args,
        &[
            ("SOLITE_TEST_KEY", &key),
            ("FAKE_ENV_OUT", env_out.to_str().unwrap()),
            ("FAKE_KEYDIR_OUT", keydir_out.to_str().unwrap()),
        ],
    );
    out.ok();
    let seen = fs::read_to_string(&env_out).unwrap();
    assert!(seen.contains("FAKE_ENV_OUT="), "the env was captured");
    assert!(!seen.contains("SOLITE_TEST_KEY"), "the variable's name");
    assert!(!holds(seen.as_bytes(), &key), "the key's text");
    assert_eq!(fs::read_to_string(&keydir_out).unwrap(), "dirs: 1\n");
    assert_eq!(
        fs::read_to_string(format!("{}.list", keydir_out.display())).unwrap(),
        "",
        "the key folder held a file while the builder ran"
    );
}

#[test]
fn a_failing_builder_exits_1_keeps_the_log_without_the_key_and_leaves_no_output() {
    let fx = Fixture::new();
    let out = fx.run_with(&fx.args(), &[("FAKE_FAIL", "1")]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(out.stderr.contains("the builder failed"), "{}", out.all());
    assert!(
        out.stderr.contains("the fake build broke"),
        "the log's tail"
    );
    assert!(!fx.out().exists());
    let logs = fx.leftovers("solite-release-log");
    assert_eq!(logs.len(), 1, "{logs:?}");
    let folder = fx.tmp().join(&logs[0]);
    assert!(
        out.stderr.contains(folder.to_str().unwrap()),
        "{}",
        out.stderr
    );
    let log = fs::read(folder.join("release.log")).unwrap();
    assert!(String::from_utf8_lossy(&log).contains("the fake build broke"));
    assert!(!holds(&log, &fx.key_hex()), "the log holds the key");
    assert!(!holds(out.all().as_bytes(), &fx.key_hex()));
    // The key folder and the work folder are gone.
    assert_eq!(fx.leftovers("solite-release-key"), Vec::<String>::new());
    assert_eq!(fx.leftovers("solite-release-work"), Vec::<String>::new());
}

#[test]
fn an_interrupt_during_the_build_removes_the_folders_and_the_builders_children() {
    let fx = Fixture::new();
    let child_file = fx.path("builder-child");
    let mut args = fx.args();
    args.insert(0, SCRIPT.to_string());
    // Python starts the script with SIGINT at its default, whatever the test runner inherited
    // (a shell cannot trap a signal that was ignored when it started), in a process group of its
    // own, so the test's SIGINT to that group is the terminal's Ctrl-C.
    let mut command = Command::new("python3");
    command
        .args([
            "-c",
            "import os, signal, sys\nsignal.signal(signal.SIGINT, signal.SIG_DFL)\nos.setpgrp()\nos.execvp('sh', ['sh'] + sys.argv[1:])",
        ])
        .args(&args)
        .env("SOLITE_HOST_BIN", env!("CARGO_BIN_EXE_solite-host"))
        .env("SOLITE_RELEASE_BUILDER", fx.builder())
        .env("SOLITE_RELEASE_TREE", fx.path("tree"))
        .env("SOLITE_RELEASE_TIME", TIME)
        .env("TMPDIR", fx.tmp())
        .env("FAKE_CHILD", &child_file)
        .env_remove("GITHUB_ACTIONS")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let started = Instant::now();
    // The builder's own child (`sleep 47`) is running once its pid is written.
    let grandchild = loop {
        if let Some(pid) = fs::read_to_string(&child_file)
            .ok()
            .and_then(|t| t.trim().parse::<u32>().ok())
        {
            break pid;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the builder never started"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert!(alive(grandchild));
    // Ctrl-C: SIGINT to the whole process group.
    assert!(
        Command::new("kill")
            .args(["-INT", "--", &format!("-{}", child.id())])
            .status()
            .unwrap()
            .success()
    );
    let waited = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(4),
            "the script did not stop at the signal"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(130));
    let mut waited = 0;
    while alive(grandchild) && waited < 60 {
        thread::sleep(Duration::from_millis(50));
        waited += 1;
    }
    assert!(!alive(grandchild), "the builder's child is still running");
    assert_eq!(fx.leftovers("solite-release-key"), Vec::<String>::new());
    assert_eq!(fx.leftovers("solite-release-work"), Vec::<String>::new());
    assert!(!fx.out().exists());
}

/// TERM to the script alone (not to its group): its builder's children stop, and the program that
/// launched it (a parent shell, as `quiet.sh` or a CI step) keeps running and reads its code.
#[test]
fn a_term_to_the_script_alone_stops_its_builder_and_spares_its_parent() {
    let fx = Fixture::new();
    let child_file = fx.path("builder-child");
    let pid_file = fx.path("script-pid");
    let rc_file = fx.path("script-rc");
    let mut args = fx.args();
    args.insert(0, SCRIPT.to_string());
    let mut command = Command::new("python3");
    command
        .args([
            "-c",
            "import os, sys\nos.setpgrp()\nos.execvp('sh', ['sh', '-c', 'sh \"$@\" & echo $! > \"$PID_FILE\"; wait $!; echo $? > \"$RC_FILE\"', 'parent'] + sys.argv[1:])",
        ])
        .args(&args)
        .env("SOLITE_HOST_BIN", env!("CARGO_BIN_EXE_solite-host"))
        .env("SOLITE_RELEASE_BUILDER", fx.builder())
        .env("SOLITE_RELEASE_TREE", fx.path("tree"))
        .env("SOLITE_RELEASE_TIME", TIME)
        .env("TMPDIR", fx.tmp())
        .env("FAKE_CHILD", &child_file)
        .env("PID_FILE", &pid_file)
        .env("RC_FILE", &rc_file)
        .env_remove("GITHUB_ACTIONS")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut parent = command.spawn().unwrap();
    let started = Instant::now();
    let grandchild = loop {
        if let Some(pid) = fs::read_to_string(&child_file)
            .ok()
            .and_then(|t| t.trim().parse::<u32>().ok())
        {
            break pid;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the builder never started"
        );
        thread::sleep(Duration::from_millis(50));
    };
    let script = fs::read_to_string(&pid_file).unwrap();
    assert!(
        Command::new("kill")
            .args(["-TERM", script.trim()])
            .status()
            .unwrap()
            .success()
    );
    let waited = Instant::now();
    let status = loop {
        if let Some(status) = parent.try_wait().unwrap() {
            break status;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(4),
            "the parent did not end"
        );
        thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "the parent was killed: {status:?}");
    assert_eq!(fs::read_to_string(&rc_file).unwrap().trim(), "143");
    let mut waited = 0;
    while alive(grandchild) && waited < 60 {
        thread::sleep(Duration::from_millis(50));
        waited += 1;
    }
    assert!(!alive(grandchild), "the builder's child is still running");
    assert!(!fx.out().exists());
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

// ---------------------------------------------------------------------------------------------
// The two steps: --build-only, then --prebuilt (R1.6)
// ---------------------------------------------------------------------------------------------

impl Fixture {
    /// The arguments of a build-only run: no key.
    fn build_only_args(&self) -> Vec<String> {
        [
            VERSION,
            "--build-only",
            "--public-key-file",
            self.public().to_str().unwrap(),
            "--recipe-key-file",
            self.recipe_public().to_str().unwrap(),
            "--out",
            self.out().to_str().unwrap(),
        ]
        .map(String::from)
        .to_vec()
    }

    /// A folder with the two files a build-only run writes; `app` is the application image.
    fn prebuilt(&self, app: &str) -> PathBuf {
        let dir = self.path("prebuilt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("app.bin"), app).unwrap();
        fs::write(dir.join("flash.bin"), "prebuilt flash image").unwrap();
        dir
    }

    fn run_prebuilt(&self, dir: &Path) -> Out {
        let mut args = self.args();
        args.extend(["--prebuilt".to_string(), dir.to_str().unwrap().to_string()]);
        self.run_marked(&args, &[])
    }
}

#[test]
fn build_only_writes_the_two_files_without_a_key_and_signs_nothing() {
    let fx = Fixture::new();
    // No host program at all: a build-only run does not probe or sign.
    let args = fx.build_only_args();
    let mut command = fx.command(&args.iter().map(String::as_str).collect::<Vec<_>>());
    command.env("SOLITE_HOST_BIN", "/nonexistent/solite-host");
    let out = Out::of(command.output().unwrap());
    out.ok();
    assert_eq!(names(&fx.out()), ["app.bin", "flash.bin"]);
    assert!(
        out.stdout
            .lines()
            .any(|l| l == "release: built, not signed (--build-only)"),
        "{}",
        out.stdout
    );
    let app = fs::read(fx.out().join("app.bin")).unwrap();
    assert!(holds(&app, &fx.public_hex()), "the image carries the key");
    assert!(
        holds(&app, &fx.recipe_public_hex()),
        "the image carries the recipe key"
    );
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

#[test]
fn build_only_refuses_a_key_option_or_prebuilt_and_builds_nothing() {
    let fx = Fixture::new();
    for extra in [
        vec![
            "--key-file",
            fx.key().to_str().unwrap().to_owned().leak() as &str,
        ],
        vec!["--key-env", "SOLITE_TEST_KEY"],
        vec![
            "--prebuilt",
            fx.path("prebuilt").to_str().unwrap().to_owned().leak() as &str,
        ],
    ] {
        let mut args = fx.build_only_args();
        args.extend(extra.iter().map(|a| a.to_string()));
        let out = fx.run_marked(&args, &[("SOLITE_TEST_KEY", &fx.key_hex())]);
        out.refused("--build-only");
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn build_only_keeps_the_checks_that_need_no_key() {
    let fx = Fixture::new();
    fx.write_tree("0.1.0", VERSION, VERSION);
    let out = fx.run_marked(&fx.build_only_args(), &[]);
    out.refused("Cargo.toml says version 0.1.0");
    nothing_built(&fx, &fx.mark());
    let fx = Fixture::new();
    fs::remove_file(fx.public()).unwrap();
    fx.run_marked(&fx.build_only_args(), &[])
        .refused("the public key file");
    nothing_built(&fx, &fx.mark());
    let fx = Fixture::new();
    fx.write_notes("# Release notes\n");
    fx.run_marked(&fx.build_only_args(), &[])
        .refused("no release notes for");
    nothing_built(&fx, &fx.mark());
    let fx = Fixture::new();
    fs::create_dir_all(fx.out()).unwrap();
    fs::write(fx.out().join("old"), "x").unwrap();
    fx.run_marked(&fx.build_only_args(), &[])
        .refused("is not empty");
    assert!(!fx.mark().exists());
}

#[test]
fn a_prebuilt_folder_is_signed_instead_of_building() {
    let fx = Fixture::new();
    let app = format!(
        "an image that carries {} {}",
        fx.public_hex(),
        fx.recipe_public_hex()
    );
    let dir = fx.prebuilt(&app);
    let out = fx.run_prebuilt(&dir);
    out.ok();
    assert!(!fx.mark().exists(), "the builder ran");
    assert_eq!(names(&fx.out()), five_files());
    let sup = fs::read(fx.out().join(format!("solite-{VERSION}.sup"))).unwrap();
    assert_eq!(&sup[128..], app.as_bytes());
    assert_eq!(
        fs::read(fx.out().join(format!("solite-{VERSION}-flash.bin"))).unwrap(),
        b"prebuilt flash image"
    );
    // The folder given is left as it was.
    assert_eq!(names(&dir), ["app.bin", "flash.bin"]);
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

#[test]
fn a_prebuilt_image_without_the_public_key_is_refused() {
    let fx = Fixture::new();
    let dir = fx.prebuilt("an image of another build");
    let out = fx.run_prebuilt(&dir);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stderr.contains(&format!(
            "the prebuilt image does not carry {}'s key",
            fx.public().display()
        )),
        "{}",
        out.all()
    );
    assert!(!fx.out().exists());
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

#[test]
fn a_prebuilt_folder_missing_a_file_or_not_there_is_refused() {
    let fx = Fixture::new();
    let dir = fx.prebuilt(&format!(
        "carries {} {}",
        fx.public_hex(),
        fx.recipe_public_hex()
    ));
    fs::remove_file(dir.join("flash.bin")).unwrap();
    fx.run_prebuilt(&dir).refused("has no flash.bin");
    nothing_built(&fx, &fx.mark());
    fs::remove_file(dir.join("app.bin")).unwrap();
    fx.run_prebuilt(&dir).refused("has no app.bin");
    nothing_built(&fx, &fx.mark());
    fx.run_prebuilt(&fx.path("nowhere"))
        .refused("does not exist");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn an_image_with_no_application_descriptor_is_refused_when_the_builder_is_not_the_seam() {
    let fx = Fixture::new();
    // The host program, but for a line saying the signed image has no descriptor (the real host
    // prints it for an image that is not a firmware).
    let wrapper = fx.path("host.sh");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\ncase \"$*\" in *--update-sign*) echo 'signed: no application descriptor' ;; esac\nexec {} \"$@\"\n",
            env!("CARGO_BIN_EXE_solite-host")
        ),
    )
    .unwrap();
    let dir = fx.prebuilt(&format!(
        "carries {} {}",
        fx.public_hex(),
        fx.recipe_public_hex()
    ));
    let mut args = fx.args();
    args.extend(["--prebuilt".to_string(), dir.to_str().unwrap().to_string()]);
    let mut command = fx.command(&args.iter().map(String::as_str).collect::<Vec<_>>());
    command
        .env("SOLITE_HOST_BIN", format!("{}", wrapper.display()))
        .env_remove("SOLITE_RELEASE_BUILDER");
    let wrapper_mode = Command::new("chmod")
        .arg("755")
        .arg(&wrapper)
        .status()
        .unwrap();
    assert!(wrapper_mode.success());
    let out = Out::of(command.output().unwrap());
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stderr
            .contains("the image has no application descriptor"),
        "{}",
        out.all()
    );
    assert!(!fx.out().exists());
}

#[test]
fn a_key_env_name_the_script_uses_itself_reads_the_environment_only() {
    let fx = Fixture::new();
    // `KEY`, `OUT` and `VERSION` are the script's own variables: the key is the environment's.
    for name in ["KEY", "OUT", "VERSION"] {
        let fx = Fixture::new();
        let args = fx.key_env_args(name);
        fx.run_with(&args, &[(name, &fx.key_hex())]).ok();
    }
    // Unset in the environment, whatever the script holds under that name: refused.
    let args = fx.key_env_args("VERSION");
    let out = fx.run_with(&args, &[]);
    out.refused("is empty");
}

// ---------------------------------------------------------------------------------------------
// A bridging release (spec 040, R1): the old key signs, the image carries the new one
// ---------------------------------------------------------------------------------------------
//
// In these tests the fixture's own key and public key (`signing.key`, `update-key.pub`) are the
// NEW pair, the one the image carries; `old.key` is the pair the boxes trust today.

impl Fixture {
    fn bridge_path(&self) -> PathBuf {
        self.path("tree/release/update-key.bridge")
    }

    /// Makes the old key and writes the bridge file for `version` naming `previous` (the old
    /// public key when `None`). Returns the old key's file and public key.
    fn bridge(&self, version: &str, previous: Option<&str>) -> (PathBuf, String) {
        let (file, public) = self.keygen("old.key");
        fs::create_dir_all(self.path("tree/release")).unwrap();
        fs::write(
            self.bridge_path(),
            format!(
                "# A bridging release.\n\nversion {version}\nprevious {}\n",
                previous.unwrap_or(&public)
            ),
        )
        .unwrap();
        (file, public)
    }

    /// The arguments of a bridge: the old key signs, the fixture's key is the next key.
    fn bridge_args(&self, old: &Path) -> Vec<String> {
        let mut args = self.args();
        let at = args.iter().position(|a| a == "--key-file").unwrap();
        args[at + 1] = old.to_str().unwrap().to_string();
        args.extend([
            "--next-key-file".into(),
            self.key().to_str().unwrap().into(),
        ]);
        args
    }

    fn verify(&self, sup: &Path, public: &str) -> bool {
        Command::new(env!("CARGO_BIN_EXE_solite-host"))
            .arg("--update-verify")
            .arg(sup)
            .args(["--public-key", public])
            .output()
            .unwrap()
            .status
            .success()
    }
}

#[test]
fn a_bridge_is_signed_with_the_old_key_carries_the_new_one_and_says_so() {
    let fx = Fixture::new();
    let (old, old_public) = fx.bridge(VERSION, None);
    let args_file = fx.dir_for_builder_args();
    let args = fx.bridge_args(&old);
    let out = fx.run_with(&args, &[("FAKE_ARGS", args_file.to_str().unwrap())]);
    out.ok();
    assert_eq!(names(&fx.out()), five_files());
    let pub8 = &fx.public_hex()[..8];
    assert!(
        out.stdout.lines().any(|l| l
            == format!(
                "release: key bridge: signed with {}, the image carries {pub8}",
                &old_public[..8]
            )),
        "{}",
        out.stdout
    );
    // The image is built with the new public key, as always.
    assert!(
        fs::read_to_string(&args_file)
            .unwrap()
            .starts_with(&format!("{VERSION} {} ", fx.public_hex()))
    );
    let sup = fx.out().join(format!("solite-{VERSION}.sup"));
    assert!(
        fx.verify(&sup, &old_public),
        "a box before the bridge accepts it"
    );
    assert!(
        !fx.verify(&sup, &fx.public_hex()),
        "a box that trusts the new key refuses the bridge"
    );
    // Neither private key is in a file or a line.
    for key in [
        fs::read_to_string(&old).unwrap().trim().to_string(),
        fx.key_hex(),
    ] {
        for name in names(&fx.out()) {
            assert!(
                !holds(&fs::read(fx.out().join(&name)).unwrap(), &key),
                "{name}"
            );
        }
        assert!(!holds(out.all().as_bytes(), &key));
    }
    assert_eq!(fx.leftovers("solite-release-"), Vec::<String>::new());
}

#[test]
fn a_bridge_reads_both_keys_from_the_environment_and_the_builder_sees_neither() {
    let fx = Fixture::new();
    let (old, _) = fx.bridge(VERSION, None);
    let old_hex = fs::read_to_string(&old).unwrap().trim().to_string();
    let new_hex = fx.key_hex();
    let mut args = fx.key_env_args("SOLITE_TEST_OLD");
    args.extend(["--next-key-env".into(), "SOLITE_TEST_NEW".into()]);
    let env_out = fx.path("builder-env");
    let out = fx.run_with(
        &args,
        &[
            ("SOLITE_TEST_OLD", &old_hex),
            ("SOLITE_TEST_NEW", &new_hex),
            ("FAKE_ENV_OUT", env_out.to_str().unwrap()),
        ],
    );
    out.ok();
    assert_eq!(names(&fx.out()), five_files());
    let seen = fs::read_to_string(&env_out).unwrap();
    assert!(seen.contains("FAKE_ENV_OUT="), "the env was captured");
    for text in ["SOLITE_TEST_OLD", "SOLITE_TEST_NEW", &old_hex, &new_hex] {
        assert!(!holds(seen.as_bytes(), text), "the builder saw {text}");
    }
    assert!(!holds(out.all().as_bytes(), &old_hex));
    assert!(!holds(out.all().as_bytes(), &new_hex));
    // The same variable for both keys is refused: the second would be empty.
    let fx = Fixture::new();
    let (old, _) = fx.bridge(VERSION, None);
    let mut args = fx.key_env_args("SOLITE_TEST_OLD");
    args.extend(["--next-key-env".into(), "SOLITE_TEST_OLD".into()]);
    let old_hex = fs::read_to_string(&old).unwrap().trim().to_string();
    fx.run_marked(&args, &[("SOLITE_TEST_OLD", &old_hex)])
        .refused("the next key is missing");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn the_signing_key_being_the_new_key_is_refused_nothing_written() {
    let fx = Fixture::new();
    fx.bridge(VERSION, None);
    // The reverse: the new key signs, the next key is the old one.
    let mut args = fx.args();
    let old = fx.path("keys/old.key");
    args.extend(["--next-key-file".into(), old.to_str().unwrap().into()]);
    let out = fx.run_marked(&args, &[]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stderr
            .contains("the signing key does not match the bridge's previous key"),
        "{}",
        out.all()
    );
    assert!(!fx.mark().exists() && !fx.out().exists());
}

#[test]
fn a_bridge_without_the_next_key_is_refused() {
    let fx = Fixture::new();
    let (old, _) = fx.bridge(VERSION, None);
    let mut args = fx.bridge_args(&old);
    args.truncate(args.len() - 2);
    fx.run_marked(&args, &[])
        .refused("a bridging release needs the next key (--next-key-file or --next-key-env)");
    nothing_built(&fx, &fx.mark());
    // An empty or malformed one too.
    let mut args = fx.bridge_args(&old);
    let at = args.len() - 1;
    let bad = fx.path("keys/bad.key");
    fs::write(&bad, "not a key").unwrap();
    args[at] = bad.to_str().unwrap().into();
    fx.run_marked(&args, &[])
        .refused("the next key is not 64 hexadecimal characters");
    nothing_built(&fx, &fx.mark());
    let mut args = fx.bridge_args(&old);
    args.truncate(args.len() - 2);
    args.extend(["--next-key-env".into(), "SOLITE_TEST_NEW".into()]);
    fx.run_marked(&args, &[])
        .refused("the next key is missing: SOLITE_TEST_NEW is empty");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_next_key_that_is_not_the_committed_one_is_refused_nothing_written() {
    let fx = Fixture::new();
    let (old, _) = fx.bridge(VERSION, None);
    // Swapped: the old key as the next key.
    let mut args = fx.bridge_args(&old);
    let at = args.len() - 1;
    args[at] = old.to_str().unwrap().into();
    // (the signing key and the next key are then the same: refused before the probe)
    fx.run_marked(&args, &[]).refused("are the same");
    // A third key.
    let (third, _) = fx.keygen("third.key");
    args[at] = third.to_str().unwrap().into();
    let out = fx.run_marked(&args, &[]);
    assert_eq!(out.code, 1, "{}", out.all());
    assert!(
        out.stderr.contains("the next key does not match"),
        "{}",
        out.all()
    );
    assert!(!fx.mark().exists() && !fx.out().exists());
    assert!(!holds(out.all().as_bytes(), &fx.key_hex()));
}

#[test]
fn a_bridge_for_another_version_is_refused() {
    let fx = Fixture::new();
    let (old, _) = fx.bridge("0.1.0", None);
    fx.run_marked(&fx.bridge_args(&old), &[]).refused(&format!(
        "release/update-key.bridge is for 0.1.0, not {VERSION}: a bridge is one release; remove the file after it (docs/release.md)"
    ));
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_previous_that_is_the_committed_key_or_not_hex_is_refused() {
    let fx = Fixture::new();
    let (old, _) = fx.bridge(VERSION, Some(&Fixture::new().public_hex()));
    // `previous` equal to the public key file the script was given.
    fs::write(
        fx.bridge_path(),
        format!("version {VERSION}\nprevious {}\n", fx.public_hex()),
    )
    .unwrap();
    fx.run_marked(&fx.bridge_args(&old), &[])
        .refused("previous is the current public key: nothing to rotate");
    nothing_built(&fx, &fx.mark());
    for bad in ["abcd", &"g".repeat(64), &"a".repeat(65)] {
        fs::write(
            fx.bridge_path(),
            format!("version {VERSION}\nprevious {bad}\n"),
        )
        .unwrap();
        fx.run_marked(&fx.bridge_args(&old), &[])
            .refused("release/update-key.bridge: previous is not 64 hex characters");
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn a_bridge_file_with_a_bad_form_is_refused_with_its_reason() {
    let fx = Fixture::new();
    let (old, old_public) = fx.bridge(VERSION, None);
    for (text, reason) in [
        (
            format!("version {VERSION}\nprevious {old_public}\nnext {old_public}\n"),
            "unknown line: next",
        ),
        (
            format!("version {VERSION}\nversion {VERSION}\nprevious {old_public}\n"),
            "version given twice",
        ),
        (
            format!("version {VERSION}\nprevious {old_public}\nprevious {old_public}\n"),
            "previous given twice",
        ),
        (format!("previous {old_public}\n"), "version is missing"),
        (format!("version {VERSION}\n"), "previous is missing"),
        (
            format!("version {VERSION} extra\nprevious {old_public}\n"),
            "version needs one value",
        ),
    ] {
        fs::write(fx.bridge_path(), text).unwrap();
        fx.run_marked(&fx.bridge_args(&old), &[])
            .refused(&format!("release/update-key.bridge: {reason}"));
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn comments_blank_lines_crlf_and_extra_spaces_in_the_bridge_file_are_accepted() {
    let fx = Fixture::new();
    let (old, old_public) = fx.bridge(VERSION, None);
    fs::write(
        fx.bridge_path(),
        format!("# one\r\n\r\n  version   {VERSION}  \r\n\t# two\r\nprevious\t{old_public}\r\n"),
    )
    .unwrap();
    fx.run_with(&fx.bridge_args(&old), &[]).ok();
    assert_eq!(names(&fx.out()), five_files());
}

#[test]
fn next_key_options_without_a_bridge_file_are_refused() {
    let fx = Fixture::new();
    for extra in [
        vec!["--next-key-file", "/nonexistent/next.key"],
        vec!["--next-key-env", "SOLITE_TEST_NEW"],
    ] {
        let mut args = fx.args();
        args.extend(extra.iter().map(|a| a.to_string()));
        fx.run_marked(&args, &[("SOLITE_TEST_NEW", &fx.key_hex())])
            .refused("--next-key-* is for a bridging release only (release/update-key.bridge)");
        nothing_built(&fx, &fx.mark());
    }
}

#[test]
fn a_bridge_with_build_only_builds_without_a_key_and_checks_its_version() {
    let fx = Fixture::new();
    fx.bridge(VERSION, None);
    let out = fx.run_with(&fx.build_only_args(), &[]);
    out.ok();
    assert_eq!(names(&fx.out()), ["app.bin", "flash.bin"]);
    assert!(holds(
        &fs::read(fx.out().join("app.bin")).unwrap(),
        &fx.public_hex()
    ));
    // A wrong version, or a previous equal to the key, is refused before the build.
    let fx = Fixture::new();
    fx.bridge("0.1.0", None);
    fx.run_marked(&fx.build_only_args(), &[])
        .refused("is for 0.1.0");
    nothing_built(&fx, &fx.mark());
    // A next key with --build-only is refused like a key.
    let fx = Fixture::new();
    fx.bridge(VERSION, None);
    let mut args = fx.build_only_args();
    args.extend(["--next-key-file".into(), fx.key().to_str().unwrap().into()]);
    fx.run_marked(&args, &[])
        .refused("--build-only takes no key: the signing is the next step (--prebuilt)");
    nothing_built(&fx, &fx.mark());
}

#[test]
fn a_bridge_is_signed_from_a_prebuilt_folder_and_the_public_key_file_is_the_one_given() {
    let fx = Fixture::new();
    let (old, old_public) = fx.bridge(VERSION, None);
    let dir = fx.prebuilt(&format!(
        "image with {} {}",
        fx.public_hex(),
        fx.recipe_public_hex()
    ));
    let mut args = fx.bridge_args(&old);
    args.extend(["--prebuilt".into(), dir.to_str().unwrap().into()]);
    fx.run_with(&args, &[]).ok();
    let sup = fx.out().join(format!("solite-{VERSION}.sup"));
    assert!(fx.verify(&sup, &old_public));
    assert!(!fx.verify(&sup, &fx.public_hex()));
}

#[test]
fn the_bridge_file_option_overrides_the_trees_file() {
    let fx = Fixture::new();
    let (old, old_public) = fx.bridge(VERSION, None);
    // The tree's file is removed, another one given by --bridge-file.
    let elsewhere = fx.path("elsewhere.bridge");
    fs::rename(fx.bridge_path(), &elsewhere).unwrap();
    let mut args = fx.bridge_args(&old);
    args.extend(["--bridge-file".into(), elsewhere.to_str().unwrap().into()]);
    let out = fx.run_with(&args, &[]);
    out.ok();
    assert!(
        out.stdout
            .contains(&format!("signed with {}", &old_public[..8]))
    );
}
