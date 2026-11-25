//! The home's side of the account (spec 029): the account, the sessions and the attempts it keeps,
//! the presence that confirms a person at the box, `auth.json`'s change flag for the saver, and the
//! factory reset. The password hash itself is computed by the API, outside the lock.

use std::fmt;
use std::sync::Arc;

use super::{CLOCK_SET_AFTER_MS, Home, ListCursor};
use crate::auth::{
    ACCESS_TTL_S, Account, AuthTokens, NeverConfirmed, Presence, to_file_json, token_hex,
};
use crate::backup::{BackupState, BackupTicket};
use crate::devices::Id;
use crate::devices::Random;
use crate::failure::{Failure, Outcome};
use crate::live::{EngineEvent, Topic};
use crate::settings::Settings;
use crate::zones::ZoneUpdate;

/// The presence adapter the home holds: one, `NeverConfirmed` until the caller gives its own.
pub(super) struct PresenceSlot(Box<dyn Presence>);

impl Default for PresenceSlot {
    fn default() -> Self {
        PresenceSlot(Box::new(NeverConfirmed))
    }
}

impl fmt::Debug for PresenceSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PresenceSlot")
    }
}

/// Two tokens of 16 random bytes each.
fn draw_tokens(random: Random) -> Outcome<([u8; 16], [u8; 16])> {
    let mut access = [0u8; 16];
    let mut refresh = [0u8; 16];
    let drawn = random(&mut access).and_then(|()| random(&mut refresh));
    drawn.map_err(|e| {
        log::error!("no random bytes: {e}");
        Failure::new(500, "Internal error")
    })?;
    Ok((access, refresh))
}

impl Home {
    // ---- The account ----

    /// The account, if the setup was made.
    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    /// True when there is no account: the API asks for the setup.
    pub fn setup_required(&self) -> bool {
        self.account.is_none()
    }

    /// The account read from `auth.json` (`None`: no account).
    pub fn load_auth(&mut self, account: Option<Account>) {
        self.account = account;
    }

    /// The text of `auth.json`: the account and its hash, or `{"version":1}`.
    pub fn auth_json(&self) -> String {
        to_file_json(self.account.as_ref())
    }

    /// True when the account changed since `auth.json` was last taken.
    pub fn auth_changed(&self) -> bool {
        self.auth_changed
    }

    /// `auth.json` as a cursor of one chunk if the account changed, and the flag cleared.
    pub fn take_auth_cursor(&mut self) -> Option<ListCursor> {
        if !self.auth_changed {
            return None;
        }
        self.auth_changed = false;
        Some(ListCursor::saved_auth())
    }

    pub fn mark_auth_changed(&mut self) {
        self.auth_changed = true;
    }

    /// The last login since the start, in milliseconds since 1970 (`lastLoginAt`).
    pub fn last_login_at(&self) -> Option<u64> {
        self.last_login
    }

    // ---- The presence ----

    /// Gives the home its presence adapter (the box's button, the Mac's always-yes).
    pub fn set_presence(&mut self, presence: impl Presence + 'static) {
        self.presence = PresenceSlot(Box::new(presence));
    }

    /// True while a person at the box has confirmed.
    pub fn presence_confirmed(&self) -> bool {
        self.presence.0.confirmed()
    }

    /// What the person must do to confirm.
    pub fn presence_hint(&self) -> &str {
        self.presence.0.hint()
    }

    // ---- The sessions ----

    /// True when `token` is the access token of a live session at `now`.
    pub fn access_valid(&self, token: &str, now: u64) -> bool {
        self.sessions.access_valid(token, now)
    }

    /// Notes a setup or login attempt that will hash; false when `MAX_ATTEMPTS` were made in the
    /// last minute (the caller answers 429, without hashing).
    pub fn allow_attempt(&mut self, now: u64) -> bool {
        self.attempts.allow(now)
    }

    /// Opens a session for the account and answers its tokens, with `lastLoginAt` set to `now`.
    fn open_session(
        &mut self,
        tokens: ([u8; 16], [u8; 16]),
        now: u64,
        logged_in: bool,
    ) -> Option<AuthTokens> {
        let account = self.account.as_ref()?;
        self.sessions.open(tokens.0, tokens.1, now);
        if logged_in {
            self.last_login = Some(now);
        }
        Some(AuthTokens {
            access_token: token_hex(&tokens.0),
            refresh_token: token_hex(&tokens.1),
            expires_in: ACCESS_TTL_S,
            user: account.user(self.last_login),
        })
    }

    /// The end of the setup: the account is made (403 `Setup already completed` when one exists,
    /// which is how two setups at once end), a session opened, `auth.json` marked to be written.
    /// The tokens are drawn first: a random source that fails makes nothing (500).
    pub fn set_up(&mut self, account: Account, now: u64, random: Random) -> Outcome<AuthTokens> {
        if self.account.is_some() {
            return Err(Failure::new(403, "Setup already completed"));
        }
        let tokens = draw_tokens(random)?;
        self.account = Some(account);
        self.auth_changed = true;
        self.open_session(tokens, now, true)
            .ok_or_else(|| Failure::new(500, "Internal error"))
    }

    /// A login whose password was checked: a session opened. 401 `Invalid credentials` when there
    /// is no account (the account was reset meanwhile).
    pub fn log_in(&mut self, now: u64, random: Random) -> Outcome<AuthTokens> {
        if self.account.is_none() {
            return Err(Failure::new(401, "Invalid credentials"));
        }
        let tokens = draw_tokens(random)?;
        self.open_session(tokens, now, true)
            .ok_or_else(|| Failure::new(500, "Internal error"))
    }

    /// `POST /auth/refresh`: a live refresh token closes its session and opens a new one
    /// (rotation); anything else is 401 `Invalid refresh token`.
    pub fn refresh_session(
        &mut self,
        refresh_token: &str,
        now: u64,
        random: Random,
    ) -> Outcome<AuthTokens> {
        let invalid = || Failure::new(401, "Invalid refresh token");
        if self.account.is_none() {
            return Err(invalid());
        }
        let tokens = draw_tokens(random)?;
        if !self.sessions.take_refresh(refresh_token, now) {
            return Err(invalid());
        }
        self.open_session(tokens, now, false).ok_or_else(invalid)
    }

    /// `POST /auth/logout`: closes the session of this refresh token (its access token with it).
    pub fn close_session(&mut self, refresh_token: &str) {
        self.sessions.close(refresh_token);
    }

    /// The dates the account keeps, taken before the clock was set, move by its jump (D45); the
    /// account's `createdAt` changes, so `auth.json` is written again.
    pub(super) fn shift_auth_before_clock(&mut self, jump: u64) {
        self.sessions.shift_before_clock(jump);
        self.attempts.shift_before_clock(jump);
        if let Some(at) = self.last_login.as_mut()
            && *at < CLOCK_SET_AFTER_MS
        {
            *at = at.saturating_add(jump);
        }
        if let Some(account) = self.account.as_mut()
            && account.created_at < CLOCK_SET_AFTER_MS
        {
            account.created_at = account.created_at.saturating_add(jump);
            self.auth_changed = true;
        }
    }

    // ---- The factory reset ----

    /// True while the pairing window is open, or devices that joined are still being set up
    /// after it closed (or an opening was asked and not taken yet).
    pub fn pairing_in_progress(&self) -> bool {
        let window = self.pairing();
        window.phase() != super::Phase::Closed || window.state(window.now()).0
    }

    /// The factory reset (R7), the core function the button's long press calls: 409 `Pairing in
    /// progress` while pairing runs; else every recipe instance, equipment, device (forgotten,
    /// they stay in the Zigbee network) and zone but the root is deleted as the API's deletions
    /// do (with their live events), the root's name and description are a first start's, the
    /// settings are emptied, the account and every session are gone. Every file is written by the
    /// saver. The Zigbee network (`zigbee.json`) and the Wi-Fi settings stay.
    pub fn factory_reset(&mut self, now: u64) -> Outcome<()> {
        if self.pairing_in_progress() {
            return Err(Failure::new(409, "Pairing in progress"));
        }
        let recipes: Vec<Id> = self.recipes().instances().map(|i| i.id).collect();
        for id in recipes {
            if let Err(e) = self.delete_recipe_instance(&id.to_string()) {
                log::warn!("factory reset: recipe instance {id}: {}", e.message);
            }
        }
        let equipments: Vec<Id> = self
            .equipments()
            .list()
            .iter()
            .map(|e| e.equipment.id)
            .collect();
        for id in equipments {
            if let Err(e) = self.delete_equipment(&id.to_string(), now) {
                log::warn!("factory reset: equipment {id}: {}", e.message);
            }
        }
        let devices: Vec<Id> = self.devices.list().iter().map(|d| d.device.id).collect();
        for id in devices {
            self.remove_device(&id.to_string(), now);
        }
        // Leaves first: a zone with children is refused. At most `MAX_ZONES` passes.
        loop {
            let leaf = self
                .zones()
                .list()
                .iter()
                .find(|z| {
                    z.id != Id::ROOT_ZONE
                        && !self
                            .zones()
                            .list()
                            .iter()
                            .any(|o| o.parent_id == Some(z.id))
                })
                .map(|z| z.id);
            let Some(leaf) = leaf else {
                break;
            };
            if let Err(e) = self.delete_zone(&leaf.to_string(), now) {
                log::warn!("factory reset: zone {leaf}: {}", e.message);
                break;
            }
        }
        let first_start = self.zones().get(&Id::ROOT_ZONE).is_some_and(|root| {
            root.name == "Maison"
                && root.icon.as_deref() == Some("home")
                && root.description.is_none()
        });
        if !first_start {
            let reset = ZoneUpdate {
                name: Some("Maison".into()),
                icon: Some(Some("home".into())),
                description: Some(None),
                ..Default::default()
            };
            if let Err(e) = self.update_zone(&Id::ROOT_ZONE.to_string(), reset, now) {
                log::warn!("factory reset: root zone: {}", e.message);
            }
        }
        let keys: Vec<String> = self.settings.to_map().keys().cloned().collect();
        if !keys.is_empty() {
            self.settings = Settings::default();
            self.settings_changed = true;
            self.forget_sunlight();
            self.live
                .emit(Topic::System, || EngineEvent::SettingsChanged { keys });
        }
        self.account = None;
        self.sessions.clear();
        self.attempts.clear();
        self.last_login = None;
        self.auth_changed = true;
        self.recompute(now);
        Ok(())
    }

    // ---- Backup (spec 035) ----

    /// The backup state, shared with the saver (it writes no file while an export reads them).
    pub fn backup_state(&self) -> Arc<BackupState> {
        Arc::clone(&self.backup)
    }

    /// Starts a backup job: 409 `A restore waits for the restart` or `Backup already in
    /// progress`. The ticket ends the job when it is dropped.
    pub fn backup_begin(&self, restore: bool) -> Outcome<BackupTicket> {
        self.backup.begin(restore)
    }
}
