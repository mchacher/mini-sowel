//! The interface packed for the flash (spec 022 R7): `collect` reads `ui/dist/` (`index.html` and
//! the files of `assets/`), checks the bounds and the names, and `generate` gzips each file (level
//! 9, no time in the header, so two runs give the same bytes) and writes `ui_files.rs`: a table
//! of `(path, gzip bytes)` sorted by path, whose bytes come from `include_bytes!` and so stay in
//! the application's flash. Build time only: the firmware's code never links `flate2`.

use std::fs;
use std::io::Write;
use std::path::Path;

use flate2::{Compression, GzBuilder};

/// The most files packed (`index.html` included).
pub const MAX_UI_FILES: usize = 32;
/// The largest file, in bytes, before compression.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
/// The environment variable that names another folder than `ui/dist`.
pub const DIST_VARIABLE: &str = "SOLITE_UI_DIST";

const NOT_BUILT: &str = "interface: ui/dist not built: cd ui && npm ci && npm run build";

/// One file of the interface, as built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiFile {
    /// `index.html` or `assets/<name>`, as the core's `static_files` route names them.
    pub path: String,
    /// The bytes as built, before compression.
    pub original: Vec<u8>,
}

/// What `generate` wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub files: usize,
    /// The total of the gzip files, in bytes.
    pub gzip_bytes: usize,
}

impl Summary {
    /// The marker the table carries: `solite-ui <n> files, <gzip KB> KB` (KB rounded down).
    pub fn marker(&self) -> String {
        format!(
            "solite-ui {} files, {} KB",
            self.files,
            self.gzip_bytes / 1024
        )
    }
}

/// The folder to read: `SOLITE_UI_DIST` when set and not empty, else `default`.
pub fn dist_dir(default: &Path) -> std::path::PathBuf {
    match std::env::var_os(DIST_VARIABLE) {
        Some(value) if !value.is_empty() => value.into(),
        _ => default.to_path_buf(),
    }
}

/// A name of `assets/` that the core's `static_files::route` would serve (it never serves
/// another): not empty, not starting with `.`, no `/`, `\`, `%`, NUL or `..`.
fn name_served(name: &str) -> bool {
    !(name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\', '%', '\0'])
        || name.contains(".."))
}

fn read_file(path: &Path, shown: &str) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("interface: cannot read {shown}: {e}"))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "interface: {shown} is {} bytes, over the 1 MB bound per file",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// Reads the built interface: `index.html` (required) and the files of `assets/`, sorted by path.
/// Fails, naming the file, on a bound passed, a sub-folder or a name the core would not serve.
pub fn collect(dist: &Path) -> Result<Vec<UiFile>, String> {
    let index = dist.join("index.html");
    if !index.is_file() {
        return Err(NOT_BUILT.to_string());
    }
    let mut files = vec![UiFile {
        path: "index.html".to_string(),
        original: read_file(&index, "index.html")?,
    }];
    let assets = dist.join("assets");
    if assets.is_dir() {
        let entries =
            fs::read_dir(&assets).map_err(|e| format!("interface: cannot read assets/: {e}"))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("interface: cannot read assets/: {e}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let shown = format!("assets/{name}");
            if !name_served(&name) {
                return Err(format!(
                    "interface: {shown} has a name the core's route never serves"
                ));
            }
            let kind = entry
                .file_type()
                .map_err(|e| format!("interface: cannot read {shown}: {e}"))?;
            if !kind.is_file() {
                return Err(format!(
                    "interface: {shown} is not a plain file (no sub-folder in assets/)"
                ));
            }
            // Bound the count before reading more: one past it is refused at once.
            if files.len() >= MAX_UI_FILES {
                return Err(format!(
                    "interface: more than MAX_UI_FILES ({MAX_UI_FILES}) files, at {shown}"
                ));
            }
            files.push(UiFile {
                original: read_file(&entry.path(), &shown)?,
                path: shown,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// The gzip of `bytes`: level 9, a header with no time and a fixed system byte.
fn gzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), Compression::best());
    encoder
        .write_all(bytes)
        .map_err(|e| format!("interface: gzip failed: {e}"))?;
    encoder
        .finish()
        .map_err(|e| format!("interface: gzip failed: {e}"))
}

/// Writes the `.gz` of each file and `ui_files.rs` into `out_dir`. The table, `UI_FILES`, is
/// sorted by path (`solite_core::api::static_files::find` searches it) and `UI_MARKER` is the
/// marker string.
pub fn generate(files: &[UiFile], out_dir: &Path) -> Result<Summary, String> {
    if files.len() > MAX_UI_FILES {
        return Err(format!(
            "interface: {} files, over MAX_UI_FILES ({MAX_UI_FILES})",
            files.len()
        ));
    }
    let mut sorted: Vec<&UiFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    fs::create_dir_all(out_dir).map_err(|e| format!("interface: {}: {e}", out_dir.display()))?;
    let out_dir = out_dir
        .canonicalize()
        .map_err(|e| format!("interface: {}: {e}", out_dir.display()))?;
    let mut table = String::new();
    let mut gzip_bytes = 0;
    for file in &sorted {
        let packed = gzip(&file.original)?;
        gzip_bytes += packed.len();
        let flat = format!("{}.gz", file.path.replace('/', "__"));
        let target = out_dir.join(&flat);
        fs::write(&target, &packed).map_err(|e| format!("interface: {}: {e}", target.display()))?;
        table.push_str(&format!(
            "    ({:?}, include_bytes!({:?})),\n",
            file.path,
            target.display().to_string()
        ));
    }
    let summary = Summary {
        files: sorted.len(),
        gzip_bytes,
    };
    let source = format!(
        "// Generated by solite-ui-embed: do not edit.\n\
         /// The interface's gzip files, sorted by path.\n\
         pub static UI_FILES: &[(&str, &[u8])] = &[\n{table}];\n\
         /// Logged at boot, checked before a flash.\n\
         pub static UI_MARKER: &str = {:?};\n",
        summary.marker()
    );
    let target = out_dir.join("ui_files.rs");
    fs::write(&target, source).map_err(|e| format!("interface: {}: {e}", target.display()))?;
    Ok(summary)
}
