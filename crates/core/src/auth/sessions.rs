//! The sessions (R2) and the attempts (R3.6), both in RAM only and both bounded.

use super::{
    ACCESS_TTL_S, ATTEMPT_WINDOW_MS, MAX_ATTEMPTS, MAX_SESSIONS, REFRESH_TTL_S, ct_eq, hex, unhex,
};
use crate::home::CLOCK_SET_AFTER_MS;

/// A token as the API writes it: 32 lowercase hexadecimal characters.
pub fn token_hex(bytes: &[u8; 16]) -> String {
    hex(bytes)
}

/// One login: an access token and a refresh token, each 16 random bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    access: [u8; 16],
    refresh: [u8; 16],
    /// Milliseconds since 1970, on the wall clock the home is given.
    opened_at: u64,
    access_until: u64,
    refresh_until: u64,
}

/// The open sessions, oldest first: at most `MAX_SESSIONS`.
#[derive(Debug, Default)]
pub struct Sessions {
    items: Vec<Session>,
}

/// The index of the session whose `token` (by `pick`) is `wanted`, looking at all of them in the
/// same time whatever the answer.
fn find(items: &[Session], wanted: &[u8; 16], pick: fn(&Session) -> &[u8; 16]) -> Option<usize> {
    let mut found = None;
    for (index, session) in items.iter().enumerate() {
        if ct_eq(pick(session), wanted) {
            found = Some(index);
        }
    }
    found
}

impl Sessions {
    pub fn new() -> Self {
        Sessions::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Opens a session at `now`; the ninth drops the one opened first.
    pub fn open(&mut self, access: [u8; 16], refresh: [u8; 16], now: u64) {
        if self.items.len() >= MAX_SESSIONS {
            self.items.remove(0);
        }
        self.items.push(Session {
            access,
            refresh,
            opened_at: now,
            access_until: now.saturating_add(ACCESS_TTL_S * 1000),
            refresh_until: now.saturating_add(REFRESH_TTL_S * 1000),
        });
    }

    /// True when `token` is the access token of a live session: opened by `now` and not past its
    /// end (the last millisecond of the lifetime is still valid).
    pub fn access_valid(&self, token: &str, now: u64) -> bool {
        let Some(bytes) = unhex::<16>(token) else {
            return false;
        };
        find(&self.items, &bytes, |s| &s.access)
            .map(|i| &self.items[i])
            .is_some_and(|s| s.opened_at <= now && now <= s.access_until)
    }

    /// Closes the session of this refresh token and says whether it was live (opened by `now`,
    /// not past its end): the caller then opens the next one (rotation). A token that is past its
    /// end is closed too.
    pub fn take_refresh(&mut self, token: &str, now: u64) -> bool {
        let Some(bytes) = unhex::<16>(token) else {
            return false;
        };
        let Some(index) = find(&self.items, &bytes, |s| &s.refresh) else {
            return false;
        };
        let session = self.items.remove(index);
        session.opened_at <= now && now <= session.refresh_until
    }

    /// Closes the session of this refresh token, if any.
    pub fn close(&mut self, token: &str) {
        let Some(bytes) = unhex::<16>(token) else {
            return;
        };
        if let Some(index) = find(&self.items, &bytes, |s| &s.refresh) {
            self.items.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The dates taken before the clock was set (still in 1970) move forward by the clock's jump
    /// (D45), as the devices' do.
    pub fn shift_before_clock(&mut self, jump: u64) {
        for session in &mut self.items {
            if session.opened_at < CLOCK_SET_AFTER_MS {
                session.opened_at = session.opened_at.saturating_add(jump);
                session.access_until = session.access_until.saturating_add(jump);
                session.refresh_until = session.refresh_until.saturating_add(jump);
            }
        }
    }

    /// A session opened after `now` (the clock went back) is expired: dropped.
    pub fn drop_after(&mut self, now: u64) {
        self.items.retain(|s| s.opened_at <= now);
    }

    /// The sessions whose refresh token ended are dropped (what no token can use anymore).
    pub fn drop_ended(&mut self, now: u64) {
        self.items.retain(|s| now <= s.refresh_until);
    }
}

/// The times of the last attempts that hashed, to refuse the eleventh within a minute: at most
/// `MAX_ATTEMPTS` kept.
#[derive(Debug, Default)]
pub struct Attempts {
    times: Vec<u64>,
}

impl Attempts {
    pub fn new() -> Self {
        Attempts::default()
    }

    pub fn len(&self) -> usize {
        self.times.len()
    }

    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// Notes an attempt at `now` and says whether it is accepted: false (and nothing noted) when
    /// `MAX_ATTEMPTS` were made less than `ATTEMPT_WINDOW_MS` ago. An attempt dated after `now`
    /// (the clock went back) is dropped, so it never blocks.
    pub fn allow(&mut self, now: u64) -> bool {
        self.times
            .retain(|&t| t <= now && now - t < ATTEMPT_WINDOW_MS);
        if self.times.len() >= MAX_ATTEMPTS {
            return false;
        }
        self.times.push(now);
        true
    }

    pub fn clear(&mut self) {
        self.times.clear();
    }

    pub fn shift_before_clock(&mut self, jump: u64) {
        for time in &mut self.times {
            if *time < CLOCK_SET_AFTER_MS {
                *time = time.saturating_add(jump);
            }
        }
    }

    pub fn drop_after(&mut self, now: u64) {
        self.times.retain(|&t| t <= now);
    }
}
