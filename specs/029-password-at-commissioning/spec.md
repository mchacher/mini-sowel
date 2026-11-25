# Spec 029: password-at-commissioning

## Context

Roadmap J4 (milestone J, the autonomous box), second item of the "Order of work while the box is
away" (2026-11-25): "A password at commissioning, confirmed by the box's button (EN 18031: no
common default password); factory reset. Done when: the box asks for a password on first start."

Today the API and `/ws` have no authentication (D13): the Mac host serves on `127.0.0.1` only, and
the box, which serves its home network (D33), keeps the pairing route off (404, D28, D36) until a
password exists. Two inbox lines wait for this increment: Sowel's bearer token on the box's `/ws`
(only `bearer.none` is echoed), and the pairing windows of the box to be replaced by the button or
the authenticated route.

EN 18031-1 (radio equipment, network-connected) asks for an authentication mechanism without a
common default password, protected against brute force; EN 18031-2 asks that the user can delete
their data. Sowel's authentication (users, JWT access token and refresh token, first-run setup)
is the model to mirror, as a subset (D43).

This run is autonomous (the user is away): the open questions are decided here and recorded
(decisions D47 to D49, and "Decisions taken in this spec" below).

## Goals

1. A box (or a Mac host) with no password asks for one: the first person to open the interface
   sets a user name and a password, confirmed by a physical action at the box; there is no
   default password.
2. Every route of the API, and `/ws`, needs Sowel's bearer token, obtained by logging in with
   that password (Sowel's routes, bodies and answers).
3. The pairing route works on the box once authenticated (the 404 of D28 goes).
4. A factory reset erases the home and the password: the box is back at its first start.
5. The button's confirmation is an adapter of the core; the box's real button is wired later.

## Out of scope

- Several users, roles (`standard`), API tokens (`swl_`), MFA, trusted devices, `/api/v1/me`,
  `/api/v1/users`, password change (`PUT /api/v1/me/password`): one account, the admin. Password
  change goes to the roadmap inbox.
- The real button of the Dongle Max (its pin, short press, long press): wired on the box later;
  until then the firmware's confirmation is the 10 minutes after a power-on (R6.3). A box check
  pending.
- A factory reset that also leaves the Zigbee network and forgets its keys (`/data/zigbee.json`):
  the radio network is kept (inbox line). A factory reset without the password (forgotten
  password): it needs the button's long press, wired later; until then the data partition is
  erased by a flash without `--keep-storage`.
- Erasing the Wi-Fi settings (`wifi.json`) at a factory reset (inbox line).
- The ESP32's SHA accelerator for the hash (more iterations): an adapter later (inbox line).
- HTTPS: the box serves HTTP on the home network (D33); tokens travel in clear on the LAN.
- Removing the power-on pairing window and the console's `pair` (D32, D36): they stay until the
  button is wired (the inbox line is rewritten).
- A per-address rate limit (the core does not see the client's address): one limit for the box.

## Requirements

### R1: the account and its password

1. The home has at most one account: an id (UUID v4), a `username` (1 to 64 characters, no
   control character), a `displayName` (1 to 64 characters), a `language` (`fr` or `en`, `fr`
   when missing, as Sowel), a `createdAt` (ISO 8601), and its password hash. Its role is always
   `admin`.
2. **The hash**: PBKDF2-HMAC-SHA256 (RFC 8018), a 16-byte salt from the caller's random source,
   `PBKDF2_ITERATIONS` = 10 000 iterations, a 32-byte result. The iteration count is stored with
   the hash, so a later change still checks the old hashes. Passwords: 8 to 128 characters
   (Unicode scalar values) and at most 512 bytes of UTF-8 (Sowel: at least 6; 8 is the floor of
   NIST SP 800-63B, D47); a password outside is refused with 400 `Invalid field: password`.
3. **Cost, reasoned for the ESP32** (D47): one iteration is two SHA-256 compressions once HMAC's
   keys are prepared; in software on the ESP32 at 240 MHz a compression takes about 25 to 50 µs,
   so a hash takes about 0.5 to 1 s, on the HTTP worker that asked, never under the home's lock
   (R4.4). Memory: under 1 KB of stack, nothing on the heap that grows. On the Mac: a few
   milliseconds. The time of one login on the box is a box check (pending).
4. **Storage**: `auth.json` in the data partition (the host's data folder on the Mac), written by
   the engine's saver like `settings.json` (D40's 2 s delay; one write per setup or reset, none per
   login): `{"version":1,"user":{"id","username","displayName","language","createdAt"},
   "password":{"scheme":"pbkdf2-sha256","iterations":10000,"salt":"<32 hex>","hash":"<64 hex>"}}`.
   No account: the file is missing, or `{"version":1}` (after a factory reset). A file that cannot
   be read or parsed, a `version` other than 1, a `user` with a missing or invalid field (R1.1's
   bounds), a `scheme` other than `pbkdf2-sha256`, iterations outside 1 000 to 1 000 000, a salt or
   hash of another length stop the start, as the other files (it is never read as "no account",
   which would open the setup to anyone). The password and the tokens are never stored on the
   flash, logged, or put in an answer other than the tokens' own (R3).
5. Hashes and tokens are compared in constant time.
6. **An existing home without `auth.json`** (a box or a Mac data folder from before this change):
   no account, so setup is required; nothing of the home is erased or changed.
7. An account made before the box's clock is set has its `createdAt` moved by the clock's jump
   (D45), and `auth.json` is written again then.

### R2: sessions and tokens

1. A login or a setup opens a **session**: an access token and a refresh token, each 16 random
   bytes written as 32 lowercase hex characters (opaque: not a JWT, D47). The access token lasts
   `ACCESS_TTL_S` = 900 s, the refresh token `REFRESH_TTL_S` = 2 592 000 s (30 days), Sowel's
   defaults.
2. Sessions are kept in RAM only, at most `MAX_SESSIONS` = 8; a 9th login drops the session opened
   first. A restart ends every session (the user logs in again); nothing is written to the flash
   for a session.
3. Dates of sessions and of the attempts (R3.6) taken before the box's clock is set are moved by
   its jump, as the devices' (D45, `Home::observe_wall`). A session opened after `now` (the clock
   went back) is expired and dropped; an attempt dated after `now` is dropped (it never blocks).
4. `lastLoginAt` (Sowel's `User` field) is the last login since the start, in RAM; `null` after a
   restart.
5. A refresh token is not an access token: sent as a bearer, 401.
6. The random source fails (setup, login, refresh) → 500 `Internal error`, no account and no
   session made.

### R3: Sowel's authentication routes

Mirrored from Sowel's `src/api/routes/auth.ts` (bodies checked as the other routes: `Invalid
JSON`, `Unknown field: <f>`, `Invalid field: <f>`, `Missing field: <f>` with 400; a POST without
`Content-Type: application/json` 415, after the `Host` check and before the body is read). Status,
setup, login and refresh need no token (Sowel's `PUBLIC_ROUTES`); logout goes through the gate
(R4), as in Sowel.

1. `GET /api/v1/auth/status` → 200 `{"setupRequired": <true when there is no account>}`.
2. `POST /api/v1/auth/setup` `{username, password, displayName, language?}`, checked in this
   order: 415; an account exists → 403 `{"error":"Setup already completed"}` (before the body, as
   Sowel's `preValidation`); the body → 400 (`language` missing → `fr`, other than `fr`/`en` →
   400); the presence (R6) → 403 `{"error":"<the presence's hint>","confirmRequired":true}`
   (Solite's, D48); the rate limit → 429 (R3.6); then the hash:
   - else the account is made, a session opened, 201 with Sowel's `AuthTokens`:
     `{"accessToken","refreshToken","expiresIn":900,"user":<User>}`; `User` is Sowel's
     (`src/shared/types.ts`): `{"id","username","displayName","role":"admin","preferences":
     {"language"},"enabled":true,"lastLoginAt","createdAt"}`. `auth.json` is written by the saver.
   - Two setups at once: the first to finish makes the account; the other gets 403 `Setup already
     completed`.
3. `POST /api/v1/auth/login` `{username, password}` (Sowel's `trustedDeviceToken` accepted and
   ignored): the right pair → 200 `AuthTokens`; anything else, no account included → 401
   `{"error":"Invalid credentials"}`. A wrong user name costs the same hash as a wrong password.
4. `POST /api/v1/auth/refresh` `{refreshToken}`: a live refresh token → 200 a new `AuthTokens`
   (rotation: the old session is closed, as Sowel); else 401 `{"error":"Invalid refresh token"}`.
5. `POST /api/v1/auth/logout` `{refreshToken?}`, behind the gate → 204; the session of that
   refresh token is closed (its access token too: stricter than Sowel, whose JWT lives to its
   end); without `refreshToken`, or an unknown one, 204 and nothing closed.
6. **Brute force**: setup and login together, at most `MAX_ATTEMPTS` = 10 per 60 s (Sowel: 10 per
   minute on login); beyond, 429 `{"error":"Rate limit exceeded, retry in 1 minute"}` without
   hashing. Only an attempt that would hash counts (a 400, 403 or 415 does not). One counter for
   the box, not per address (the core does not see addresses): a device of the home network can
   keep the owner from logging in while it tries; open sessions and their refresh keep working
   (accepted for a home network, D47).

### R4: the token on every other route

1. Every other `/api/v1/...` route (existing and unknown paths) goes through Sowel's gate, before
   the 413 and 415 checks (an unauthenticated POST gets 401; the box's own 411, 413 and 408, decided
   from the request's head while it reads the body, come before it), in Sowel's order: no account → 403 `{"error":"Setup required","setupRequired":true}`; no
   `Authorization: Bearer <token>` header → 401 `{"error":"Authentication required"}`; a token
   that is not a live access token → 401 `{"error":"Invalid or expired token"}`. Then the route
   answers as today.
2. The interface's files (`/`, `/assets/...`) need no token (Sowel serves its interface publicly).
   The `Host` check comes first, as today (403 `Forbidden host`).
3. The same rules on the Mac host (`--serve`) and on the box.
4. The hash runs without the home's lock: the core reads what it needs under the lock, hashes, and
   takes the lock again to finish; the radio loop is never held by a login.

### R5: the token on `/ws`

1. Sowel's rule: the token comes in the `Authorization` header or in a subprotocol
   `bearer.<token>`. Solite takes the first offered `bearer.` value other than `bearer.none`
   (D49). The subprotocol echoed is the first offered `bearer.` one, as today.
2. The interface offers two subprotocols, `bearer.none` then `bearer.<access token>`: ESP-IDF
   echoes only a fixed subprotocol and reads at most 49 characters of the header, so the box
   echoes `bearer.none` and reads the whole header itself for the token (D49).
3. After the upgrade, as Sowel: no token → close 4001 `Authentication required`; a token that is
   not a live access token → close 4001 `Invalid token`. The `Host` and `Origin` checks (4003) come
   first, the client bound (1013) after. A session that ends later (logout included) does not close
   an open socket (Sowel's: the token is checked at the handshake).
4. On the box a client must offer `bearer.none` (or send the `Authorization` header): one that
   offers only `bearer.<token>`, as Sowel's own interface does, gets no subprotocol echoed and its
   browser drops the socket (a deviation, D49, in `docs/sowel-subset.md`). The firmware sets
   ESP-IDF's `httpd_ws` log to errors only, so the offered subprotocols (the token) are never
   written to the serial log.

### R6: the confirmation at the box (presence)

1. The core defines an adapter trait, `Presence`: `confirmed() -> bool`, true while a person at
   the box has confirmed (the button pressed less than `CONFIRM_WINDOW_MS` = 120 000 ms ago, once
   wired), and `hint() -> &str`, the text the refusal carries (what the person must do). The home
   holds one (`Home::set_presence`); by default it never confirms (hint "Confirmation at the box
   is not available"). The presences count time on a monotonic uptime clock given to them
   (`Arc<dyn Fn() -> u64 + Send + Sync>`, as `orders().set_clock`), never the wall clock, which
   jumps when it is set.
2. The Mac host (`--serve`, loopback only) confirms always (`AlwaysConfirmed`): the person at the
   Mac is at the host.
3. The box, until its button is wired: confirmed during the 10 minutes after a power-on boot (the
   same reset reasons as D32's pairing window: power-on, external reset, USB), never after a
   software restart or a crash (`BootPresence`, in the core, D48). Its hint: "Unplug the box and
   plug it back in, then try again within 10 minutes".
4. A `ButtonPresence` in the core, for the firmware's button later: `press()` confirms until
   `CONFIRM_WINDOW_MS` later; hint "Press the button on the box, then try again". Tested in the
   core; not wired in this spec.

### R7: factory reset

1. `POST /api/v1/system/factory-reset` (`Content-Type: application/json`, an empty body or `{}`;
   Solite's, no Sowel counterpart, D48), behind the gate, then 415 / 400 for the body, then the
   presence (403 `{"error":"<hint>","confirmRequired":true}`). While the pairing window is open or
   a device is still being set up after it closed: 409 `{"error":"Pairing in progress"}`.
2. It erases, in this order, as the API's deletions do (with their live events): every recipe
   instance, every equipment, every device (the home forgets them; they stay in the Zigbee
   network), every zone but the root, the root's name and description back to a first start's;
   the settings emptied; the account and every session. Then 204. Every file is written by the
   saver (`auth.json` as `{"version":1}`).
3. After it, the API answers 403 `Setup required` and the interface shows the first-start screen.
   What it leaves: the Zigbee network and its keys (`zigbee.json`), the Wi-Fi settings
   (`wifi.json`, its password included) and the network setting; the forgotten devices stay in the
   Zigbee network and come back only when they announce themselves during a pairing window (in
   practice, each reset and paired again). EN 18031-2's deletion is therefore partial (D48, an
   inbox line).
4. `Home::factory_reset` is the core function the button's long press will call later.

### R8: pairing on the box

1. The box's pairing route (`GET` and `POST /api/v1/integrations/solite/permit-join`) answers as on
   the Mac, behind the token (D49 replaces D28's constraint and D36's "never from the network").
   The power-on window and the console's `pair` stay.

### R9: the interface

Ported from Sowel's `ui/src/pages/SetupPage.tsx`, `LoginPage.tsx`, `store/useAuth.ts`,
`api/auth.ts`, `api/client.ts` (the token and its refresh on 401), with Sowel's tokens and classes.

1. **At start** the interface reads `GET /auth/status`: `setupRequired` → the **first-start
   screen**; no stored token → the **login screen**; else the home as today.
2. **First-start screen** (Sowel's `SetupPage`): the Solite mark, "Set up your Solite", fields
   user name, display name, password, confirm password; "Passwords do not match" and "At least 8
   characters" checked before sending; the host's refusal shown under the form (for example the
   button's text); on success the tokens are kept and the home shows.
3. **Login screen** (Sowel's `LoginPage`, without MFA): user name, password, "Log in"; "Invalid
   credentials" or the host's text on a refusal.
4. Tokens are kept in `localStorage` under Sowel's keys (`sowel_access_token`,
   `sowel_refresh_token`). Every request carries `Authorization: Bearer <access>`; a 401 asks
   `POST /auth/refresh` once (one at a time) and repeats the request; a refresh refused clears the
   tokens and shows the login screen; a 403 with `setupRequired` shows the first-start screen.
5. `/ws` offers `bearer.none` and `bearer.<access>`; a close 4001 refreshes the token, then
   reconnects; a refresh refused shows the login screen.
6. **Log out**: at the foot of the sidebar, under Administration; `POST /auth/logout`, tokens
   cleared, login screen.
7. **Factory reset**: a card at the foot of Settings: what is erased, a "Factory reset" button,
   a confirmation dialog (what is erased, "Erase everything"); success clears the tokens and shows
   the first-start screen; a refusal (the presence's hint) shows the host's text.
8. French and English, as the rest. The first-load budget (170 KB gzip) holds: two pages, the
   auth store and the client's refresh, about 3 to 5 KB gzip; the figure is in the build's report.
9. The password checks of the setup screen: 8 to 128 characters, counted as the host counts them
   (Unicode code points).

### R10: scripts

`scripts/demo-garage.sh` and `scripts/measure-api.sh` read the password from `SOLITE_PASSWORD`
(never an argument) and the user name from `SOLITE_USER` (`admin` when unset): they ask
`/auth/status`, set the account up when it is required (display name = user name), else log in,
and send the token on every request. Without `SOLITE_PASSWORD`: `<script>: set SOLITE_PASSWORD
(the password of the box)`, exit 2. A refused login or setup prints the host's text, exit 1.
`measure-api.sh` logs in before its 33 timed requests (the login is not counted). The password is
never printed.

## Living spec delta

### docs/living-spec/README.md

- New section **"Protects the home with a password"** (after "Serves Sowel's API from the box"):
  R1 to R8 in business words: one account, set at first start with the button's confirmation (the
  Mac confirms always, the box during 10 minutes after a power-on until its button is wired), no
  default password, the hash and its file `auth.json`, sessions in RAM (8, 15 minutes, 30 days,
  lost at a restart), Sowel's five `/auth` routes, the gate and its errors, 10 attempts per minute,
  `/ws`'s token, the factory reset.
- Changed, "Serves Sowel's device API": "without authentication" → "with Sowel's bearer token
  (see "Protects the home with a password")"; the table gains the `/api/v1/auth/*` routes and
  `POST /api/v1/system/factory-reset`; "The route has no authentication (decision D28)" → "The
  route needs the token (D49)."
- Changed, "Live events": "a `bearer.` subprotocol is echoed back (its token is not checked)" →
  the token rule of R5 and the 4001 closes.
- Changed, "Serves Sowel's API from the box": the pairing route's 404 → answers as on the Mac,
  behind the token; "Only the subprotocol `bearer.none` is echoed" → "`bearer.none` is echoed; the
  token is read from the second subprotocol".
- Changed, "Shows the home in the browser": the first-start and login screens, the log out in the
  sidebar, the factory reset card in Settings.
- Changed, "Runs without the dongle": `demo-garage.sh` and `measure-api.sh` read
  `SOLITE_PASSWORD` and `SOLITE_USER`.

### No longer true

- `docs/living-spec/README.md:288`: "without authentication (a browser page from another site is
  refused...)" → "with Sowel's bearer token (a browser page from another site is also refused...)".
- `docs/living-spec/README.md:324`: "The route has no authentication (decision D28)." → "The route
  needs the token (D49)."
- `docs/living-spec/README.md:328`: "a `bearer.` subprotocol is echoed back (its token is not
  checked)" → R5.
- `docs/living-spec/README.md:355-357`: the box's pairing route answering 404 → removed.
- `docs/living-spec/README.md:360`: "Only the subprotocol `bearer.none` is echoed." → R5.2.
- `firmware/src/live_ws.rs:41` comment ("the only one echoed") → still true, plus the token read.
- `docs/living-spec/README.md:304`: the list of error codes gains 401, 409 and 429.
- `docs/architecture.md`: the files in `/data` gain `auth.json`.
- `docs/roadmap.md`: E3d's "the pairing route ... stays disabled (404) until J4's password
  exists"; the inbox lines on `/ws`'s bearer (removed) and the box's pairing windows (rewritten);
  the box's `measure-api.sh` command line gains `SOLITE_PASSWORD`.
- `scripts/e2e-sim.sh:58`: it waits for `/api/v1/zones` to answer, which the gate now refuses →
  it waits for `/api/v1/auth/status`.
- `README.md` and `CLAUDE.md`: the `demo-garage.sh` and `measure-api.sh` command lines gain
  `SOLITE_PASSWORD=<password>`.
- `docs/sowel-subset.md`: Routes table and Deviations (the factory reset route).
- `docs/architecture.md`: the core's blocks (the `auth` module, the `Presence` adapter).
- Others found by the implementer's search (feature skill 3.4), listed in its report.

## Decisions taken in this spec

- One account, Sowel's admin; opaque tokens in RAM; PBKDF2-HMAC-SHA256 at 10 000 iterations;
  passwords of 8 bytes or more; one rate limit for the box; three RustCrypto crates (`sha2`,
  `hmac`, `pbkdf2`) in the core (std has no hash; D12): **D47**.
- The `Presence` adapter; the Mac confirms always; the box confirms 10 minutes after a power-on
  until the button is wired; the factory reset route, Solite's: **D48**.
- The box's pairing route behind the token (replaces D28's constraint and D36's "never from the
  network", narrows D13's and D33's "without authentication" and "only `bearer.none` is
  echoed"); `/ws`'s token in the second subprotocol: **D49**.

## Acceptance criteria

- [ ] A fresh Mac host (`--sim scenarios/garage.json --serve`) answers `GET /api/v1/devices` with
      403 `Setup required`; `GET /api/v1/auth/status` with `{"setupRequired":true}`.
- [ ] The interface shows the first-start screen; after it, the home; after a reload, still the
      home; after "Log out", the login screen; a wrong password shows "Invalid credentials".
- [ ] Every API route without a token answers 401; with the token, as before.
- [ ] `/ws` without a token closes with 4001; with the interface's two subprotocols, it lives.
- [ ] The host restarted keeps the account (`auth.json`) and asks to log in again.
- [ ] A data folder with a home and no `auth.json` asks for the setup and keeps its home.
- [ ] The 11th login attempt within a minute answers 429.
- [ ] The factory reset from Settings erases the home and shows the first-start screen.
- [ ] `sh scripts/demo-garage.sh 8080` with `SOLITE_PASSWORD` sets the garage up on a fresh host.
- [ ] The four firmware images build; the box's pairing route is no longer 404 (core test with
      `Reach::Lan`).
- [ ] Box checks pending (listed in the roadmap's "Tests to run with the box in hand").

## Edge cases

- No account and a login → 401 `Invalid credentials` (Sowel: no user matches).
- Setup with an account → 403 `Setup already completed`; two setups at once → one account.
- Setup without presence → 403 with `confirmRequired`; nothing written.
- A password of 7 bytes, of 129 bytes, empty → 400; a username of 65 characters or with a control
  character → 400; a missing field → 400; an unknown field → 400.
- A token of the wrong length, not hex, expired, of a closed session, after a restart → 401.
- `Authorization` without `Bearer `, or `Bearer` with nothing → 401 `Authentication required`.
- A 9th session drops the first; its access token answers 401.
- A refresh token used twice → the second 401 (rotation).
- The clock jumps from 1970 to 2026 while a session is open → the session lives on (moved).
- The rate limit's 11th attempt → 429 and no hash computed; after 60 s, accepted again.
- `auth.json` corrupt, with 999 iterations, a salt of 15 bytes, scheme `bcrypt` → the start stops
  with `auth: <where>: cannot read: <reason>`.
- Factory reset during a pairing window → 409; without presence → 403; without token → 401.
- `/ws` offering only `bearer.none` → 4001 `Authentication required`.
- A refresh token sent as a bearer → 401.
- The random source fails → 500, nothing made.
- Logout without a token → 401; without `refreshToken` → 204.
- `auth.json` with `{"version":2}`, a `user` without `id`, a username of 65 characters → the start
  stops.
- A request with the token on an unknown `/api/v1/x` path → 404 (after the gate); without → 401.

## Sowel contract

- Routes: `GET /api/v1/auth/status`, `POST /api/v1/auth/setup`, `POST /api/v1/auth/login`,
  `POST /api/v1/auth/refresh`, `POST /api/v1/auth/logout` (`src/api/routes/auth.ts`,
  `docs/technical/api-reference.md` "Authentication"); the gate's three errors
  (`src/auth/auth-middleware.ts`); `/ws`'s token and 4001 closes (`src/api/websocket.ts`).
- Types: `User`, `UserRole`, `UserPreferences` (`src/shared/types.ts`); `AuthTokens`
  (`src/auth/auth-service.ts`).
- Deviations (D47 to D49, `docs/sowel-subset.md`): `POST /api/v1/system/factory-reset` (Solite's
  route); opaque tokens; one account; passwords of 8 bytes or more; `confirmRequired` on setup;
  one rate limit; logout closes the access token; `/ws`'s token is the first `bearer.` other than
  `bearer.none`; `preferences` has `language` only.

## How the user will check it

On the Mac, with the simulator (no hardware): `cargo run -p solite-host -- --sim
scenarios/garage.json --serve --speed 10` on a fresh data folder, then `cd ui && npm run dev`:
the first-start screen; set a password; the home; log out; log in with a wrong then the right
password; Settings › Factory reset → the first-start screen again. In a terminal: `curl
http://127.0.0.1:8080/api/v1/devices` → 403 `Setup required`, then 401 once set up.
`SOLITE_PASSWORD=... sh scripts/demo-garage.sh 8080` sets the garage up.

On the box (pending, the user present, after their yes to flash, D6, D39): within 10 minutes of a
power-on the interface asks for the password, inside `box.sh saves --budget 1` (one write of
`auth.json`); one login timed; `/ws` live in the browser; Discovery's "Add a device" opens pairing
from the interface; then the button wired (confirmation and long-press reset).
