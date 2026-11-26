//! Reading and dropping what is left of a request body, so that a refusal reaches the client.
//!
//! A refused upload (a restore with a wrong password, an update with a bad header) is answered
//! while most of its body is still on the way. A socket closed with unread received bytes ends
//! with a reset, and the client loses the answer. The box therefore reads the rest of the body
//! and drops it before it closes, within a bound.

use std::io::Read;

/// The most time (ms) spent reading the rest of a refused upload. It covers a 2 MB body over Wi-Fi
/// at a slow but steady rate, and a client that stalls or sends forever cannot keep the job's
/// thread longer: past it the connection closes (with a reset, as before).
pub const DRAIN_LIMIT_MS: u64 = 10_000;

/// The one buffer of a drain, on the stack (the jobs' threads have 10 KB and 16 KB of it).
pub const DRAIN_BUFFER_BYTES: usize = 1024;

/// Reads and drops up to `remaining` bytes of `reader`; returns how many were dropped. Stops at
/// the declared length, at the end of the reader, at its first error (a socket timeout included),
/// or once `now_ms` (a monotonic clock) is `limit_ms` past its first reading, checked between
/// two reads.
pub fn drain_body(
    reader: &mut dyn Read,
    remaining: usize,
    now_ms: &mut dyn FnMut() -> u64,
    limit_ms: u64,
) -> usize {
    let started = now_ms();
    let mut buffer = [0u8; DRAIN_BUFFER_BYTES];
    let mut dropped = 0;
    while dropped < remaining {
        if now_ms().saturating_sub(started) >= limit_ms {
            break;
        }
        let want = buffer.len().min(remaining - dropped);
        match reader.read(&mut buffer[..want]) {
            Ok(0) | Err(_) => break,
            Ok(count) => dropped += count,
        }
    }
    dropped
}
