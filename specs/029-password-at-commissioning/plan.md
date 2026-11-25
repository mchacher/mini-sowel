# Plan: Spec 029

## Steps

- [ ] 1. Living spec updated (the delta of `spec.md`)
- [ ] 2. Core types: `auth` module (account, hash, sessions, attempts, presence), `User`/`AuthTokens` JSON
- [ ] 3. Core logic: `Home`'s auth side, `auth.json` load and save, `factory_reset`, `observe_wall`
- [ ] 4. Core API: `/auth` routes, the gate, `handle_guarded`, `factory-reset`, the box's pairing route, `bearer_token`
- [ ] 5. Host adapter: `serve.rs`, `ws.rs`, `AlwaysConfirmed`; host tests' helpers
- [ ] 6. Firmware adapter: `http_server.rs`, `live_ws.rs`, `BootPresence`
- [ ] 7. Scripts: `demo-garage.sh`, `measure-api.sh`
- [ ] 8. Interface: auth API and store, client, live, setup and login screens, log out, factory reset; e2e
- [ ] 9. Docs: decisions D47 to D49, roadmap, specs-index, sowel-subset, architecture, memory-budget, README, CLAUDE.md

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| core `auth` hash | RFC 7914 / known PBKDF2-HMAC-SHA256 vector (`password`, `salt`, 1 and 4096 iterations) | the known 32 bytes |
| core `auth` hash | verify the right password, a wrong one, one differing in its last byte | true, false, false |
| core `auth` hash | two hashes of one password | different salts, different hashes |
| core `auth` file | an account written then parsed | equal; no password in the text |
| core `auth` file | missing; `{"version":1}` | no account |
| core `auth` file | not JSON; scheme `bcrypt`; 999 and 1 000 001 iterations; salt of 15 bytes; hash of 31 bytes | error `auth: <where>: cannot read: ...`, start stopped (`load_home`) |
| core `auth` sessions | open, access valid until 900 s, invalid at 900 s + 1 ms | true then false |
| core `auth` sessions | refresh: new pair, old access and refresh invalid; refresh used twice | second refused |
| core `auth` sessions | 9th session | the first's tokens invalid, 8 kept (bound) |
| core `auth` sessions | token of 31 chars, not hex, upper-case, empty | invalid |
| core `auth` sessions | clock jump 1970 → 2026 with a session open | still valid (moved) |
| core `auth` sessions | clock back before `openedAt` | invalid |
| core `auth` attempts | 10 attempts in 60 s accepted, 11th refused, accepted again at 60 s | as said |
| core `auth` presence | `BootPresence` power-on at 0 and 599 999 ms, not at 600 000; software restart never | as said |
| core `auth` presence | `ButtonPresence` press at t, confirmed until t + 120 000 exclusive | as said |
| core API `/auth/status` | no account; account | `setupRequired` true; false |
| core API setup | nominal | 201, `AuthTokens` with Sowel's `User` (JSON compared with an example from `types.ts`), `auth.json` changed flag set |
| core API setup | account exists | 403 `Setup already completed` |
| core API setup | presence not confirmed | 403 `confirmRequired`, no account |
| core API setup | password of 7 characters, 129 characters, 4 emoji (16 bytes) and 8 emoji; username 65 chars, with `\n`; missing `displayName`; unknown field; `language` `de` | 400 each |
| core API setup | two setups prepared at once, finished one after the other | the second 403 `Setup already completed` |
| core API login | right pair; wrong password; wrong username; no account | 200; 401 ×3 `Invalid credentials` |
| core API login | 11th attempt within 60 s | 429, and no hash computed (a counter of hashes stays) |
| core API refresh | nominal; unknown; used twice | 200 new pair; 401; 401 |
| core API logout | with the refresh token | 204, the access token then 401 |
| core API logout | without token; without `refreshToken` | 401 `Authentication required`; 204 |
| core API order | unauthenticated POST without content type; setup with account and bad body | 401; 403 `Setup already completed` |
| core API attempts | 400, 403 presence, 415 refusals | not counted toward the 429 |
| core API lastLoginAt | after setup, after login; after a reload of the home | set; `null` |
| core API refresh-as-bearer | refresh token in `Authorization` | 401 |
| core API random fails | setup, login | 500, no account, no session |
| core API setup | without `language` | `preferences.language` = `fr` |
| core auth clock | attempts and sessions dated after `now` | dropped, never block |
| core auth clock | account made in 1970, then the clock set | `createdAt` moved, `auth.json` to write |
| core load | a home without `auth.json` | setup required, home intact |
| core load | `{"version":2}`; `user` without `id`; username of 65 chars | start stopped |
| core API factory reset | live events of the deletions; a device still being set up after the window closed | events sent (`device.removed`, `equipment.removed`...); 409 |
| core API factory reset | without content type | 415 after the gate |
| core presence | hint in the 403 body (`BootPresence`, `ButtonPresence`, default) | each its text |
| host ws | logout while a socket is open | the socket stays open |
| host scripts | refused login | exit 1, host text, password never in output |
| core API gate | no account, any route | 403 `Setup required` with `setupRequired` |
| core API gate | no header; `Basic x`; `Bearer ` empty; wrong token; expired | 401 `Authentication required` ×3, `Invalid or expired token` ×2 |
| core API gate | right token, `GET /devices`, an unknown `/api/v1/x` | 200; 404 |
| core API gate | interface files without token; foreign `Host` | 200 (or the interface's 404); 403 `Forbidden host` |
| core API gate | `handle_guarded` never holds the lock while hashing (a test presence or hash hook observes the lock free) | the lock is free during the hash |
| core API factory reset | nominal with token and presence | 204; no device, equipment, recipe instance, zone but the root; settings empty; no account; old token 401; `auth.json` written as `{"version":1}` |
| core API factory reset | without presence; during a pairing window | 403 `confirmRequired`; 409 `Pairing in progress`, nothing erased |
| core API pairing | `Reach::Lan`, with token, `GET permit-join` | 200 (was 404) |
| core API routes | every route of `ROUTES` answers something other than the route-level `Not found` (existing test) | includes the six new routes |
| core contract | `sowel_contract` with the new routes and the deviation | green |
| core live | `bearer_token`: `bearer.none, bearer.abc` → `abc`; `bearer.abc` → `abc`; `bearer.none` → none; `Authorization: Bearer x` → `x` first | as said |
| core saver | `auth.json` written after the definition delay; a failure retried | as the settings |
| core existing | every existing core test (through `handle`, unguarded) | unchanged |
| host serve | fresh host: `GET /devices` 403; setup over HTTP 201; with token 200; without 401 | as said |
| host serve | restart: account kept, old token 401, login 200 | as said |
| host ws | no token → close 4001 `Authentication required`; wrong → 4001 `Invalid token`; two subprotocols → welcome, `bearer.none` echoed | as said |
| host scripts | `demo-garage.sh` and `measure-api.sh` with `SOLITE_PASSWORD` on a fresh host (setup) and on a set one (login); without it exit 2 and the line | as said |
| host existing | every existing host test, through the helpers with the token | unchanged |
| UI `useAuth` / `client` | 401 → one refresh → request repeated; refresh refused → tokens cleared, login; two 401 at once → one refresh | as said |
| UI `client` | the `Authorization` header on every request | present |
| UI `App` | status `setupRequired` → setup screen; no token → login; token → home; 403 `setupRequired` from a route → setup | as said |
| UI `SetupPage` | mismatch; 7 characters; 129 characters; host refusal text shown; success → home | as said |
| UI `LoginPage` | wrong password → "Invalid credentials"; success → home | as said |
| UI `useLive` | protocols `["bearer.none","bearer.<token>"]`; close 4001 → refresh, reconnect | as said |
| UI `Sidebar` | "Log out" → logout sent, tokens cleared, login screen | as said |
| UI `SettingsPage` | factory reset: confirm dialog, success → setup screen; refusal text shown | as said |
| UI budget | `npm run build` size check | under 170 KB gzip |
| e2e | the browser test passes the first-start screen first | green (opt-in) |
| firmware | the four images build | green (validate) |

## Validation

- [ ] Validate command green
- [ ] Agent review done, blocking findings fixed
- [ ] Demo seen and approved by the user
