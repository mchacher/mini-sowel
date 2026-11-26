//! The box's fetch adapter (feature `radio`, spec 039 R8): one GET through ESP-IDF's HTTP client
//! (`EspHttpConnection`), https with the common certificate bundle (`esp_crt_bundle_attach`),
//! redirects left to the core (`FollowNone`). The connection lives inside the body returned, so
//! the core may ask for the next address once the redirect's answer is dropped: one session at
//! a time (the core's 409), its record buffers in PSRAM. The buffers of the client itself, 4096
//! bytes to receive (GitHub's redirect `Location`, a signed address of several hundred characters,
//! is over the default 512) and 1024 to send, are in internal RAM (`docs/memory-budget.md`).

use std::io::{self, Read};
use std::time::Duration;

use esp_idf_svc::http::Method;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection, FollowRedirectsPolicy};
use esp_idf_svc::sys::{EspError, esp_crt_bundle_attach};
use solite_core::update::release::{Fetched, Fetcher};

/// The client's receive buffer, in bytes.
const RX_BUFFER_BYTES: usize = 4096;
/// The client's send buffer, in bytes: a request line, two headers and the host.
const TX_BUFFER_BYTES: usize = 1024;

/// The box's `Fetcher`.
pub struct EspFetcher;

/// The body of a 200: the connection, read to its end.
struct Body {
    connection: EspHttpConnection,
}

impl Read for Body {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.connection.read(buffer).map_err(io_error)
    }
}

/// A read error as an `io::Error`; a timeout keeps its kind (the core words it `timed out`).
fn io_error(e: EspError) -> io::Error {
    let text = e.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("eagain") {
        io::Error::new(io::ErrorKind::TimedOut, text)
    } else {
        io::Error::other(text)
    }
}

impl Fetcher for EspFetcher {
    fn get(&mut self, url: &str, idle_ms: u64) -> Result<Fetched, String> {
        let configuration = Configuration {
            buffer_size: Some(RX_BUFFER_BYTES),
            buffer_size_tx: Some(TX_BUFFER_BYTES),
            timeout: Some(Duration::from_millis(idle_ms)),
            follow_redirects_policy: FollowRedirectsPolicy::FollowNone,
            crt_bundle_attach: Some(esp_crt_bundle_attach),
            ..Default::default()
        };
        let mut connection =
            EspHttpConnection::new(&configuration).map_err(|e| format!("cannot start: {e}"))?;
        let agent = format!("solite/{}", solite_core::VERSION);
        let headers = [("User-Agent", agent.as_str()), ("Accept", "*/*")];
        connection
            .initiate_request(Method::Get, url, &headers)
            .map_err(|e| e.to_string())?;
        connection.initiate_response().map_err(|e| e.to_string())?;
        match connection.status() {
            200 => {
                let length = connection
                    .header("Content-Length")
                    .and_then(|text| text.trim().parse::<u64>().ok());
                Ok(Fetched::Body {
                    length,
                    body: Box::new(Body { connection }),
                })
            }
            301 | 302 | 303 | 307 | 308 => Ok(Fetched::Redirect(
                connection.header("Location").map(str::to_string),
            )),
            status => Ok(Fetched::Status(status)),
        }
    }
}
