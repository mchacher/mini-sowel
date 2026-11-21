//! Sends the interface's files (spec 014): the core says which file and its content type, this
//! opens it under the `--ui` folder and streams it, never whole in memory.

use std::fs::File;
use std::path::Path;

use mini_sowel_core::api::StaticFile;
use tiny_http::{Header, Request, Response, StatusCode};

/// The default folder of `--ui`: the interface's build output.
pub const DEFAULT_UI_DIR: &str = "ui/dist";

/// What `GET /` says when the interface is not built.
const NOT_BUILT: &str = r#"{"error":"Interface not built: run npm run build in ui/"}"#;

/// `index.html` is looked at again by the browser each time; a file of `assets/` has a hash in
/// its name (Vite's) and never changes under it.
const CACHE_INDEX: &str = "no-cache";
const CACHE_ASSET: &str = "public, max-age=31536000, immutable";

/// Another site framing the page could make the user switch a light.
const SECURITY_HEADERS: [(&str, &str); 3] = [
    ("X-Frame-Options", "DENY"),
    ("Content-Security-Policy", "frame-ancestors 'none'"),
    ("X-Content-Type-Options", "nosniff"),
];

/// True when the folder holds a built interface.
pub fn is_built(ui: &Path) -> bool {
    ui.join("index.html").is_file()
}

/// The line printed at start when it does not (`folder` as it was given).
pub fn missing_line(folder: &Path) -> String {
    format!(
        "interface: {} not found, run npm run build in ui/",
        folder.display()
    )
}

fn header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name, value).ok()
}

fn with_security<R: std::io::Read>(mut response: Response<R>) -> Response<R> {
    for (name, value) in SECURITY_HEADERS {
        if let Some(h) = header(name, value) {
            response.add_header(h);
        }
    }
    response
}

/// Opens a file of the interface when it is a regular file that really lives under the `--ui`
/// folder: a symlink, or a path whose canonical form leaves the folder, is refused.
fn open_inside(ui: &Path, relative: &str) -> Option<(File, u64)> {
    let path = ui.join(relative);
    if std::fs::symlink_metadata(&path)
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    if !path
        .canonicalize()
        .ok()?
        .starts_with(ui.canonicalize().ok()?)
    {
        return None;
    }
    let file = File::open(&path).ok()?;
    let meta = file.metadata().ok()?;
    meta.is_file().then_some((file, meta.len()))
}

/// Answers a refusal (404, 405) of a path of the interface, with the security headers.
pub fn send_refusal(request: Request, status: u16, body: Option<String>) {
    let mut response = Response::from_string(body.unwrap_or_default()).with_status_code(status);
    if let Some(h) = header("Content-Type", "application/json; charset=utf-8") {
        response.add_header(h);
    }
    let _ = request.respond(with_security(response));
}

/// Answers a request for a file of the interface. The client may have gone: nothing to do about
/// it.
pub fn send(request: Request, ui: &Path, file: &StaticFile) {
    let Some((reader, length)) = open_inside(ui, &file.path) else {
        // Only a missing index says that the interface is not built.
        let text = if file.is_index() {
            NOT_BUILT
        } else {
            r#"{"error":"Not found"}"#
        };
        return send_refusal(request, 404, Some(text.to_string()));
    };
    let cache = if file.is_index() {
        CACHE_INDEX
    } else {
        CACHE_ASSET
    };
    let headers: Vec<Header> = [
        header("Content-Type", file.content_type),
        header("Cache-Control", cache),
    ]
    .into_iter()
    .flatten()
    .collect();
    let length = usize::try_from(length).ok();
    // A known length is sent as `Content-Length`, however long (the default of the server is to
    // chunk what is over 32 KB); the file is still read and written a piece at a time.
    let response = Response::new(StatusCode(200), headers, reader, length, None)
        .with_chunked_threshold(usize::MAX);
    let _ = request.respond(with_security(response));
}
