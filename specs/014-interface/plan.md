# Plan: Spec 014

## Steps

- [x] 1. Living spec updated
- [x] 2. Core: `GET /` and `/assets/*` left to the host
- [x] 3. Host: static files, `--ui`
- [x] 4. Interface: ported files (headers naming their Sowel source), `t()`, API calls
- [x] 5. Interface: stores and the live store
- [x] 6. Interface: Sidebar, ZonePage, empty states, banner
- [x] 7. Dev proxy
- [x] 8. Screenshots, Hybrid and Dark; bundle measured
- [x] 9. Tests; each module seen failing once
- [x] 10. Docs

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| host static | `GET /` with `ui/dist` | 200 `text/html`, `no-cache`, the file's bytes |
| host static | `GET /assets/index-abc.js`, `.css`, `.svg`, `.woff2`, unknown extension | content types of R1, `immutable` |
| host static | `/assets/../Cargo.toml`, `/assets/%2e%2e/x`, `/assets/a\b`, missing asset | 404 |
| host static | no `ui/dist` | API still served; `GET /` 404 with its text; `interface: ui/dist not found, run npm run build in ui/` printed |
| host static | `Host: evil.example` on `/` | 403 |
| host static | `--ui <folder>` | files served from it |
| i18n | a key in French and English; unknown key | the text; the key |
| live store | open | subscribe `["zones","equipments"]` sent |
| live store | an array of events; a single event; text that is not JSON | applied; applied; ignored |
| live store | close | reconnect after 1 s, 2 s, 4 s ... capped at 30 s (fake timers); each open refetches |
| live store | page visible / online after a close | reconnects at once |
| stores | `zone.data.changed` | the zone's aggregate replaced |
| stores | `equipment.data.changed` | the binding's value updated |
| stores | `equipment.created` twice within 1 s | one refetch |
| stores | `zone.removed` of the zone shown | back to the first zone |
| App | zones and equipments from mocked fetch | sidebar tree indented; first zone with equipments selected |
| App | hash `#/zone/<unknown>` | first zone |
| App | zone without equipment | its empty text |
| ZonePage | aggregate | `<h1>` name; pills: temperature, humidity, luminosity, motion; lights line |
| ZonePage | equipments of the zone only, family order; a disabled one dimmed without control | as listed |
| LightControl | press when off | POST `.../orders/state` `{"value":"ON"}`; button disabled; state unchanged until the event |
| LightControl | 502 | the error text shown, gone after 5 s |
| Banner | socket closed 3 s | shown; hidden on open |
| core static | `/`, `/assets/x.js`, `/assets/../a`, `/assets/a%2e`, `/assets/a\\b`, `/assets/`, `POST /`, `/?a=1` | Static index; Static asset with type; 404 ×4; 405; Static index |
| host static | security headers on `/` and an asset | the three headers |
| host static | `--ui` folder without `index.html`; `--ui` without `--serve` | the start line, `GET /` 404; exit 2 |
| host static | a 1 MB asset | bytes identical, `Content-Length` = size, `File` reader (review) |
| i18n | plural key at count 1 and 3; `{{name}}`; `defaultValue`; French browser | `_one`, `_other`; interpolated; the default; French, `<html lang="fr">` |
| client | GET answered 503 then 200; GET failing | retried once after 1 s, data; the store's error text |
| live store | subprotocol | `bearer.none` |
| live store | backoff with `Math.random` mocked | 1000 + jitter, 2000 + jitter ... |
| live store | page visible while open | nothing |
| live store | `/ws` answered 503 (5th tab) | banner, retries with the backoff |
| App | only the root zone | root selected, the "no room" text |
| App | click a zone; `hashchange` | hash `#/zone/<id>`; that zone selected |
| ZonePage | an offline equipment | Sowel's badge |
| ZonePage | `light_dimmable` | name, icon, badge, no control |
| LightControl | order answered 503; network error | its text 5 s |
| theme | `matchMedia` dark; then light | `data-theme="dark"`; `hybrid` |
| demo script | run on the sim; run twice | ids printed; second run refused |
| budget | `npm run build` | first load ≤ 80 KB gzip (measured), check ≤ 170 KB |
| host | sim garage demo (acceptance) | screenshots; light follows the motion |

## Validation

- [x] `sh scripts/validate.sh` green
- [x] Every row above has its test
- [x] Code review done, findings fixed
- [x] Screenshots, Hybrid and Dark
