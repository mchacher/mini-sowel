//! The radio's byte pipe over TCP: the Dongle Max's original firmware relays the radio's serial
//! line on port 6638.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use solite_core::radio::{RadioTransport, TransportError};

/// How long a connection attempt waits.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    /// Connects to `host:port`; the error is the reason as text.
    pub fn connect(address: &str) -> Result<Self, String> {
        let addresses = address.to_socket_addrs().map_err(|e| e.to_string())?;
        let mut last_error = String::from("no address found");
        for socket_address in addresses {
            match TcpStream::connect_timeout(&socket_address, CONNECT_TIMEOUT) {
                Ok(stream) => {
                    // Small frames: do not wait to batch them. Best effort: on macOS the option
                    // fails (EINVAL) when the radio has already closed, which the first read
                    // then reports as "connection closed".
                    let _ = stream.set_nodelay(true);
                    return Ok(TcpTransport { stream });
                }
                Err(e) => last_error = e.to_string(),
            }
        }
        Err(last_error)
    }
}

impl RadioTransport for TcpTransport {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        let timeout = Duration::from_millis(timeout_ms.max(1));
        if let Err(e) = self.stream.set_read_timeout(Some(timeout)) {
            // macOS answers EINVAL to a socket option once the peer has closed the connection.
            return Err(if e.kind() == ErrorKind::InvalidInput {
                TransportError::Closed
            } else {
                TransportError::Io(e.to_string())
            });
        }
        read_outcome(self.stream.read(buf))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.stream
            .write_all(bytes)
            .and_then(|()| self.stream.flush())
            .map_err(|e| TransportError::Io(e.to_string()))
    }
}

/// What a read of the radio's socket means. Nothing read in time, or a signal that interrupted
/// the read (Ctrl-C on Linux does), is "no byte yet": the caller then checks whether it must stop.
fn read_outcome(result: std::io::Result<usize>) -> Result<usize, TransportError> {
    match result {
        Ok(0) => Err(TransportError::Closed),
        Ok(n) => Ok(n),
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
            ) =>
        {
            Ok(0)
        }
        Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::BrokenPipe) => {
            Err(TransportError::Closed)
        }
        Err(e) => Err(TransportError::Io(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_interrupted_by_a_signal_is_no_byte_yet() {
        let interrupted = std::io::Error::from(ErrorKind::Interrupted);
        assert!(matches!(read_outcome(Err(interrupted)), Ok(0)));
    }
}
