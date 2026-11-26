//! The host's `Fetcher` (spec 039, R9, D69): one GET, no redirect followed, no TLS library.
//! `http://` (a local server) by the standard library: the head parsed with `httparse`, a body
//! delimited by `Content-Length` or by the connection's end. `https://` through the system's
//! `curl` program, its output's head parsed the same way. The core checked the URL and follows the
//! redirects; this only moves the bytes. A range request (spec 040 R6) adds `Range: bytes=<n>-`
//! and a 206 gives its `Content-Range` and `ETag`.

use std::io::{self, Cursor, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

use solite_core::update::release::{Fetched, Fetcher};

/// The longest answer head read before giving up (a head is a few hundred bytes; a GitHub
/// redirect's `Location` a few thousand at most).
const MAX_HEAD_BYTES: usize = 16 * 1024;
/// The most headers of one answer.
const MAX_HEADERS: usize = 64;

/// What the head of an answer says.
#[derive(Debug, PartialEq, Eq)]
struct Head {
    status: u16,
    reason: String,
    location: Option<String>,
    length: Option<u64>,
    chunked: bool,
    content_range: Option<String>,
    etag: Option<String>,
}

/// Parses an answer head from the start of `buffer`: `None` while it is not complete, else the
/// head and the number of bytes it took.
fn parse_head(buffer: &[u8]) -> Result<Option<(Head, usize)>, String> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut response = httparse::Response::new(&mut headers);
    let used = match response.parse(buffer) {
        Ok(httparse::Status::Complete(used)) => used,
        Ok(httparse::Status::Partial) => return Ok(None),
        Err(e) => return Err(format!("invalid answer: {e}")),
    };
    let mut head = Head {
        status: response.code.unwrap_or(0),
        reason: response.reason.unwrap_or("").to_string(),
        location: None,
        length: None,
        chunked: false,
        content_range: None,
        etag: None,
    };
    for header in response.headers.iter() {
        let value = String::from_utf8_lossy(header.value).trim().to_string();
        if header.name.eq_ignore_ascii_case("Location") {
            head.location = Some(value);
        } else if header.name.eq_ignore_ascii_case("Content-Length") {
            head.length = Some(
                value
                    .parse()
                    .map_err(|_| "invalid answer: Content-Length".to_string())?,
            );
        } else if header.name.eq_ignore_ascii_case("Content-Range") {
            head.content_range = Some(value);
        } else if header.name.eq_ignore_ascii_case("ETag") {
            head.etag = Some(value);
        } else if header.name.eq_ignore_ascii_case("Transfer-Encoding")
            && value.to_ascii_lowercase().contains("chunked")
        {
            head.chunked = true;
        }
    }
    Ok(Some((head, used)))
}

/// Reads from `source` until a head is complete; the head and the body bytes read past it. A
/// proxy's `200 Connection established` head is skipped (`curl` prints it before the answer).
fn read_head(source: &mut dyn Read) -> Result<(Head, Vec<u8>), String> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        if let Some((head, used)) = parse_head(&buffer)? {
            let rest = buffer.split_off(used);
            if head.status == 200 && head.reason.eq_ignore_ascii_case("Connection established") {
                buffer = rest;
                continue;
            }
            return Ok((head, rest));
        }
        if buffer.len() > MAX_HEAD_BYTES {
            return Err("answer head too large".to_string());
        }
        match source.read(&mut chunk) {
            Ok(0) => return Err("connection closed before the answer".to_string()),
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return Err(reason(&e)),
        }
    }
}

fn reason(error: &io::Error) -> String {
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "timed out".to_string(),
        _ => error.to_string(),
    }
}

/// A body: the bytes read with the head, then the source; bounded by `Content-Length` when
/// given (an end before it is `connection lost`), else read to the connection's end.
struct Body<S: Read> {
    prefix: Cursor<Vec<u8>>,
    source: S,
    remaining: Option<u64>,
}

impl<S: Read> Read for Body<S> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.remaining == Some(0) {
            return Ok(0);
        }
        let max = self.remaining.map_or(out.len(), |r| {
            out.len().min(usize::try_from(r).unwrap_or(usize::MAX))
        });
        let mut count = self.prefix.read(&mut out[..max])?;
        if count == 0 {
            count = self.source.read(&mut out[..max])?;
            if count == 0 && self.remaining.is_some() {
                return Err(io::Error::new(ErrorKind::UnexpectedEof, "connection lost"));
            }
        }
        if let Some(remaining) = &mut self.remaining {
            *remaining -= count as u64;
        }
        Ok(count)
    }
}

/// What the head makes of the answer: a body, a redirect, or a status.
///
/// `decoded` is true for `curl`'s output: `-i` prints the server's `Transfer-Encoding: chunked`
/// header but writes the body already decoded, so it is read to the end.
fn fetched<S: Read + 'static>(
    mut head: Head,
    rest: Vec<u8>,
    source: S,
    decoded: bool,
) -> Result<Fetched, String> {
    if decoded && head.chunked {
        head.chunked = false;
        head.length = None;
    }
    match head.status {
        200 | 206 => {
            if head.chunked {
                return Err("chunked answers are not supported over http".to_string());
            }
            let body = Box::new(Body {
                prefix: Cursor::new(rest),
                source,
                remaining: head.length,
            });
            if head.status == 206 {
                return Ok(Fetched::Partial {
                    content_range: head.content_range,
                    length: head.length,
                    etag: head.etag,
                    body,
                });
            }
            Ok(Fetched::Body {
                length: head.length,
                etag: head.etag,
                body,
            })
        }
        301 | 302 | 303 | 307 | 308 => Ok(Fetched::Redirect(head.location)),
        status => Ok(Fetched::Status(status)),
    }
}

/// The host and port, and the path with its query (no fragment), of an `http://` URL the core
/// already checked.
fn split_http(url: &str) -> Result<(String, String), String> {
    let rest = url
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("http://"))
        .map(|_| &url[7..])
        .ok_or_else(|| "not an http URL".to_string())?;
    let end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(end);
    let path = path.split('#').next().unwrap_or("");
    let path = if path.is_empty() { "/" } else { path };
    Ok((authority.to_string(), path.to_string()))
}

/// The seconds of a curl timeout: the idle time rounded up, at least 1.
fn seconds(idle_ms: u64) -> u64 {
    idle_ms.div_ceil(1000).max(1)
}

/// One GET, `http` by the standard library, `https` by `curl`.
pub struct HttpFetcher {
    program: String,
}

impl HttpFetcher {
    pub fn new() -> Self {
        HttpFetcher {
            program: "curl".to_string(),
        }
    }

    fn get_http(url: &str, from: Option<u64>, idle_ms: u64) -> Result<Fetched, String> {
        let (authority, path) = split_http(url)?;
        let target = if authority.contains(':') {
            authority.clone()
        } else {
            format!("{authority}:80")
        };
        let idle = Duration::from_millis(idle_ms.max(1));
        let address = target
            .to_socket_addrs()
            .map_err(|e| format!("{authority}: {e}"))?
            .next()
            .ok_or_else(|| format!("{authority}: no address"))?;
        let mut stream = TcpStream::connect_timeout(&address, idle).map_err(|e| {
            if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) {
                "timed out".to_string()
            } else {
                format!("{authority}: {e}")
            }
        })?;
        stream
            .set_read_timeout(Some(idle))
            .and_then(|()| stream.set_write_timeout(Some(idle)))
            .map_err(|e| e.to_string())?;
        let range = from.map_or(String::new(), |from| format!("Range: bytes={from}-\r\n"));
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: solite/{}\r\nAccept: */*\r\n{range}Connection: close\r\n\r\n",
            solite_core::VERSION
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| reason(&e))?;
        let (head, rest) = read_head(&mut stream)?;
        fetched(head, rest, stream, false)
    }

    fn get_https(&self, url: &str, from: Option<u64>, idle_ms: u64) -> Result<Fetched, String> {
        let mut child = Command::new(&self.program)
            .args(curl_args(url, from, idle_ms))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    "https needs the curl program".to_string()
                } else {
                    format!("cannot run curl: {e}")
                }
            })?;
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cannot read curl's output".to_string());
        };
        match read_head(&mut stdout) {
            Ok((head, rest)) => fetched(head, rest, CurlOutput { child, stdout }, true),
            Err(message) => {
                // curl stopped before an answer: its own message says why.
                let _ = child.wait();
                let mut text = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut text);
                }
                let text = text.trim();
                Err(if text.is_empty() {
                    message
                } else {
                    text.to_string()
                })
            }
        }
    }
}

/// The command line of `curl` for one GET (no redirect followed, the head printed, a range when
/// `from` is given).
fn curl_args(url: &str, from: Option<u64>, idle_ms: u64) -> Vec<String> {
    let idle = seconds(idle_ms).to_string();
    let mut args: Vec<String> = [
        "-q",
        "-sS",
        "-i",
        "-g",
        "--http1.1",
        "--proto",
        "=https",
        "--max-redirs",
        "0",
        "--connect-timeout",
        &idle,
        "--speed-limit",
        "1",
        "--speed-time",
        &idle,
        "-H",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    args.push(format!("User-Agent: solite/{}", solite_core::VERSION));
    if let Some(from) = from {
        args.push("-H".to_string());
        args.push(format!("Range: bytes={from}-"));
    }
    args.push("--".to_string());
    args.push(url.to_string());
    args
}

impl Default for HttpFetcher {
    fn default() -> Self {
        HttpFetcher::new()
    }
}

impl HttpFetcher {
    fn request(&self, url: &str, from: Option<u64>, idle_ms: u64) -> Result<Fetched, String> {
        let is_http = url
            .get(..7)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"));
        if is_http {
            HttpFetcher::get_http(url, from, idle_ms)
        } else {
            self.get_https(url, from, idle_ms)
        }
    }
}

impl Fetcher for HttpFetcher {
    fn get(&mut self, url: &str, idle_ms: u64) -> Result<Fetched, String> {
        self.request(url, None, idle_ms)
    }

    fn get_range(&mut self, url: &str, from: u64, idle_ms: u64) -> Result<Fetched, String> {
        self.request(url, Some(from), idle_ms)
    }
}

/// `curl`'s output after its head: killed when dropped, so a body left unread (a refused
/// download) never leaves the program running.
struct CurlOutput {
    child: Child,
    stdout: ChildStdout,
}

impl Read for CurlOutput {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = self.stdout.read(out)?;
        if count == 0 && !out.is_empty() {
            // The end of curl's output: a non-zero exit (a timeout, a cut TLS connection) is
            // an error carrying curl's message, not a short body.
            let status = self.child.wait()?;
            let mut text = String::new();
            if let Some(mut stderr) = self.child.stderr.take() {
                let _ = stderr.read_to_string(&mut text);
            }
            exit_result(status.success(), &text)?;
        }
        Ok(count)
    }
}

/// The end of curl's output: nothing when it exited well, else an error with its message.
fn exit_result(success: bool, stderr: &str) -> io::Result<()> {
    if success {
        return Ok(());
    }
    let text = stderr.trim();
    Err(io::Error::other(if text.is_empty() {
        "curl failed"
    } else {
        text
    }))
}

impl Drop for CurlOutput {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::{Map, Value, json};
    use solite_core::backup::Pacing;
    use solite_core::home::Home;
    use solite_core::update::release::{DEFAULT_MANIFEST_URL, Limits};
    use solite_core::update::{HEADER_BYTES, Updates, Version, image_digest, signed_bytes};

    use super::*;
    use crate::backup_files::HostPacing;
    use crate::update_files::FileAdapter;

    /// The real clock, but a wait does not sleep (the resumes of these tests wait 2 s and more).
    struct QuickPacing;

    impl Pacing for QuickPacing {
        fn now_ms(&mut self) -> u64 {
            HostPacing.now_ms()
        }

        fn pause(&mut self) {}

        fn wait_ms(&mut self, _ms: u64) {}
    }

    const T0: u64 = 1_795_252_323_120;
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// What a test server does with a path.
    #[derive(Clone)]
    enum Behaviour {
        /// Writes these bytes, then closes.
        Raw(Vec<u8>),
        /// Writes the head and some of the body, then waits this long without a byte.
        Stall(Vec<u8>, u64),
        /// Writes the bytes one at a time, a pause between two, until the client goes.
        Trickle(Vec<u8>, u64),
    }

    fn http(status: &str, headers: &[&str], body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\n").into_bytes();
        for header in headers {
            out.extend_from_slice(format!("{header}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(body);
        out
    }

    fn ok(body: &[u8]) -> Behaviour {
        Behaviour::Raw(http(
            "200 OK",
            &[&format!("Content-Length: {}", body.len())],
            body,
        ))
    }

    fn found(location: &str) -> Behaviour {
        Behaviour::Raw(http(
            "302 Found",
            &[&format!("Location: {location}"), "Content-Length: 0"],
            b"",
        ))
    }

    /// A test HTTP server on `127.0.0.1:0`, in threads of this test.
    struct TestServer {
        port: u16,
        routes: Arc<Mutex<HashMap<String, Behaviour>>>,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl TestServer {
        fn start() -> TestServer {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let routes: Arc<Mutex<HashMap<String, Behaviour>>> = Arc::default();
            let requests: Arc<Mutex<Vec<String>>> = Arc::default();
            let (shared_routes, shared_requests) = (Arc::clone(&routes), Arc::clone(&requests));
            thread::spawn(move || {
                for stream in listener.incoming().map_while(Result::ok) {
                    let (routes, requests) =
                        (Arc::clone(&shared_routes), Arc::clone(&shared_requests));
                    thread::spawn(move || serve_one(stream, &routes, &requests));
                }
            });
            TestServer {
                port,
                routes,
                requests,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn route(&self, path: &str, behaviour: Behaviour) {
            self.routes
                .lock()
                .unwrap()
                .insert(path.to_string(), behaviour);
        }
    }

    fn serve_one(
        mut stream: TcpStream,
        routes: &Mutex<HashMap<String, Behaviour>>,
        requests: &Mutex<Vec<String>>,
    ) {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => return,
            }
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let path = text.split(' ').nth(1).unwrap_or("/").to_string();
        requests.lock().unwrap().push(text);
        let behaviour = routes.lock().unwrap().get(&path).cloned();
        match behaviour {
            None => {
                let _ = stream.write_all(&http("404 Not Found", &["Content-Length: 0"], b""));
            }
            Some(Behaviour::Raw(bytes)) => {
                let _ = stream.write_all(&bytes);
            }
            Some(Behaviour::Stall(bytes, ms)) => {
                let _ = stream.write_all(&bytes);
                thread::sleep(Duration::from_millis(ms));
            }
            Some(Behaviour::Trickle(bytes, ms)) => {
                for byte in bytes {
                    if stream.write_all(&[byte]).is_err() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(ms));
                }
            }
        }
    }

    fn get(server: &TestServer, path: &str, idle_ms: u64) -> Result<Fetched, String> {
        HttpFetcher::new().get(&server.url(path), idle_ms)
    }

    fn read_all(fetched: Fetched) -> (Option<u64>, io::Result<Vec<u8>>) {
        let Fetched::Body {
            length, mut body, ..
        } = fetched
        else {
            panic!("not a body");
        };
        let mut out = Vec::new();
        let result = body.read_to_end(&mut out).map(|_| out);
        (length, result)
    }

    // ---- HttpFetcher ----

    #[test]
    fn a_200_with_a_length_gives_the_length_and_the_bytes() {
        let server = TestServer::start();
        server.route("/a", ok(b"hello"));
        let (length, bytes) = read_all(get(&server, "/a", 2000).unwrap());
        assert_eq!(length, Some(5));
        assert_eq!(bytes.unwrap(), b"hello");
    }

    #[test]
    fn a_200_without_a_length_is_read_to_the_end() {
        let server = TestServer::start();
        server.route("/a", Behaviour::Raw(http("200 OK", &[], b"until the end")));
        let (length, bytes) = read_all(get(&server, "/a", 2000).unwrap());
        assert_eq!(length, None);
        assert_eq!(bytes.unwrap(), b"until the end");
    }

    #[test]
    fn a_302_gives_its_location_and_a_404_its_status() {
        let server = TestServer::start();
        server.route("/a", found("https://example.com/b"));
        match get(&server, "/a", 2000).unwrap() {
            Fetched::Redirect(location) => {
                assert_eq!(location.as_deref(), Some("https://example.com/b"));
            }
            _ => panic!("not a redirect"),
        }
        match get(&server, "/missing", 2000).unwrap() {
            Fetched::Status(status) => assert_eq!(status, 404),
            _ => panic!("not a status"),
        }
    }

    #[test]
    fn a_chunked_answer_is_refused() {
        let server = TestServer::start();
        server.route(
            "/a",
            Behaviour::Raw(http(
                "200 OK",
                &["Transfer-Encoding: chunked"],
                b"5\r\nhello\r\n0\r\n\r\n",
            )),
        );
        let error = get(&server, "/a", 2000).err().unwrap();
        assert_eq!(error, "chunked answers are not supported over http");
    }

    #[test]
    fn a_server_that_never_answers_times_out() {
        let server = TestServer::start();
        server.route("/a", Behaviour::Stall(Vec::new(), 2000));
        let started = std::time::Instant::now();
        let error = get(&server, "/a", 300).err().unwrap();
        assert_eq!(error, "timed out");
        assert!(started.elapsed() < Duration::from_millis(1800));
    }

    #[test]
    fn a_body_cut_before_its_length_is_connection_lost() {
        let server = TestServer::start();
        server.route(
            "/a",
            Behaviour::Raw(http("200 OK", &["Content-Length: 100"], b"only ten b")),
        );
        let (_, bytes) = read_all(get(&server, "/a", 2000).unwrap());
        let error = bytes.unwrap_err();
        assert_eq!(error.to_string(), "connection lost");
    }

    #[test]
    fn the_request_carries_the_user_agent_and_no_cookie_or_authorization() {
        let server = TestServer::start();
        server.route("/a?q=1", ok(b"x"));
        let _ = read_all(get(&server, "/a?q=1#frag", 2000).unwrap());
        let request = server.requests.lock().unwrap()[0].clone();
        assert!(request.starts_with("GET /a?q=1 HTTP/1.1\r\n"), "{request}");
        assert!(
            request.contains(&format!("User-Agent: solite/{}\r\n", solite_core::VERSION)),
            "{request}"
        );
        let lower = request.to_ascii_lowercase();
        assert!(!lower.contains("cookie") && !lower.contains("authorization"));
        assert!(lower.contains("accept: */*"));
    }

    #[test]
    fn get_range_sends_the_range_and_parses_a_206_with_its_headers() {
        let server = TestServer::start();
        server.route(
            "/a",
            Behaviour::Raw(http(
                "206 Partial Content",
                &[
                    "Content-Range: bytes 5-9/10",
                    "Content-Length: 5",
                    "ETag: \"abc\"",
                ],
                b"world",
            )),
        );
        let fetched = HttpFetcher::new()
            .get_range(&server.url("/a"), 5, 2000)
            .unwrap();
        let request = server.requests.lock().unwrap()[0].clone();
        assert!(request.contains("\r\nRange: bytes=5-\r\n"), "{request}");
        let Fetched::Partial {
            content_range,
            length,
            etag,
            mut body,
        } = fetched
        else {
            panic!("not a partial");
        };
        assert_eq!(content_range.as_deref(), Some("bytes 5-9/10"));
        assert_eq!(length, Some(5));
        assert_eq!(etag.as_deref(), Some("\"abc\""));
        let mut text = Vec::new();
        body.read_to_end(&mut text).unwrap();
        assert_eq!(text, b"world");
    }

    #[test]
    fn a_plain_get_sends_no_range_and_a_200_gives_its_etag() {
        let server = TestServer::start();
        server.route(
            "/a",
            Behaviour::Raw(http(
                "200 OK",
                &["Content-Length: 2", "ETag: W/\"x\""],
                b"hi",
            )),
        );
        let Fetched::Body { etag, .. } = get(&server, "/a", 2000).unwrap() else {
            panic!("not a body");
        };
        assert_eq!(etag.as_deref(), Some("W/\"x\""));
        let request = server.requests.lock().unwrap()[0].clone();
        assert!(!request.to_ascii_lowercase().contains("range"), "{request}");
    }

    #[test]
    fn the_curl_command_line_carries_the_range_only_when_asked() {
        let url = "https://example.com/a.sup";
        let plain = curl_args(url, None, 30_000);
        assert!(
            !plain.iter().any(|arg| arg.starts_with("Range")),
            "{plain:?}"
        );
        assert_eq!(plain.last().map(String::as_str), Some(url));
        let ranged = curl_args(url, Some(4096), 30_000);
        let at = ranged
            .iter()
            .position(|arg| arg == "Range: bytes=4096-")
            .expect("the range header");
        assert_eq!(ranged[at - 1], "-H");
        // Before the `--` that ends the options, the URL last.
        assert_eq!(ranged[ranged.len() - 2], "--");
        assert_eq!(ranged.last().map(String::as_str), Some(url));
        assert!(ranged.iter().any(|arg| arg == "--max-redirs"));
    }

    #[test]
    fn a_captured_curl_head_of_a_206_is_parsed_with_its_range_and_etag() {
        let captured = b"HTTP/1.1 206 Partial Content\r\nContent-Type: application/octet-stream\r\nContent-Range: bytes 100-199/200\r\nETag: \"0xDEADBEEF\"\r\nContent-Length: 100\r\n\r\nabc";
        let (head, rest) = read_head(&mut Cursor::new(captured.to_vec())).unwrap();
        assert_eq!(head.status, 206);
        assert_eq!(head.content_range.as_deref(), Some("bytes 100-199/200"));
        assert_eq!(head.etag.as_deref(), Some("\"0xDEADBEEF\""));
        match fetched(head, rest, io::empty(), true).unwrap() {
            Fetched::Partial {
                content_range,
                length,
                etag,
                ..
            } => {
                assert_eq!(content_range.as_deref(), Some("bytes 100-199/200"));
                assert_eq!(length, Some(100));
                assert_eq!(etag.as_deref(), Some("\"0xDEADBEEF\""));
            }
            _ => panic!("not a partial"),
        }
    }

    #[test]
    fn a_captured_curl_head_is_parsed_after_a_proxys_connection_established() {
        let captured = b"HTTP/1.1 200 Connection established\r\n\r\nHTTP/1.1 302 Found\r\nServer: GitHub.com\r\nLocation: https://release-assets.githubusercontent.com/x?sig=a%2Fb\r\nContent-Length: 0\r\n\r\n";
        let (head, rest) = read_head(&mut Cursor::new(captured.to_vec())).unwrap();
        assert_eq!(head.status, 302);
        assert_eq!(
            head.location.as_deref(),
            Some("https://release-assets.githubusercontent.com/x?sig=a%2Fb")
        );
        assert!(rest.is_empty());
        match fetched(head, rest, io::empty(), true).unwrap() {
            Fetched::Redirect(Some(_)) => {}
            _ => panic!("not a redirect"),
        }
        // A head that never ends is an error, not a hang.
        let error = read_head(&mut Cursor::new(b"HTTP/1.1 200 OK\r\nX: y".to_vec())).err();
        assert_eq!(
            error.as_deref(),
            Some("connection closed before the answer")
        );
    }

    #[test]
    fn curls_chunked_header_is_ignored_its_body_is_already_decoded() {
        let captured = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\n\r\n{\"a\":1}";
        let (head, rest) = read_head(&mut Cursor::new(captured.to_vec())).unwrap();
        assert!(head.chunked);
        let (length, bytes) = read_all(fetched(head, rest, io::empty(), true).unwrap());
        assert_eq!(length, None);
        assert_eq!(bytes.unwrap(), b"{\"a\":1}");
        // Over http the same head is still refused.
        let (head, rest) = read_head(&mut Cursor::new(captured.to_vec())).unwrap();
        assert!(fetched(head, rest, io::empty(), false).is_err());
    }

    #[test]
    fn a_non_zero_curl_exit_is_an_error_with_its_message() {
        assert!(exit_result(true, "").is_ok());
        let error = exit_result(false, "curl: (28) Operation timed out\n").unwrap_err();
        assert_eq!(error.to_string(), "curl: (28) Operation timed out");
        assert_eq!(
            exit_result(false, " ").unwrap_err().to_string(),
            "curl failed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_curl_that_dies_after_the_head_gives_an_error_not_a_short_body() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp();
        let script = dir.join("fake-curl");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'HTTP/1.1 200 OK\\r\\n\\r\\n{\"x\"'\necho 'curl: (28) Operation timed out' >&2\nexit 28\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut fetcher = HttpFetcher {
            program: script.to_string_lossy().to_string(),
        };
        let (length, bytes) = read_all(fetcher.get("https://example.com/", 1000).unwrap());
        assert_eq!(length, None);
        assert_eq!(
            bytes.unwrap_err().to_string(),
            "curl: (28) Operation timed out"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_curl_https_says_so() {
        let mut fetcher = HttpFetcher {
            program: "solite-no-such-program".to_string(),
        };
        let error = fetcher.get("https://example.com/", 1000).err().unwrap();
        assert_eq!(error, "https needs the curl program");
    }

    /// Needs the network and `curl`: the real address answers a redirect (a release exists) or a
    /// 404 (none yet), after a TLS handshake.
    #[test]
    #[ignore = "network"]
    fn the_real_github_address_answers_over_tls() {
        match HttpFetcher::new()
            .get(DEFAULT_MANIFEST_URL, 15_000)
            .unwrap()
        {
            Fetched::Redirect(_) | Fetched::Status(404) | Fetched::Body { .. } => {}
            Fetched::Partial { .. } => panic!("a 206 to an unranged request"),
            Fetched::Status(status) => panic!("status {status}"),
        }
    }

    // ---- The pull through real sockets ----

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "solite-fetch-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn image(len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| u8::try_from((i * 13 + i / 97) % 256).unwrap())
            .collect()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    struct Rig {
        server: TestServer,
        signing: SigningKey,
        updates: Updates,
        adapter: FileAdapter,
        home: Home,
        data: PathBuf,
    }

    impl Rig {
        fn new() -> Rig {
            let mut seed = [0u8; 32];
            crate::random::SystemRandom::open()
                .unwrap()
                .fill(&mut seed)
                .unwrap();
            let signing = SigningKey::from_bytes(&seed);
            let (updates, _) =
                Updates::from_key_text(Some(&hex(&signing.verifying_key().to_bytes())));
            let server = TestServer::start();
            let data = temp();
            let mut home = Home::new(T0);
            let mut entries = Map::new();
            entries.insert("update.manifestUrl".into(), json!(server.url("/m.json")));
            home.put_settings(&entries).unwrap();
            Rig {
                server,
                signing,
                updates,
                adapter: FileAdapter::new(&data),
                home,
                data,
            }
        }

        fn sup(&self, version: Version, image: &[u8]) -> Vec<u8> {
            let digest = image_digest(image);
            let signed = signed_bytes(version, u32::try_from(image.len()).unwrap(), &digest);
            let mut out = signed.to_vec();
            out.extend_from_slice(&self.signing.sign(&signed).to_bytes());
            out.extend_from_slice(image);
            out
        }

        fn manifest(&self, sup: &[u8], version: &str) -> Value {
            json!({
                "format": 1, "version": version, "minVersion": null,
                "image": {
                    "size": sup.len(),
                    "sha256": hex(&image_digest(sup)),
                    "url": self.server.url("/a"),
                },
            })
        }

        /// The manifest, and the file behind two redirects.
        fn serve(&self, manifest: &Value, sup: &[u8]) {
            self.server
                .route("/m.json", ok(manifest.to_string().as_bytes()));
            self.server.route("/a", found(&self.server.url("/b")));
            self.server.route("/b", found("/c"));
            self.server.route("/c", ok(sup));
        }

        fn pull(&mut self, check: Limits, download: Limits) -> (u16, Value) {
            let ticket = self.home.release_ticket(false, T0).unwrap();
            let response = ticket.pull(
                &self.updates,
                &mut HttpFetcher::new(),
                &mut self.adapter,
                &mut QuickPacing,
                check,
                download,
            );
            (
                response.status,
                serde_json::from_str(&response.body.unwrap()).unwrap(),
            )
        }

        fn quick(&mut self) -> (u16, Value) {
            self.pull(
                Limits {
                    idle_ms: 2000,
                    total_ms: 10_000,
                },
                Limits {
                    idle_ms: 2000,
                    total_ms: 10_000,
                },
            )
        }

        fn untouched(&self) {
            let update = self.data.join("update");
            assert!(!update.join("boot.json").exists());
            assert!(!update.join("slot-1.bin").exists());
            assert!(!update.join("slot-1.bin.part").exists());
        }
    }

    fn error_of(answer: &(u16, Value)) -> (u16, &str) {
        (answer.0, answer.1["error"].as_str().unwrap_or(""))
    }

    #[test]
    fn a_pull_through_two_redirects_writes_the_slot() {
        let mut rig = Rig::new();
        let sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        rig.serve(&rig.manifest(&sup, "0.2.0"), &sup);
        let (status, body) = rig.quick();
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["message"], "Installed 0.2.0, restarting");
        let slot = std::fs::read(rig.data.join("update/slot-1.bin")).unwrap();
        assert_eq!(slot, sup[HEADER_BYTES..]);
        assert_eq!(rig.adapter.boot_slot(), 1);
    }

    #[test]
    fn a_cut_connection_leaves_the_slot_not_bootable() {
        let mut rig = Rig::new();
        let sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        rig.serve(&rig.manifest(&sup, "0.2.0"), &sup);
        rig.server.route(
            "/c",
            Behaviour::Raw(http(
                "200 OK",
                &[&format!("Content-Length: {}", sup.len())],
                &sup[..9000],
            )),
        );
        let answer = rig.quick();
        // The range request gets the same cut 200: the server does not resume.
        assert_eq!(
            error_of(&answer),
            (
                500,
                "Download failed: the server does not resume (200 to a range request)"
            )
        );
        rig.untouched();
    }

    #[test]
    fn a_wrong_hash_a_wrong_signature_and_an_old_version_are_refused() {
        let mut rig = Rig::new();
        let sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        let mut manifest = rig.manifest(&sup, "0.2.0");
        manifest["image"]["sha256"] = json!(hex(&image_digest(b"other")));
        rig.serve(&manifest, &sup);
        let answer = rig.quick();
        assert_eq!(
            error_of(&answer),
            (400, "Download does not match the manifest")
        );
        rig.untouched();

        let mut rig = Rig::new();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let mut sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        let signed = signed_bytes(Version::new(0, 2, 0), 20_000, &image_digest(&image(20_000)));
        sup[64..128].copy_from_slice(&other.sign(&signed).to_bytes());
        rig.serve(&rig.manifest(&sup, "0.2.0"), &sup);
        assert_eq!(error_of(&rig.quick()), (400, "Invalid signature"));
        rig.untouched();

        let mut rig = Rig::new();
        let version = Version::parse(solite_core::VERSION).unwrap();
        let sup = rig.sup(version, &image(20_000));
        rig.serve(&rig.manifest(&sup, &version.to_string()), &sup);
        assert_eq!(error_of(&rig.quick()), (400, "No update available"));
        rig.untouched();
    }

    #[test]
    fn a_min_version_too_high_and_a_manifest_too_large_are_refused() {
        let mut rig = Rig::new();
        let sup = rig.sup(Version::new(0, 9, 0), &image(20_000));
        let mut manifest = rig.manifest(&sup, "0.9.0");
        manifest["minVersion"] = json!("0.5.0");
        rig.serve(&manifest, &sup);
        let answer = rig.quick();
        assert_eq!(answer.0, 400, "{}", answer.1);
        assert!(answer.1["error"].as_str().unwrap().contains("needs 0.5.0"));
        rig.untouched();

        let mut rig = Rig::new();
        rig.server.route("/m.json", ok(&vec![b' '; 16 * 1024 + 1]));
        let answer = rig.quick();
        assert_eq!(
            error_of(&answer),
            (500, "Update check failed: Manifest too large")
        );
        rig.untouched();
    }

    #[test]
    fn a_download_larger_than_the_manifest_says_is_refused() {
        let mut rig = Rig::new();
        let sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        let mut manifest = rig.manifest(&sup, "0.2.0");
        manifest["image"]["size"] = json!(sup.len() - 100);
        rig.serve(&manifest, &sup);
        // The server says its length: refused before any write.
        assert_eq!(
            error_of(&rig.quick()),
            (400, "Download size differs from the manifest")
        );
        rig.untouched();
        // The server says none (close-delimited): the count catches it.
        let mut rig = Rig::new();
        let mut manifest = rig.manifest(&sup, "0.2.0");
        manifest["image"]["size"] = json!(sup.len() - 100);
        rig.serve(&manifest, &sup);
        rig.server
            .route("/c", Behaviour::Raw(http("200 OK", &[], &sup)));
        let answer = rig.quick();
        assert_eq!(answer.0, 400, "{}", answer.1);
        rig.untouched();
    }

    #[test]
    fn a_slow_server_hits_the_idle_then_the_total_timeout() {
        let mut rig = Rig::new();
        let sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        rig.serve(&rig.manifest(&sup, "0.2.0"), &sup);
        rig.server.route(
            "/c",
            Behaviour::Stall(
                http(
                    "200 OK",
                    &[&format!("Content-Length: {}", sup.len())],
                    &sup[..3000],
                ),
                3000,
            ),
        );
        let limits = |idle_ms, total_ms| Limits { idle_ms, total_ms };
        let answer = rig.pull(limits(2000, 10_000), limits(300, 10_000));
        // The idle timeout is a cut: resumed, and the same stalled 200 answers the range request.
        assert_eq!(
            error_of(&answer),
            (
                500,
                "Download failed: the server does not resume (200 to a range request)"
            )
        );
        rig.untouched();

        let mut rig = Rig::new();
        // The file is signed by this rig's own key: the signature of its header checks out, and
        // only the total time limit can stop the trickle.
        let sup = rig.sup(Version::new(0, 2, 0), &image(20_000));
        rig.serve(&rig.manifest(&sup, "0.2.0"), &sup);
        let mut slow = http("200 OK", &[&format!("Content-Length: {}", sup.len())], b"");
        slow.extend_from_slice(&sup);
        rig.server.route("/c", Behaviour::Trickle(slow, 5));
        let answer = rig.pull(limits(2000, 10_000), limits(1000, 1000));
        assert_eq!(
            error_of(&answer),
            (500, "Download failed: took longer than 1 s")
        );
        rig.untouched();
    }

    #[test]
    fn a_check_through_real_sockets_fills_the_version() {
        let rig = Rig::new();
        let sup = rig.sup(Version::new(0, 2, 0), &image(5000));
        rig.serve(&rig.manifest(&sup, "0.2.0"), &sup);
        let ticket = rig.home.release_ticket(true, T0).unwrap();
        let response = ticket.check(&mut HttpFetcher::new(), &mut HostPacing, Limits::CHECK);
        assert_eq!(response.status, 200);
        let info: Value = serde_json::from_str(&response.body.unwrap()).unwrap();
        assert_eq!(info["latest"], "0.2.0");
        assert_eq!(info["updateAvailable"], true);
        let _ = HostPacing.now_ms();
    }
}
