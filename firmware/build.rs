//! The ESP-IDF build environment, and for the radio image (feature `radio`, spec 023 R7) the
//! interface packed from `../ui/dist` (or `SOLITE_UI_DIST`) into `OUT_DIR/ui_files.rs` by
//! `crates/ui-embed`. The other images embed nothing and build without `ui/dist`.

use std::path::{Path, PathBuf};

fn embed_interface() -> Result<(), String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let out_dir = std::env::var("OUT_DIR").map_err(|e| e.to_string())?;
    let dist: PathBuf = solite_ui_embed::dist_dir(&Path::new(&manifest).join("../ui/dist"));
    println!(
        "cargo:rerun-if-env-changed={}",
        solite_ui_embed::DIST_VARIABLE
    );
    println!("cargo:rerun-if-changed={}", dist.display());
    println!(
        "cargo:rerun-if-changed={}",
        dist.join("index.html").display()
    );
    println!("cargo:rerun-if-changed={}", dist.join("assets").display());
    let files = solite_ui_embed::collect(&dist)?;
    solite_ui_embed::generate(&files, Path::new(&out_dir))?;
    Ok(())
}

fn main() {
    embuild::espidf::sysenv::output();
    if std::env::var_os("CARGO_FEATURE_RADIO").is_some()
        && let Err(message) = embed_interface()
    {
        // The build stops here, with the command to run.
        eprintln!("{message}");
        println!("cargo:warning={message}");
        std::process::exit(1);
    }
}
