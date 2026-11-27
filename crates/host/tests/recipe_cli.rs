//! `--recipe-sign` and `--recipe-verify` (spec 042, R1.6): a script signed with a key made by
//! `--update-keygen` is accepted by the core's checks and by the verifier (exit 0); a changed file
//! or a file signed with another key is refused with its reason (exit 1); a script with no `x.y.z`
//! version, a missing key and an existing output are refused (exit 2). Keys are made here, never
//! read from the repository.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use solite_core::recipes::install::file;
use solite_core::update::{Version, parse_public_key};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "solite-recipe-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn host(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A key file made by `--update-keygen`, and the public key it printed.
fn key(dir: &Path, name: &str) -> (PathBuf, String) {
    let file = dir.join(name);
    let output = host(&["--update-keygen".as_ref(), file.as_os_str()]);
    assert!(output.status.success(), "{output:?}");
    let public = stdout(&output)
        .lines()
        .find_map(|l| l.strip_prefix("public key: ").map(str::to_string))
        .expect("the public key line");
    (file, public)
}

fn script(id: &str, version_line: &str) -> String {
    format!(
        r#"recipe({{
  api: 1, id: "{id}", name: "{id}", description: "A test recipe", {version_line}
  slots: {{ timeout: {{ type: "duration", defaultValue: "10m" }} }},
  start: function (api) {{ api.log("started"); }},
}});"#
    )
}

fn sign(script: &Path, key: &Path, out: &Path) -> Output {
    host(&[
        "--recipe-sign".as_ref(),
        script.as_os_str(),
        "--key".as_ref(),
        key.as_os_str(),
        "--out".as_ref(),
        out.as_os_str(),
    ])
}

fn verify(file: &Path, public: &str) -> Output {
    host(&[
        "--recipe-verify".as_ref(),
        file.as_os_str(),
        "--public-key".as_ref(),
        public.as_ref(),
    ])
}

#[test]
fn a_signed_script_is_accepted_by_the_core_and_by_the_verifier() {
    let dir = temp();
    let (key_file, public) = key(&dir, "recipe.key");
    let source = dir.join("a.js");
    std::fs::write(&source, script("a", r#"version: "1.2.3","#)).unwrap();
    let out = dir.join("a.srs");
    let signed = sign(&source, &key_file, &out);
    assert!(signed.status.success(), "{signed:?}");
    assert!(stdout(&signed).contains("a 1.2.3"), "{}", stdout(&signed));
    let bytes = std::fs::read(&out).unwrap();
    let checked = file::check(&bytes, &parse_public_key(&public).unwrap()).unwrap();
    assert_eq!(checked.version, Version::new(1, 2, 3));
    assert_eq!(checked.source, std::fs::read_to_string(&source).unwrap());
    let verified = verify(&out, &public);
    assert_eq!(verified.status.code(), Some(0), "{verified:?}");
    assert!(stdout(&verified).contains("verified:"));
}

#[test]
fn a_changed_file_or_another_key_is_refused_with_exit_1_and_its_reason() {
    let dir = temp();
    let (key_file, public) = key(&dir, "recipe.key");
    let (_, other) = key(&dir, "other.key");
    let source = dir.join("a.js");
    std::fs::write(&source, script("a", r#"version: "1.0.0","#)).unwrap();
    let out = dir.join("a.srs");
    assert!(sign(&source, &key_file, &out).status.success());
    // A byte of the source changed.
    let mut bytes = std::fs::read(&out).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    let changed = dir.join("changed.srs");
    std::fs::write(&changed, &bytes).unwrap();
    let refused = verify(&changed, &public);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert!(
        stderr(&refused).contains("SHA-256 differs"),
        "{}",
        stderr(&refused)
    );
    // The right file, the wrong key.
    let wrong = verify(&out, &other);
    assert_eq!(wrong.status.code(), Some(1), "{wrong:?}");
    assert!(
        stderr(&wrong).contains("Invalid signature"),
        "{}",
        stderr(&wrong)
    );
}

#[test]
fn a_script_without_an_x_y_z_version_is_not_signed_exit_2() {
    let dir = temp();
    let (key_file, _) = key(&dir, "recipe.key");
    for (name, line) in [("none", ""), ("short", r#"version: "1.0","#)] {
        let source = dir.join(format!("{name}.js"));
        std::fs::write(&source, script("a", line)).unwrap();
        let out = dir.join(format!("{name}.srs"));
        let refused = sign(&source, &key_file, &out);
        assert_eq!(refused.status.code(), Some(2), "{name}: {refused:?}");
        assert!(
            stderr(&refused).contains("version must be x.y.z"),
            "{}",
            stderr(&refused)
        );
        assert!(!out.exists());
    }
}

#[test]
fn an_existing_output_a_missing_key_and_a_build_without_the_recipe_key_are_exit_2() {
    let dir = temp();
    let (key_file, public) = key(&dir, "recipe.key");
    let source = dir.join("a.js");
    std::fs::write(&source, script("a", r#"version: "1.0.0","#)).unwrap();
    let out = dir.join("a.srs");
    assert!(sign(&source, &key_file, &out).status.success());
    let again = sign(&source, &key_file, &out);
    assert_eq!(again.status.code(), Some(2), "{again:?}");
    assert!(stderr(&again).contains("nothing is overwritten"));
    let missing = sign(&source, &dir.join("nokey"), &dir.join("b.srs"));
    assert_eq!(missing.status.code(), Some(2), "{missing:?}");
    // No key given and none built in (this test binary's host has none).
    let none = host(&["--recipe-verify".as_ref(), out.as_os_str()]);
    assert_eq!(none.status.code(), Some(2), "{none:?}");
    assert!(stderr(&none).contains("no recipe key"), "{}", stderr(&none));
    let _ = public;
}

#[test]
fn the_recipe_tools_are_a_task_of_their_own() {
    let dir = temp();
    let (key_file, _) = key(&dir, "recipe.key");
    let output = host(&[
        "--recipe-sign".as_ref(),
        "a.js".as_ref(),
        "--key".as_ref(),
        key_file.as_os_str(),
        "--serve".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr(&output).contains("--recipe-sign"));
}
