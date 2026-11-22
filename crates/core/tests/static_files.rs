//! The interface's files, as the core routes them: the path checks and the content types. The
//! host opens and streams the files (`crates/host/tests/static_files.rs`).

use solite_core::api::static_files::{STATIC_HEADERS, cache_control, find};
use solite_core::api::{Reach, Reply, Request, StaticFile, handle_stream};
use solite_core::home::Home;

const PORT: u16 = 8080;
const HOST: &str = "127.0.0.1:8080";

fn route(method: &str, path: &str, host: Option<&str>) -> Reply {
    let mut home = Home::new(1_795_252_323_120);
    let mut random = |buffer: &mut [u8]| {
        buffer.fill(7);
        Ok(())
    };
    handle_stream(
        &mut home,
        &Request {
            method,
            path,
            host,
            port: PORT,
            reach: Reach::Loopback,
            content_type: None,
            body: b"",
        },
        1_795_252_323_120,
        &mut random,
    )
}

fn file(reply: Reply) -> StaticFile {
    match reply {
        Reply::Static(file) => file,
        other => panic!("expected a file, got {other:?}"),
    }
}

fn status(reply: Reply) -> u16 {
    match reply {
        Reply::Whole(response) | Reply::StaticError(response) => response.status,
        other => panic!("expected a whole answer, got {other:?}"),
    }
}

#[test]
fn the_root_is_the_index_and_a_query_string_is_ignored() {
    let index = file(route("GET", "/", Some(HOST)));
    assert_eq!(
        (index.path.as_str(), index.content_type),
        ("index.html", "text/html")
    );
    assert!(index.is_index());
    assert_eq!(file(route("GET", "/?a=1", Some(HOST))), index);
}

#[test]
fn an_asset_is_a_file_of_assets_with_the_type_of_its_extension() {
    let script = file(route("GET", "/assets/x.js", Some(HOST)));
    assert_eq!(script.path, "assets/x.js");
    assert_eq!(script.content_type, "application/javascript");
    assert!(!script.is_index());
    for (name, kind) in [
        ("a.css", "text/css"),
        ("a.svg", "image/svg+xml"),
        ("a.woff2", "font/woff2"),
        ("a.bin", "application/octet-stream"),
    ] {
        let found = file(route("GET", &format!("/assets/{name}?v=2"), Some(HOST)));
        assert_eq!(found.content_type, kind);
    }
}

#[test]
fn a_path_that_could_leave_the_folder_or_has_no_name_is_404() {
    for path in [
        "/assets/../a",
        "/assets/a%2e",
        "/assets/%2e%2e/x",
        "/assets/a\\b",
        "/assets/",
        "/assets/a/b",
        "/assets/.hidden",
    ] {
        assert_eq!(status(route("GET", path, Some(HOST))), 404, "{path}");
    }
}

#[test]
fn another_method_is_405_and_another_host_is_403() {
    assert_eq!(status(route("POST", "/", Some(HOST))), 405);
    assert_eq!(status(route("DELETE", "/assets/x.js", Some(HOST))), 405);
    assert_eq!(status(route("GET", "/", Some("evil.example"))), 403);
    assert_eq!(status(route("GET", "/", None)), 403);
}

#[test]
fn the_security_headers_and_the_cache_rule_are_the_hosts() {
    assert_eq!(
        STATIC_HEADERS,
        [
            ("X-Frame-Options", "DENY"),
            ("Content-Security-Policy", "frame-ancestors 'none'"),
            ("X-Content-Type-Options", "nosniff"),
        ]
    );
    let index = file(route("GET", "/", Some(HOST)));
    let asset = file(route("GET", "/assets/index-abc123.js", Some(HOST)));
    assert_eq!(cache_control(&index), "no-cache");
    assert_eq!(cache_control(&asset), "public, max-age=31536000, immutable");
}

#[test]
fn find_looks_a_path_up_in_a_sorted_table() {
    let table: [(&str, &[u8]); 4] = [
        ("assets/a.css", b"css"),
        ("assets/b.js", b"js"),
        ("assets/c.js", b"js2"),
        ("index.html", b"html"),
    ];
    assert_eq!(find(&table, "assets/b.js"), Some(&b"js"[..]));
    assert_eq!(find(&table, "index.html"), Some(&b"html"[..]));
    assert_eq!(find(&table, "assets/missing.js"), None);
    for unsafe_path in ["../index.html", "assets/%2e%2e", "assets/sub/b.js", ""] {
        assert_eq!(find(&table, unsafe_path), None, "{unsafe_path}");
    }
}
