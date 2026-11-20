//! The host program against a fake radio listening on a local TCP port, speaking real ASH.

#[path = "../../core/tests/common/mod.rs"]
mod common;

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::process::{Command, Output};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use common::{FakeNcp, Mode};

/// Serves one connection with the fake radio; returns every byte it received.
fn serve(ncp: FakeNcp) -> (SocketAddr, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut ncp = ncp;
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut buf = [0u8; 512];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let answer = ncp.on_bytes(&buf[..n]);
                    if !answer.is_empty() && stream.write_all(&answer).is_err() {
                        break;
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        ncp.received
    });
    (address, handle)
}

fn run(args: &[&str]) -> (Output, Duration) {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .args(args)
        .output()
        .expect("the host program starts");
    (output, start.elapsed())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8 output")
}

#[test]
fn radio_argument_shows_the_four_lines_of_the_fake_radio_and_exits_0() {
    let (address, handle) = serve(FakeNcp::new());
    let url = format!("tcp://{address}");
    let (output, elapsed) = run(&["--radio", &url]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let expected = format!(
        "{}\nradio: {url}\nezsp: protocol 13, stack type 2, EmberZNet 7.4.4.0\n\
         radio address: 0x8C6FB9FFFE123456\nnetwork: no network\n",
        mini_sowel_core::banner()
    );
    assert_eq!(text(&output.stdout), expected);
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    // The host started ASH with RST, cancel byte first, as UG101 does.
    let received = handle.join().unwrap();
    assert_eq!(&received[..5], &[0x1A, 0xC0, 0x38, 0xBC, 0x7E]);
}

#[test]
fn a_refused_connection_prints_cannot_connect_and_exits_1() {
    // Bind then drop: the port is closed.
    let address = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let (output, elapsed) = run(&["--radio", &format!("tcp://{address}")]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with(&format!("radio: cannot connect to {address}: ")),
        "{stderr}"
    );
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
}

#[test]
fn a_radio_that_never_answers_prints_no_answer_to_reset_and_exits_1() {
    let mut ncp = FakeNcp::new();
    ncp.mode = Mode::Silent;
    let (address, handle) = serve(ncp);
    let (output, elapsed) = run(&["--radio", &format!("tcp://{address}")]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stderr).trim(), "radio: no answer to reset");
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    handle.join().unwrap();
}

#[test]
fn an_error_frame_after_rstack_prints_the_code_and_exits_1() {
    let mut ncp = FakeNcp::new();
    ncp.mode = Mode::ErrorAfterRstAck(0x51);
    let (address, handle) = serve(ncp);
    let (output, elapsed) = run(&["--radio", &format!("tcp://{address}")]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stderr).trim(), "radio: error 0x51");
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    handle.join().unwrap();
}

#[test]
fn a_malformed_radio_argument_prints_the_expected_form_and_exits_2() {
    for args in [
        &["--radio", "nonsense"][..],
        &["--radio", "tcp://host"],
        &["--radio", "tcp://host:notaport"],
        &["--radio"],
        &["--other"],
    ] {
        let (output, _) = run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            text(&output.stderr).contains("--radio tcp://<address>:<port>"),
            "{args:?}"
        );
    }
}

#[test]
fn without_argument_the_banner_only_is_printed_and_the_exit_is_0() {
    let (output, _) = run(&[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        format!("{}\n", mini_sowel_core::banner())
    );
}

#[test]
fn a_radio_that_accepts_then_closes_prints_connection_closed_and_exits_1() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
    });
    let (output, elapsed) = run(&["--radio", &format!("tcp://{address}")]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stderr).trim(),
        "radio: connection closed by the radio"
    );
    assert!(elapsed < Duration::from_secs(10), "{elapsed:?}");
    handle.join().unwrap();
}
