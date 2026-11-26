//! Dropping what is left of a refused upload's body (the box does it before it closes, so that
//! the client reads the refusal instead of a reset).

use std::cell::Cell;
use std::io::{Error, Read, Result};

use solite_core::api::drain::{DRAIN_BUFFER_BYTES, DRAIN_LIMIT_MS, drain_body};

/// A reader of `left` bytes, at most 700 per read, that then fails or ends; it records what it
/// was asked.
struct Body {
    left: usize,
    fail_when_empty: bool,
    asked: usize,
    biggest: usize,
}

impl Body {
    fn new(left: usize, fail_when_empty: bool) -> Self {
        Body {
            left,
            fail_when_empty,
            asked: 0,
            biggest: 0,
        }
    }
}

impl Read for Body {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        self.biggest = self.biggest.max(buffer.len());
        if self.left == 0 && self.fail_when_empty {
            return Err(Error::other("timeout"));
        }
        let count = buffer.len().min(self.left).min(700);
        self.left -= count;
        self.asked += count;
        Ok(count)
    }
}

fn frozen() -> impl FnMut() -> u64 {
    || 0
}

#[test]
fn the_whole_remaining_body_is_drained() {
    let mut body = Body::new(5000, false);
    let dropped = drain_body(&mut body, 5000, &mut frozen(), DRAIN_LIMIT_MS);
    assert_eq!(dropped, 5000);
    assert_eq!(body.left, 0);
    assert!(body.biggest <= DRAIN_BUFFER_BYTES);
}

#[test]
fn a_reader_that_times_out_halfway_stops_the_drain() {
    let mut body = Body::new(2000, true);
    let dropped = drain_body(&mut body, 5000, &mut frozen(), DRAIN_LIMIT_MS);
    assert_eq!(dropped, 2000);
}

#[test]
fn an_endless_reader_is_stopped_by_the_deadline() {
    struct Endless;
    impl Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
            Ok(buffer.len())
        }
    }
    let clock = Cell::new(0u64);
    let mut now = || {
        clock.set(clock.get() + 100);
        clock.get()
    };
    let dropped = drain_body(&mut Endless, usize::MAX, &mut now, DRAIN_LIMIT_MS);
    assert!(dropped > 0);
    // About 100 readings of the clock fit in the limit: a bounded number of reads.
    assert!(dropped <= 101 * DRAIN_BUFFER_BYTES, "dropped {dropped}");
}

#[test]
fn nothing_is_read_when_nothing_remains() {
    let mut body = Body::new(100, false);
    assert_eq!(drain_body(&mut body, 0, &mut frozen(), DRAIN_LIMIT_MS), 0);
    assert_eq!(body.asked, 0);
    assert_eq!(body.biggest, 0);
}

#[test]
fn never_more_than_the_declared_length_is_read() {
    let mut body = Body::new(10_000, false);
    let dropped = drain_body(&mut body, 1500, &mut frozen(), DRAIN_LIMIT_MS);
    assert_eq!(dropped, 1500);
    assert_eq!(body.asked, 1500);
}
