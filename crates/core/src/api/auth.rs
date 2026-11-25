//! Sowel's authentication routes (`src/api/routes/auth.ts`: status, setup, login, refresh,
//! logout) and the gate every other route goes through (`src/auth/auth-middleware.ts`), spec 029.
//! The two routes that hash a password work in three phases so that the home's lock is never held
//! while the hash runs (R4.4): `prepare` under the lock (checks, presence, rate limit), the hash
//! with no lock, `finish` under the lock again.

use std::sync::Mutex;

use serde_json::json;

use super::input::{self, Fields};
use super::{Request, Response, error, failure, json, method_not_allowed, no_content};
use crate::auth::{
    Account, Language, MAX_PASSWORD_BYTES, PBKDF2_ITERATIONS, PasswordHash, ct_eq, display_name_ok,
    password_ok, pbkdf2_sha256, username_ok,
};
use crate::devices::{Id, Random, lock};
use crate::failure::{Failure, Outcome};
use crate::home::{Home, new_id};

/// How the three phases reach the home: the plain `handle` has it (`Direct`), the guarded handler
/// has its lock and takes it phase by phase (`Shared`).
pub(super) enum Access<'a> {
    Direct(&'a mut Home),
    Shared(&'a Mutex<Home>),
}

impl Access<'_> {
    fn with<R>(&mut self, f: impl FnOnce(&mut Home) -> R) -> R {
        match self {
            Access::Direct(home) => f(home),
            Access::Shared(home) => f(&mut lock(home)),
        }
    }
}

/// True for the four routes that need no token (Sowel's `PUBLIC_ROUTES`): `/auth/status`,
/// `/auth/setup`, `/auth/login`, `/auth/refresh`, whatever the method (a wrong method is 405).
pub(super) fn is_public(segments: &[&str]) -> bool {
    matches!(segments, ["auth", "status" | "setup" | "login" | "refresh"])
}

/// 403 `Setup required`: there is no account yet.
fn setup_required() -> Response {
    Response {
        status: 403,
        body: Some(json!({"error": "Setup required", "setupRequired": true}).to_string()),
    }
}

/// 403 with the presence's hint: a person at the box has not confirmed (Solite's, D48).
pub(super) fn confirm_required(hint: &str) -> Response {
    Response {
        status: 403,
        body: Some(json!({"error": hint, "confirmRequired": true}).to_string()),
    }
}

fn rate_limited() -> Response {
    error(429, "Rate limit exceeded, retry in 1 minute")
}

fn invalid_credentials() -> Response {
    error(401, "Invalid credentials")
}

/// Sowel's gate, in its order: no account 403 `Setup required`; no `Authorization: Bearer
/// <token>` 401 `Authentication required`; a token that is not a live access token 401 `Invalid
/// or expired token`. `None`: the request may go on.
pub(super) fn gate(home: &mut Home, request: &Request, now: u64) -> Option<Response> {
    // The clock may have been set since the last turn: sessions move first (D45).
    home.observe_wall(now);
    if home.setup_required() {
        return Some(setup_required());
    }
    let token = request
        .authorization
        .and_then(|header| header.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty());
    let Some(token) = token else {
        return Some(error(401, "Authentication required"));
    };
    if home.access_valid(token, now) {
        None
    } else {
        Some(error(401, "Invalid or expired token"))
    }
}

/// The five routes. `logout` is behind the gate when the caller is the guarded handler; the plain
/// `handle` has no gate.
pub(super) fn route(
    access: &mut Access,
    segments: &[&str],
    request: &Request,
    now: u64,
    random: Random,
) -> Response {
    let ["auth", action] = segments else {
        return error(404, "Not found");
    };
    let expect = match *action {
        "status" => "GET",
        "setup" | "login" | "refresh" | "logout" => "POST",
        _ => return error(404, "Not found"),
    };
    if request.method != expect {
        return method_not_allowed();
    }
    match *action {
        "status" => access.with(|home| {
            home.observe_wall(now);
            json(200, &json!({"setupRequired": home.setup_required()}))
        }),
        "setup" => match access.with(|home| prepare_setup(home, request, now, &mut *random)) {
            Prepared::Done(response) => response,
            Prepared::Hash(job) => finish(access, job.hash(), now, random),
        },
        "login" => match access.with(|home| prepare_login(home, request, now)) {
            Prepared::Done(response) => response,
            Prepared::Hash(job) => finish(access, job.hash(), now, random),
        },
        "refresh" => access.with(|home| refresh(home, request.body, now, random)),
        _ => access.with(|home| logout(home, request.body, now)),
    }
}

// ---- Setup and login: prepare, hash, finish ----

enum Prepared {
    Done(Response),
    Hash(Job),
}

/// What the hash needs, taken under the lock.
enum Job {
    Setup {
        id: Id,
        username: String,
        display_name: String,
        language: Language,
        password: String,
        salt: [u8; 16],
    },
    Login {
        /// The account the password is checked against, and its hash (a fixed one when there is
        /// no account or the user name is wrong: the same cost).
        account: Option<Id>,
        hash: PasswordHash,
        user_matches: bool,
        password: String,
    },
}

/// What the hash gave.
enum Hashed {
    Setup(Account),
    Login { account: Option<Id>, ok: bool },
}

impl Job {
    /// The slow part, with no lock held.
    fn hash(self) -> Hashed {
        match self {
            Job::Setup {
                id,
                username,
                display_name,
                language,
                password,
                salt,
            } => Hashed::Setup(Account {
                id,
                username,
                display_name,
                language,
                // Set under the lock, at the end of the setup.
                created_at: 0,
                password: PasswordHash::new(&password, salt),
            }),
            Job::Login {
                account,
                hash,
                user_matches,
                password,
            } => {
                let computed = pbkdf2_sha256(password.as_bytes(), &hash.salt, hash.iterations);
                let same = ct_eq(&computed, &hash.hash);
                Hashed::Login {
                    account,
                    ok: same & user_matches & account.is_some(),
                }
            }
        }
    }
}

/// A body field that must be a string.
fn required_string(fields: &Fields, field: &str) -> Outcome<String> {
    input::string(fields, field)?.ok_or_else(|| input::missing_field(field))
}

fn prepare_setup(home: &mut Home, request: &Request, now: u64, random: Random) -> Prepared {
    home.observe_wall(now);
    if !home.setup_required() {
        return Prepared::Done(error(403, "Setup already completed"));
    }
    let checked = (|| -> Outcome<(String, String, String, Language)> {
        let fields = input::object(request.body)?;
        input::refuse_unknown(
            &fields,
            &["username", "password", "displayName", "language"],
        )?;
        let username = required_string(&fields, "username")?;
        if !username_ok(&username) {
            return Err(input::invalid_field("username"));
        }
        let password = required_string(&fields, "password")?;
        if !password_ok(&password) {
            return Err(input::invalid_field("password"));
        }
        let display_name = required_string(&fields, "displayName")?;
        if !display_name_ok(&display_name) {
            return Err(input::invalid_field("displayName"));
        }
        let language = match input::string(&fields, "language")? {
            None => Language::Fr,
            Some(text) => Language::parse(&text).ok_or_else(|| input::invalid_field("language"))?,
        };
        Ok((username, password, display_name, language))
    })();
    let (username, password, display_name, language) = match checked {
        Ok(fields) => fields,
        Err(e) => return Prepared::Done(failure(&e)),
    };
    if !home.presence_confirmed() {
        return Prepared::Done(confirm_required(home.presence_hint()));
    }
    if !home.allow_attempt(now) {
        return Prepared::Done(rate_limited());
    }
    let mut salt = [0u8; 16];
    let drawn = new_id(&mut *random).and_then(|id| {
        random(&mut salt).map_err(|e| {
            log::error!("no random bytes: {e}");
            Failure::new(500, "Internal error")
        })?;
        Ok(id)
    });
    match drawn {
        Ok(id) => Prepared::Hash(Job::Setup {
            id,
            username,
            display_name,
            language,
            password,
            salt,
        }),
        Err(e) => Prepared::Done(failure(&e)),
    }
}

fn prepare_login(home: &mut Home, request: &Request, now: u64) -> Prepared {
    home.observe_wall(now);
    let checked = (|| -> Outcome<(String, String)> {
        let fields = input::object(request.body)?;
        // `trustedDeviceToken` is Sowel's (MFA): accepted, ignored.
        input::refuse_unknown(&fields, &["username", "password", "trustedDeviceToken"])?;
        Ok((
            required_string(&fields, "username")?,
            required_string(&fields, "password")?,
        ))
    })();
    let (username, password) = match checked {
        Ok(fields) => fields,
        Err(e) => return Prepared::Done(failure(&e)),
    };
    // No password of this size was ever accepted: no need to hash it (nor to count it).
    if password.len() > MAX_PASSWORD_BYTES {
        return Prepared::Done(invalid_credentials());
    }
    if !home.allow_attempt(now) {
        return Prepared::Done(rate_limited());
    }
    let job = match home.account() {
        Some(account) => Job::Login {
            account: Some(account.id),
            hash: account.password.clone(),
            user_matches: ct_eq(account.username.as_bytes(), username.as_bytes()),
            password,
        },
        None => Job::Login {
            account: None,
            hash: PasswordHash {
                iterations: PBKDF2_ITERATIONS,
                salt: [0; 16],
                hash: [0; 32],
            },
            user_matches: false,
            password,
        },
    };
    Prepared::Hash(job)
}

/// The end of a setup or a login, under the lock again.
fn finish(access: &mut Access, hashed: Hashed, now: u64, random: Random) -> Response {
    // A setup makes an account (201), a login answers the tokens (200).
    let status = if matches!(hashed, Hashed::Setup(_)) {
        201
    } else {
        200
    };
    access.with(|home| {
        home.observe_wall(now);
        let tokens = match hashed {
            Hashed::Setup(mut account) => {
                account.created_at = now;
                home.set_up(account, now, random)
            }
            Hashed::Login { account, ok } => {
                // The account the password was checked against is still the home's.
                if ok && home.account().map(|a| a.id) == account {
                    home.log_in(now, random)
                } else {
                    Err(Failure::new(401, "Invalid credentials"))
                }
            }
        };
        match tokens {
            Ok(tokens) => json(status, &tokens),
            Err(e) => failure(&e),
        }
    })
}

// ---- Refresh and logout ----

fn refresh(home: &mut Home, body: &[u8], now: u64, random: Random) -> Response {
    home.observe_wall(now);
    let token = (|| -> Outcome<String> {
        let fields = input::object(body)?;
        input::refuse_unknown(&fields, &["refreshToken"])?;
        required_string(&fields, "refreshToken")
    })();
    let token = match token {
        Ok(token) => token,
        Err(e) => return failure(&e),
    };
    match home.refresh_session(&token, now, random) {
        Ok(tokens) => json(200, &tokens),
        Err(e) => failure(&e),
    }
}

/// An empty body is `{}`.
fn optional_object(body: &[u8]) -> Outcome<Fields> {
    if body.is_empty() {
        Ok(Fields::new())
    } else {
        input::object(body)
    }
}

fn logout(home: &mut Home, body: &[u8], now: u64) -> Response {
    home.observe_wall(now);
    let token = (|| -> Outcome<Option<String>> {
        let fields = optional_object(body)?;
        input::refuse_unknown(&fields, &["refreshToken"])?;
        input::string(&fields, "refreshToken")
    })();
    match token {
        Ok(Some(token)) => home.close_session(&token),
        Ok(None) => {}
        Err(e) => return failure(&e),
    }
    no_content()
}

/// The body of a route that takes none or `{}` (the factory reset): any field is unknown.
pub(super) fn empty_body(body: &[u8]) -> Outcome<()> {
    let fields = optional_object(body)?;
    input::refuse_unknown(&fields, &[])
}
