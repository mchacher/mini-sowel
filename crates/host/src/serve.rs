//! `--serve`: resume the network, keep the devices up to date from what they send, and serve
//! Sowel's device API on 127.0.0.1. The radio loop runs in the calling thread and is the only
//! writer of `devices.json`; one thread answers HTTP, with the devices behind a lock that is never
//! held during radio I/O or a file write.

use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use mini_sowel_core::api::{MAX_BODY_BYTES, Request, handle};
use mini_sowel_core::devices::{DeviceStore, Devices, lock};
use mini_sowel_core::radio::{Radio, RadioTransport, identity_lines, network_line};
use tiny_http::{Header, Response, Server};

use crate::store::{FileStore, Saver};
use crate::{backup, clock};

/// The server, so that Ctrl-C can unblock the HTTP thread.
static SERVER: OnceLock<Arc<Server>> = OnceLock::new();

/// Wakes the HTTP thread so that it ends; called by the Ctrl-C handler. Does nothing before the
/// server exists (`serve` checks the stop flag once it does).
pub fn unblock() {
    if let Some(server) = SERVER.get() {
        server.unblock();
    }
}

/// Milliseconds since 1970.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Binds the port on 127.0.0.1 only.
fn bind(port: u16) -> Result<Arc<Server>, String> {
    Server::http(("127.0.0.1", port))
        .map(Arc::new)
        .map_err(|e| format!("serve: cannot listen on 127.0.0.1:{port}: {e}"))
}

/// Requests answered at the same time; one more is told to come back (503).
const MAX_WORKERS: usize = 4;

/// Reads one request, asks the core, answers. Runs in a worker thread: a client that stalls
/// while sending its body holds this thread only.
fn answer(
    mut request: tiny_http::Request,
    devices: &Mutex<Devices>,
    port: u16,
    content_type: &Header,
) {
    let method = request.method().to_string();
    let path = request.url().to_string();
    let host = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.to_string());
    // One byte more than allowed is enough to see that the body is too long.
    let mut body = Vec::new();
    let _ = request
        .as_reader()
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body);
    let response = {
        let mut guard = lock(devices);
        handle(
            &mut guard,
            &Request {
                method: &method,
                path: &path,
                host: host.as_deref(),
                port,
                body: &body,
            },
            now_ms(),
        )
    };
    // The client may have gone: nothing to do about it.
    let _ = match response.body {
        Some(text) => request.respond(
            Response::from_string(text)
                .with_status_code(response.status)
                .with_header(content_type.clone()),
        ),
        None => request.respond(Response::empty(response.status)),
    };
}

/// Accepts requests and hands each to a worker thread (at most `MAX_WORKERS` at once), so that
/// a stalled client never blocks the others or the exit.
fn http_loop(server: &Server, devices: &Arc<Mutex<Devices>>, port: u16, content_type: &Header) {
    let busy = Arc::new(AtomicUsize::new(0));
    for request in server.incoming_requests() {
        if busy.fetch_add(1, Ordering::SeqCst) >= MAX_WORKERS {
            busy.fetch_sub(1, Ordering::SeqCst);
            let _ = request.respond(
                Response::from_string(r#"{"error":"Busy"}"#)
                    .with_status_code(503)
                    .with_header(content_type.clone()),
            );
            continue;
        }
        let (devices, worker_busy, content_type) =
            (Arc::clone(devices), Arc::clone(&busy), content_type.clone());
        let spawned = thread::Builder::new().spawn(move || {
            answer(request, &devices, port, &content_type);
            worker_busy.fetch_sub(1, Ordering::SeqCst);
        });
        if spawned.is_err() {
            busy.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Runs until Ctrl-C. `Ok(true)` when Ctrl-C stopped it.
pub fn serve<T: RadioTransport>(
    transport: T,
    label: &str,
    port: u16,
    verbose: bool,
    data: &std::path::Path,
    stop: &AtomicBool,
) -> Result<bool, String> {
    let store = FileStore::new(data);
    let devices = Arc::new(Mutex::new(Devices::from_list(store.load()?)));
    let server = bind(port)?;
    let _ = SERVER.set(Arc::clone(&server));
    let content_type = Header::from_bytes("Content-Type", "application/json; charset=utf-8")
        .map_err(|()| "serve: cannot build the Content-Type header".to_string())?;
    {
        let (server, devices) = (Arc::clone(&server), Arc::clone(&devices));
        thread::spawn(move || http_loop(&server, &devices, port, &content_type));
    }
    // Ctrl-C before the server existed: nobody unblocked it.
    if stop.load(Ordering::SeqCst) {
        server.unblock();
    }
    let mut saver = Saver::new(store, now_ms());
    let result = radio_loop(
        transport, label, port, verbose, data, stop, &devices, &mut saver,
    );
    // The store is written first, and the HTTP threads are never waited for: a client that
    // stalls cannot keep the host from saving and exiting.
    server.unblock();
    let saved = saver.tick(&devices, now_ms(), true);
    result.and_then(|stopped| saved.map(|()| stopped))
}

#[allow(clippy::too_many_arguments)]
fn radio_loop<T: RadioTransport>(
    transport: T,
    label: &str,
    port: u16,
    verbose: bool,
    data: &std::path::Path,
    stop: &AtomicBool,
    devices: &Mutex<Devices>,
    saver: &mut Saver,
) -> Result<bool, String> {
    let mut radio = Radio::new(transport, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(label, &identity);
    // The `network:` line of the identification is left out, as with `--listen`.
    for line in &lines[..3] {
        println!("{line}");
    }
    let network = radio.listen().map_err(|e| format!("radio: {e}"))?;
    let foreign = !backup::matches(data, &network);
    println!("{}", network_line(&network, foreign));
    println!("serving: http://127.0.0.1:{port}");
    while !stop.load(Ordering::SeqCst) {
        let events = radio.poll().map_err(|e| format!("radio: {e}"))?;
        let activity = radio.take_activity();
        let now = now_ms();
        {
            let mut guard = lock(devices);
            for item in &activity {
                guard.apply_activity(item, now);
            }
            for event in &events {
                guard.measures(event, now);
            }
            guard.refresh_status(now);
        }
        if verbose {
            for event in &events {
                for line in event.lines() {
                    println!("{line}");
                }
            }
        }
        saver.tick(devices, now, false)?;
    }
    Ok(true)
}
