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

/// The script recipes of `../recipes/scripts/*.js` (spec 041 R4.2), written as the list
/// `OUT_DIR/script_files.rs`, `SCRIPT_FILES`: (file name, source) in file name order, the source
/// included with `include_str!`.
fn list_scripts() -> Result<(), String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let out_dir = std::env::var("OUT_DIR").map_err(|e| e.to_string())?;
    let folder = Path::new(&manifest).join("../recipes/scripts");
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "js")
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            println!("cargo:rerun-if-changed={}", path.display());
            names.push(name.to_string());
        }
    }
    names.sort();
    let mut text = String::from("pub const SCRIPT_FILES: &[(&str, &str)] = &[\n");
    for name in &names {
        let path = folder.join(name);
        text.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    text.push_str("];\n");
    std::fs::write(Path::new(&out_dir).join("script_files.rs"), text).map_err(|e| e.to_string())
}

fn main() {
    embuild::espidf::sysenv::output();
    if std::env::var_os("CARGO_FEATURE_SCRIPTS").is_some()
        && let Err(message) = list_scripts()
    {
        eprintln!("{message}");
        println!("cargo:warning={message}");
        std::process::exit(1);
    }
    // The update's public key is read with `option_env!` (spec 032 R2).
    println!("cargo:rerun-if-env-changed=SOLITE_UPDATE_PUBLIC_KEY");
    if std::env::var_os("CARGO_FEATURE_RADIO").is_some()
        && let Err(message) = embed_interface()
    {
        // The build stops here, with the command to run.
        eprintln!("{message}");
        println!("cargo:warning={message}");
        std::process::exit(1);
    }
}
