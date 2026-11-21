//! The interface's files: which request is a request for one, and what it is answered with. The
//! core checks the path and names the content type; the host opens the file under its `--ui`
//! folder and streams it (the core never touches a disk).

use super::{Reply, Request, error, method_not_allowed};

/// The file to send: a path relative to the interface's folder, and its content type.
#[derive(Debug, PartialEq, Eq)]
pub struct StaticFile {
    pub path: String,
    pub content_type: &'static str,
}

impl StaticFile {
    /// `index.html` is read again at each load (its assets carry a hash in their names); the
    /// files of `assets/` never change under a name.
    pub fn is_index(&self) -> bool {
        self.path == INDEX
    }
}

const INDEX: &str = "index.html";
const ASSETS: &str = "/assets/";

/// The content type of a file name, by its extension.
pub fn content_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map_or("", |(_, e)| e);
    match extension.to_ascii_lowercase().as_str() {
        "html" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// `None` when the request is not for the interface (`/` and `/assets/<name>` are); otherwise
/// the answer: a file for the host to send, 405 for a method other than GET, 404 for a path that
/// could leave the folder (`..`, a backslash, any `%`, a sub-folder) or has no name. A query
/// string is ignored.
pub(super) fn route(request: &Request) -> Option<Reply> {
    let path = request.path.split(['?', '#']).next().unwrap_or("");
    let name = if path == "/" {
        None
    } else {
        Some(path.strip_prefix(ASSETS)?)
    };
    if request.method != "GET" {
        return Some(Reply::StaticError(method_not_allowed()));
    }
    let Some(name) = name else {
        return Some(Reply::Static(StaticFile {
            path: INDEX.to_string(),
            content_type: content_type(INDEX),
        }));
    };
    let unsafe_name = name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\', '%', '\0'])
        || name.contains("..");
    if unsafe_name {
        return Some(Reply::StaticError(error(404, "Not found")));
    }
    Some(Reply::Static(StaticFile {
        path: format!("assets/{name}"),
        content_type: content_type(name),
    }))
}

#[cfg(test)]
mod tests {
    use super::content_type;

    #[test]
    fn the_extension_gives_the_type_whatever_its_case() {
        assert_eq!(content_type("a.JS"), "application/javascript");
        assert_eq!(content_type("noextension"), "application/octet-stream");
        assert_eq!(content_type("a.b.css"), "text/css");
    }
}
