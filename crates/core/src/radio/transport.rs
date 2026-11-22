//! The byte pipe between the core and the radio. The host implements it over TCP, the firmware
//! over the UART; the core never opens a port itself.

use std::fmt;

/// What can go wrong on the pipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The other end closed the connection.
    Closed,
    /// Any other failure, with the reason as text.
    Io(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Closed => write!(f, "connection closed by the radio"),
            TransportError::Io(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// A byte pipe to the radio.
pub trait RadioTransport {
    /// Reads at most `buf.len()` bytes, waiting at most `timeout_ms` milliseconds.
    /// Returns `Ok(0)` when nothing arrived in time (not an error).
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError>;

    /// Writes all the bytes.
    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError>;
}

/// One transport serves several `Radio`s in turn (a fresh one per try of the serial speed, per
/// restart): each borrows it for its life.
impl<T: RadioTransport + ?Sized> RadioTransport for &mut T {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        (**self).read(buf, timeout_ms)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        (**self).write(bytes)
    }
}
