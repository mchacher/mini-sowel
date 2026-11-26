//! The home's part of the release check and pull (spec 039): the tickets, taken under the lock.

use std::sync::Arc;

use super::Home;
use crate::failure::Outcome;
use crate::settings;
use crate::update::UpdateStatus;
use crate::update::release::{DEFAULT_MANIFEST_URL, ReleaseTicket, Releases};

impl Home {
    /// The releases state, shared with the tickets (`GET /system/version` reads its cache).
    pub fn releases(&self) -> Arc<Releases> {
        Arc::clone(&self.releases)
    }

    /// The program's updates share their status with the home (spec 043), so the restart route
    /// knows of an installation and of a probation.
    pub fn set_update_status(&mut self, status: Arc<UpdateStatus>) {
        self.update_status = status;
    }

    /// Why a restart now is refused, as the 409's text (spec 043, R1.2): an update received or
    /// installed, an update check, a backup, or the running image on probation. `None`: an update
    /// or a restore that waits for its restart does not refuse, the box restarts anyway.
    pub fn restart_refusal(&self) -> Option<&'static str> {
        self.update_status
            .restart_refusal()
            .or_else(|| self.releases.busy_message())
            .or_else(|| {
                (self.backup_state().phase() != crate::backup::Phase::Idle
                    && !self.backup_state().is_pending())
                .then_some("Backup already in progress")
            })
    }

    /// The manifest's address: the setting, or the default when unset or empty.
    fn manifest_url(&self) -> String {
        self.settings
            .get(settings::MANIFEST_URL)
            .filter(|url| !url.is_empty())
            .unwrap_or(DEFAULT_MANIFEST_URL)
            .to_string()
    }

    /// The right to check (`check`) or pull (not): 503 while the clock is not set, 409 while a
    /// check or a pull runs. `now` is the wall clock.
    pub fn release_ticket(&self, check: bool, now: u64) -> Outcome<ReleaseTicket> {
        let url = self.manifest_url();
        if check {
            self.releases.begin_check(&url, now)
        } else {
            self.releases.begin_pull(&url, now)
        }
    }

    /// The automatic check's ticket (R6): `Some` when the setting `update.autoCheck` is on and
    /// the check is due (`mono_ms`, the monotonic clock), the clock set and nothing running.
    pub fn auto_check_ticket(&self, wall_ms: u64, mono_ms: u64) -> Option<ReleaseTicket> {
        let enabled = self.settings.get(settings::AUTO_CHECK) == Some("true");
        self.releases
            .auto_ticket(enabled, &self.manifest_url(), wall_ms, mono_ms)
    }

    /// The automatic check's job could not start: ask again in a minute (R6).
    pub fn auto_check_postpone(&self, mono_ms: u64) {
        self.releases.postpone_auto(mono_ms);
    }
}
