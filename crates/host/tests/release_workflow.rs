//! `.github/workflows/release.yml` and its composite action (spec 038, R3): plain text checks, since
//! a workflow cannot be run here. They guard the rules that keep the signing key safe: where the
//! secret is read, which job may write, when a job runs, and that no cache is saved.

use std::fs;

const WORKFLOW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.github/workflows/release.yml"
);
const ACTION: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.github/actions/firmware-toolchain/action.yml"
);
const CI: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.github/workflows/ci.yml"
);

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The lines of one top-level job (from `  <name>:` up to the next job), comments left out.
fn job(text: &str, name: &str) -> Vec<String> {
    let header = format!("  {name}:");
    let mut inside = false;
    let mut lines = Vec::new();
    let mut in_jobs = false;
    for line in text.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        let is_job_header = line.starts_with("  ")
            && !line.starts_with("   ")
            && !line.trim_start().starts_with('#')
            && line.trim_end().ends_with(':');
        if is_job_header {
            inside = line == header;
            continue;
        }
        if inside && !line.trim_start().starts_with('#') {
            lines.push(line.to_string());
        }
    }
    assert!(!lines.is_empty(), "job {name} not found");
    lines
}

fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

const TAG_IF: &str = "if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')";

#[test]
fn contents_write_is_only_in_the_publish_job() {
    let text = read(WORKFLOW);
    let writers: Vec<&str> = code_lines(&text)
        .into_iter()
        .filter(|l| l.contains("contents: write"))
        .collect();
    assert_eq!(
        writers.len(),
        1,
        "one `contents: write` expected: {writers:?}"
    );
    assert!(
        job(&text, "publish")
            .iter()
            .any(|l| l.contains("contents: write"))
    );
    // The workflow's own permissions are read-only.
    assert!(text.contains("\npermissions:\n  contents: read\n"));
}

#[test]
fn the_secret_is_read_by_two_steps_of_the_sign_job_only() {
    let text = read(WORKFLOW);
    let all: Vec<&str> = code_lines(&text)
        .into_iter()
        .filter(|l| l.contains("SOLITE_UPDATE_KEY"))
        .collect();
    for l in &all {
        assert!(
            !l.contains("secrets.")
                || l.trim() == "SOLITE_UPDATE_KEY: ${{ secrets.SOLITE_UPDATE_KEY }}",
            "{l}"
        );
    }
    // `secrets.` appears twice in the whole workflow, both in the sign job.
    let secrets: Vec<&str> = code_lines(&text)
        .into_iter()
        .filter(|l| l.contains("secrets."))
        .collect();
    assert_eq!(secrets.len(), 2, "{secrets:?}");
    let sign = job(&text, "sign");
    assert_eq!(
        sign.iter()
            .filter(|l| l.contains("secrets.SOLITE_UPDATE_KEY"))
            .count(),
        2
    );
    // The job that runs third-party build code (build.rs, cmake, pip, npm) never names the secret.
    for other in ["firmware", "dry-run", "publish"] {
        assert!(
            job(&text, other)
                .iter()
                .all(|l| !l.contains("SOLITE_UPDATE_KEY")),
            "{other} must not name the secret"
        );
    }
    // The two uses are the refusal step's `env:` and the release script's `env:`.
    assert!(
        sign.iter()
            .any(|l| l.contains("--key-env SOLITE_UPDATE_KEY"))
    );
    // The key is never passed on a command line.
    assert!(!text.contains("--key $"));
}

#[test]
fn the_sign_job_builds_nothing_but_the_host_program() {
    let text = read(WORKFLOW);
    let body = job(&text, "sign").join("\n");
    assert!(!body.contains("firmware-toolchain"));
    for tool in [
        "npm",
        "espup",
        "export-esp.sh",
        "setup-node",
        "--build-only",
    ] {
        assert!(!body.contains(tool), "the sign job holds the key: {tool}");
    }
    assert!(body.contains("cargo build -p solite-host --release"));
    assert!(body.contains("--prebuilt \"$RUNNER_TEMP/firmware\""));
    assert!(body.contains("name: firmware"));
}

#[test]
fn the_firmware_job_has_no_environment_no_secret_and_reads_only() {
    let text = read(WORKFLOW);
    let firmware = job(&text, "firmware");
    assert!(firmware.iter().any(|l| l.trim() == TAG_IF));
    assert!(firmware.iter().any(|l| l.trim() == "contents: read"));
    assert!(
        firmware
            .iter()
            .all(|l| !l.contains("environment:") && !l.contains("secrets."))
    );
    let body = firmware.join("\n");
    assert!(body.contains("uses: ./.github/actions/firmware-toolchain"));
    assert!(body.contains(
        "sh scripts/release.sh \"${GITHUB_REF_NAME#v}\" --build-only --out \"$RUNNER_TEMP/build\""
    ));
    assert!(!body.contains("--key"));
    assert!(body.contains("name: firmware"));
}

#[test]
fn the_sign_job_runs_in_the_release_environment_on_a_tag_only() {
    let text = read(WORKFLOW);
    let sign = job(&text, "sign");
    assert!(sign.iter().any(|l| l.trim() == "environment: release"));
    assert!(sign.iter().any(|l| l.trim() == TAG_IF));
    assert!(sign.iter().any(|l| l.trim() == "needs: firmware"));
    assert!(sign.iter().any(|l| l.trim() == "contents: read"));
    let publish = job(&text, "publish");
    assert!(publish.iter().any(|l| l.trim() == TAG_IF));
    assert!(publish.iter().any(|l| l.trim() == "needs: sign"));
    assert!(publish.iter().all(|l| !l.contains("environment:")));
}

#[test]
fn the_dry_run_is_manual_with_no_environment_and_no_secret() {
    let text = read(WORKFLOW);
    let dry = job(&text, "dry-run");
    assert!(
        dry.iter()
            .any(|l| l.trim() == "if: github.event_name == 'workflow_dispatch'")
    );
    assert!(
        dry.iter()
            .all(|l| !l.contains("environment:") && !l.contains("secrets."))
    );
    assert!(
        dry.iter()
            .all(|l| !l.contains("gh release") && !l.contains("contents: write"))
    );
    assert!(dry.iter().any(|l| l.contains("--update-keygen")));
    // Only the manifest and the checksums are kept, under a name with the commit.
    assert!(
        dry.iter()
            .any(|l| l.contains("name: dry-run-${{ github.sha }}"))
    );
    assert!(dry.iter().any(|l| l.contains("dry-release/manifest.json")));
    assert!(dry.iter().any(|l| l.contains("dry-release/SHA256SUMS")));
    assert!(
        dry.iter()
            .all(|l| !l.contains(".sup") && !l.contains("flash.bin"))
    );
}

#[test]
fn triggers_and_concurrency() {
    let text = read(WORKFLOW);
    assert!(text.contains("  push:\n    tags: [\"v*\"]"));
    assert!(text.contains("  workflow_dispatch:"));
    // One group per kind of run: a dry run never cancels a pending tag run.
    assert!(text.contains(
        "concurrency:\n  group: release-${{ github.event_name }}\n  cancel-in-progress: false"
    ));
}

#[test]
fn publishing_is_refused_without_the_secret_the_key_or_main() {
    let text = read(WORKFLOW);
    let build = job(&text, "sign");
    let body = build.join("\n");
    assert!(body.contains(
        "::error::the secret SOLITE_UPDATE_KEY is not set in the environment release: an unsigned release is never published (see docs/release.md)"
    ));
    assert!(body.contains("[ -z \"$SOLITE_UPDATE_KEY\" ]"));
    // The variable is gone from the refusal step's shell right after the emptiness test.
    let test_at = body.find("[ -z \"$SOLITE_UPDATE_KEY\" ]").unwrap();
    let unset_at = body.find("unset SOLITE_UPDATE_KEY").unwrap();
    assert!(test_at < unset_at);
    assert!(unset_at < body.find("update-key.pub").unwrap());
    assert!(body.contains("release/update-key.pub"));
    assert!(body.contains("git merge-base --is-ancestor \"$GITHUB_SHA\" origin/main"));
    assert!(build.iter().any(|l| l.contains("fetch-depth: 0")));
    // The refusals come before the build.
    let refusal = body.find("merge-base").unwrap();
    assert!(refusal < body.find("rustup").unwrap());
    assert!(refusal < body.find("cargo build").unwrap());
    assert!(body.contains("--out \"$RUNNER_TEMP/release\""));
    assert!(body.contains("name: release"));
}

#[test]
fn no_job_saves_a_cache_through_setup_node() {
    // `cache: npm` makes setup-node save ~/.npm in its post step on a miss.
    for (name, text) in [
        ("release.yml", read(WORKFLOW)),
        ("action.yml", read(ACTION)),
    ] {
        for l in code_lines(&text) {
            assert!(
                !l.trim_start().starts_with("cache:"),
                "{name}: `{l}` saves a cache"
            );
        }
    }
    // No npm cache is restored either: nothing of a previous run's ~/.npm.
    assert!(!read(ACTION).contains("~/.npm"));
}

#[test]
fn the_firmware_job_refuses_a_tag_off_main_before_its_build() {
    let text = read(WORKFLOW);
    let start = text.find("\n  firmware:").unwrap();
    let end = text.find("\n  sign:").unwrap();
    let body = &text[start..end];
    let refusal = body.find("merge-base --is-ancestor").unwrap();
    assert!(body.contains("release/update-key.pub"));
    assert!(refusal < body.find("firmware-toolchain").unwrap());
    assert!(body.contains("fetch-depth: 0"));
    assert!(!body.contains("SOLITE_UPDATE_KEY"));
}

#[test]
fn the_toolchain_keys_and_pins_are_ci_ymls() {
    let action = read(ACTION);
    let ci = read(CI);
    let value = |text: &str, prefix: &str| -> String {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(prefix).map(|v| v.trim().to_string()))
            .unwrap_or_else(|| panic!("no `{prefix}`"))
    };
    // espup's version: the action's default and ci.yml's env.
    assert_eq!(
        value(&action, "default:"),
        value(&ci, "ESPUP_VERSION:"),
        "espup version"
    );
    // The Xtensa toolchain's key: the same text, the version variable aside.
    let key = |text: &str| {
        value(text, "key: esp-toolchain-")
            .replace("${{ inputs.espup-version }}", "<espup>")
            .replace("${{ env.ESPUP_VERSION }}", "<espup>")
    };
    assert_eq!(key(&action), key(&ci), "esp-toolchain key");
    // The rust-cache action: the same commit in ci.yml (every use) and in the action.
    let pin = "Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2.9.3";
    for (name, text) in [
        ("action.yml", &action),
        ("ci.yml", &ci),
        ("release.yml", &read(WORKFLOW)),
    ] {
        for l in text.lines().filter(|l| l.contains("Swatinem/rust-cache@")) {
            assert!(l.contains(pin), "{name}: {l}");
        }
    }
    // Both workspaces have their shared key on both sides.
    for key in ["shared-key: firmware", "shared-key: rust"] {
        assert!(action.contains(key), "{key}");
        assert!(ci.contains(key), "{key}");
    }
}

#[test]
fn the_publish_job_lists_the_four_assets_and_never_replaces() {
    let text = read(WORKFLOW);
    let body = job(&text, "publish").join("\n");
    for asset in [
        "solite-$version.sup",
        "solite-$version-flash.bin",
        "SHA256SUMS",
        "manifest.json",
    ] {
        assert!(body.contains(asset), "{asset}");
    }
    assert!(body.contains("gh release create \"$GITHUB_REF_NAME\""));
    assert!(body.contains("--verify-tag"));
    assert!(body.contains("--notes-file notes.md"));
    assert!(body.contains("GH_TOKEN: ${{ github.token }}"));
    // Neither a draft nor a pre-release, never an edit of an existing release.
    assert!(!body.contains("--draft") && !body.contains("--prerelease"));
    assert!(!body.contains("--clobber") && !body.contains("gh release edit"));
    assert!(!body.contains("secrets."));
}

#[test]
fn no_cache_is_saved_and_third_party_actions_are_pinned() {
    let workflow = read(WORKFLOW);
    let action = read(ACTION);
    for (name, text) in [("release.yml", &workflow), ("action.yml", &action)] {
        let code = code_lines(text).join("\n");
        assert!(
            !code.contains("actions/cache@"),
            "{name}: a cache that saves"
        );
        assert!(!code.contains("actions/cache/save"), "{name}");
        for l in code.lines().filter(|l| l.contains("uses:")) {
            let spec = l.split("uses:").nth(1).unwrap().trim();
            if spec.starts_with("./") || spec.starts_with("actions/") {
                continue;
            }
            let at = spec.split('@').nth(1).unwrap_or("");
            let sha = at.split_whitespace().next().unwrap_or("");
            assert!(
                sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
                "{name}: {spec} is not pinned by a commit SHA"
            );
            assert!(spec.contains("# v"), "{name}: {spec} lacks its tag comment");
        }
    }
    let rust_caches = action.matches("Swatinem/rust-cache@").count();
    assert_eq!(action.matches("save-if: \"false\"").count(), rust_caches);
    assert!(action.contains("actions/cache/restore@v4"));
    assert!(action.contains("GITHUB_TOKEN: ${{ github.token }}"));
    assert!(!code_lines(&action).join("\n").contains("secrets."));
    // The rust-cache of the sign job is restore-only too.
    let sign = job(&workflow, "sign").join("\n");
    assert_eq!(
        sign.matches("Swatinem/rust-cache@").count(),
        sign.matches("save-if: \"false\"").count()
    );
}

#[test]
fn the_build_jobs_use_the_composite_action_and_the_release_script() {
    let text = read(WORKFLOW);
    for name in ["firmware", "dry-run"] {
        let body = job(&text, name).join("\n");
        assert!(
            body.contains("uses: ./.github/actions/firmware-toolchain"),
            "{name}"
        );
        assert!(body.contains("sh scripts/release.sh"), "{name}");
    }
    let dry = job(&text, "dry-run").join("\n");
    assert!(dry.contains("cargo build -p solite-host --release"));
    assert!(
        job(&text, "publish")
            .iter()
            .all(|l| !l.contains("release.sh"))
    );
}
