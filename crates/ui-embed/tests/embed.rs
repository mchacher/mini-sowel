//! The interface packed for the flash (spec 022 R7).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::read::GzDecoder;
use solite_ui_embed::{MAX_FILE_BYTES, MAX_UI_FILES, collect, generate};

/// A new empty folder under the system's temporary folder, unique for this test run.
fn folder(label: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "solite-ui-embed-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

/// A built interface: `index.html` and these files of `assets/`.
fn dist(label: &str, assets: &[(&str, usize)]) -> PathBuf {
    let dist = folder(label);
    fs::write(
        dist.join("index.html"),
        "<!doctype html><title>Solite</title>".repeat(20),
    )
    .unwrap();
    fs::create_dir_all(dist.join("assets")).unwrap();
    for (name, size) in assets {
        let body: Vec<u8> = (0..*size).map(|i| b"abcdefgh"[i % 8]).collect();
        fs::write(dist.join("assets").join(name), body).unwrap();
    }
    dist
}

fn nominal(label: &str) -> PathBuf {
    dist(
        label,
        &[
            ("index-4f2a.js", 5000),
            ("index-9c1b.css", 2000),
            ("font-77aa.woff2", 900),
        ],
    )
}

fn gunzip(path: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_end(&mut out)
        .unwrap();
    out
}

#[test]
fn a_built_interface_is_packed_sorted_with_its_marker() {
    let dist = nominal("nominal");
    let files = collect(&dist).unwrap();
    assert_eq!(files.len(), 4);
    let out = folder("nominal-out");
    let summary = generate(&files, &out).unwrap();
    assert_eq!(summary.files, 4);
    let source = fs::read_to_string(out.join("ui_files.rs")).unwrap();
    // Sorted by path, each with an `include_bytes!` of a gzip file that exists and unpacks to the
    // original.
    let mut paths = Vec::new();
    for file in &files {
        let at = source
            .find(&format!("({:?}, include_bytes!(", file.path))
            .expect("listed");
        paths.push((at, &file.path));
        let flat = out.join(format!("{}.gz", file.path.replace('/', "__")));
        assert!(source.contains(&format!(
            "{:?}",
            flat.canonicalize().unwrap().display().to_string()
        )));
        assert_eq!(gunzip(&flat), file.original, "{}", file.path);
    }
    let mut by_position = paths.clone();
    by_position.sort();
    let order: Vec<&String> = by_position.iter().map(|(_, p)| *p).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted, "the table is sorted by path");
    assert_eq!(order[0], "assets/font-77aa.woff2");
    assert_eq!(order[3], "index.html");
    let kb = summary.gzip_bytes / 1024;
    assert_eq!(summary.marker(), format!("solite-ui 4 files, {kb} KB"));
    assert!(source.contains(&format!("\"solite-ui 4 files, {kb} KB\"")));
}

#[test]
fn a_missing_or_empty_folder_says_the_interface_is_not_built() {
    let expected = "interface: ui/dist not built: cd ui && npm ci && npm run build";
    assert_eq!(collect(&folder("empty")).unwrap_err(), expected);
    assert_eq!(
        collect(&folder("empty").join("nothing")).unwrap_err(),
        expected
    );
}

#[test]
fn one_file_past_the_bound_is_refused_and_naming_it() {
    // `index.html` and 31 assets fill the bound; 33 assets are over it.
    let at_bound: Vec<(String, usize)> = (0..MAX_UI_FILES - 1)
        .map(|i| (format!("f{i}.js"), 10))
        .collect();
    let names: Vec<(&str, usize)> = at_bound.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    assert_eq!(
        collect(&dist("at-bound", &names)).unwrap().len(),
        MAX_UI_FILES
    );
    let over: Vec<(String, usize)> = (0..33).map(|i| (format!("f{i}.js"), 10)).collect();
    let names: Vec<(&str, usize)> = over.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    let error = collect(&dist("over", &names)).unwrap_err();
    assert!(error.contains("MAX_UI_FILES (32)"), "{error}");
}

#[test]
fn a_file_of_one_megabyte_plus_one_byte_is_refused_naming_it() {
    assert!(collect(&dist("at-size", &[("exact.js", MAX_FILE_BYTES)])).is_ok());
    let error = collect(&dist("too-big", &[("big.js", MAX_FILE_BYTES + 1)])).unwrap_err();
    assert!(
        error.contains("assets/big.js") && error.contains("1 MB"),
        "{error}"
    );
}

#[test]
fn a_name_the_core_would_never_serve_is_refused_naming_the_file() {
    let sub = dist("sub", &[("a.js", 10)]);
    fs::create_dir(sub.join("assets").join("nested")).unwrap();
    assert!(collect(&sub).unwrap_err().contains("assets/nested"));
    for bad in [".hidden.js", "100%.js", "a..b.js"] {
        let error = collect(&dist("bad-name", &[(bad, 10)])).unwrap_err();
        assert!(error.contains(&format!("assets/{bad}")), "{error}");
    }
}

#[test]
fn two_runs_on_the_same_folder_give_the_same_bytes() {
    let dist = nominal("repeat");
    let run = |label: &str| {
        let out = folder(label);
        generate(&collect(&dist).unwrap(), &out).unwrap();
        out
    };
    let (first, second) = (run("repeat-a"), run("repeat-b"));
    let mut names: Vec<_> = fs::read_dir(&first)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    assert!(names.len() >= 5);
    for name in names {
        let (a, b) = (
            fs::read(first.join(&name)).unwrap(),
            fs::read(second.join(&name)).unwrap(),
        );
        if name == "ui_files.rs" {
            // The listing names the folder it was written to: compare it without it.
            let strip = |bytes: Vec<u8>, folder: &Path| {
                String::from_utf8(bytes).unwrap().replace(
                    &folder.canonicalize().unwrap().display().to_string(),
                    "<out>",
                )
            };
            assert_eq!(strip(a, &first), strip(b, &second));
        } else {
            assert_eq!(a, b, "{name:?}");
            if name.to_string_lossy().ends_with(".gz") {
                // gzip's MTIME field (bytes 4..8) is zero: a build time there would change
                // the image at every build, and two runs in the same second would not show it.
                assert_eq!(&a[4..8], &[0, 0, 0, 0], "{name:?} carries a time");
            }
        }
    }
}

#[test]
fn the_real_interface_packs_under_170_kb_of_gzip() {
    let real = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist");
    if !real.join("index.html").is_file() {
        eprintln!("ui/dist is not built: skipped");
        return;
    }
    let out = folder("real");
    let summary = generate(&collect(&real).unwrap(), &out).unwrap();
    assert!(
        summary.gzip_bytes < 170 * 1024,
        "{} bytes",
        summary.gzip_bytes
    );
}
