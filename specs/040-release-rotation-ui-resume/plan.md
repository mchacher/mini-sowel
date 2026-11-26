# Plan: Spec 040

## Steps

- [x] 1. Living spec updated (the delta of `spec.md`)
- [x] 2. Core types: not applicable (no Sowel type changes); `Fetched::Partial`, `etag`, `get_range`,
      `Pacing::wait_ms`
- [x] 3. Core logic: the resume in `update/release.rs` (R6), `forget_cache`, the log line (R7 a)
- [x] 4. Adapters: host `fetch.rs` (`Range`, 206), firmware `fetch.rs`, QEMU's in-memory cut
      (`update_check.rs`)
- [x] 5. API: not applicable (no route changes)
- [x] 6. Release tooling: `scripts/release.sh` bridge mode, `release.yml` (R1)
- [x] 7. Interface: `api/settings.ts`, the Home card's whole-map save, the update card (R4); screenshots
- [x] 8. Tests (the plan below), the chain test (R3), the settings test (R5)
- [x] 9. Docs: `docs/release.md` (R1.7, R2), decisions D70 to D72, design system, roadmap, specs
      index, README, architecture; "No longer true" searched and corrected

## Test plan

| Module | Scenario | Expected |
| ------ | -------- | -------- |
| `release.sh` (bridge) | Bridge file for this version, old key signs (`--key-file`), new key as `--next-key-file` | Release made; app.bin carries the new pub; `--update-verify` accepts the `.sup` with the old pub, refuses it with the new pub; `key bridge` line printed |
| `release.sh` (bridge) | Same, keys through `--key-env` / `--next-key-env` | Both variables absent from the builder's environment; release made |
| `release.sh` (bridge) | The produced `.sup` given to a host built with the old key only, and one built with the new key only (`--update-verify` stands for the compiled key) | Old: accepted; new: refused |
| `release.sh` (bridge) | Signing key = the new key (reverse) | Refused `the signing key does not match the bridge's previous key`, nothing written |
| `release.sh` (bridge) | No next key | Refused `a bridging release needs the next key ...` |
| `release.sh` (bridge) | Next key = the old key (swapped) | Refused `the next key does not match release/update-key.pub` |
| `release.sh` (bridge) | Bridge file's version differs | Refused `... is for <v>, not <version> ...` |
| `release.sh` (bridge) | `previous` equals the committed pub | Refused `previous is the current public key` |
| `release.sh` (bridge) | `previous` not 64 hex; an unknown line; `version` twice | Refused with the form's reason |
| `release.sh` (bridge) | `--build-only` with a valid bridge file | Built, no key asked; a wrong version refused |
| `release.sh` (bridge) | Neither key's text in the output or the log | Checked by the existing key-text rule, for both keys |
| `release.sh` (normal) | `--next-key-file` without a bridge file | Refused `--next-key-* is for a bridging release only` |
| `release.sh` (normal) | Every existing test | Unchanged, green |
| `release.yml` | The bridge's secret | Read by the `sign` job only, in the refusal and signing steps; never in `firmware`, `publish`, `dry-run` |
| `release.yml` | Bridge file present, `SOLITE_UPDATE_KEY_PREVIOUS` empty | The refusal step's `::error::` text present |
| `release.yml` | No bridge file | The signing command is the current one |
| Host chain (R3, `#[ignore]`) | Host A (key A) given a file signed by A, then one signed by B | 200 (slot = the image), then 400 `Invalid signature` |
| Host chain (R3) | Host B (key B) given a file signed by B, then one signed by A | 200, then 400 `Invalid signature` |
| Host chain (R3) | The bridge `.sup` from `release.sh` (old key A signs, image carries B) uploaded to host A | 200 |
| `release.yml` | Dry run with a bridge file in the tree | Second throwaway key and its own bridge file; `--bridge-file`, `--next-key-file`; no secret read |
| `release.sh` (bridge) | `--next-key-file` with `--build-only` | Refused `--build-only takes no key ...` |
| `release.sh` (bridge) | `--public-key-file` given: the bridge checks compare with it | Bridge accepted against the given file |
| Host settings (R5) | PUT `home.*`, then PUT `update.manifestUrl` alone | GET answers all four; after a restart on the same data, the same |
| Core release (R6) | Cut after N bytes, the `Range` request answered 206 | 200 installed; the Range header asked `bytes=N-`; one resume logged |
| Core release (R6) | Two cuts, two resumes | 200 installed |
| Core release (R6) | Cut at byte 0 of the body | `Range: bytes=0-`, 206 from 0 accepted, installed |
| Core release (R6) | Cut inside the 128-byte header | Resumed, installed |
| Core release (R6) | A read timeout instead of an end | Resumed, installed |
| Core release (R6) | Server answers 200 to the Range request | 500 `Download failed: the server does not resume (200 to a range request)`; slot not bootable |
| Core release (R6) | 416 | 500 `... the file changed on the server`; cache dropped (next pull fetches the manifest) |
| Core release (R6) | 206 with another start, another total, or a wrong `Content-Length` | 500 `... the file changed on the server` |
| Core release (R6) | ETag changed between the two answers | 500 `... the file changed on the server` |
| Core release (R6) | ETag absent on one answer | Not compared; installed when the hash matches |
| Core release (R6) | A changed file served 206 with no ETag (bytes differ) | 400 `Image does not match its signature` (the header's image digest fails first; the manifest's hash would too), slot not bootable |
| Core release (R6) | A read error once the count equals `image.size` | Treated as the end: installed when hash and signature match; no Range request |
| Core release (R6) | 503 on the Range request, then 206 | Counted as one attempt; installed |
| Core release (R6) | A redirect to http during a resume | Fails at once: `Download failed: redirect from https to http refused` |
| Core release (R6) | 206 to the first (unranged) download request, and to the manifest | `<url>: 206` failures |
| Core release (R6) | The cut body is dropped before the new request | The fake sees the first body dropped before `get_range` is called |
| Core release (R6) | Every attempt cut (4 cuts) | 500 `Download failed: connection lost (after 3 resumes)`; waits of 2, 4, 8 s on the fake clock |
| Core release (R6) | A resumed request that fails to connect, the next one succeeds | Counted as one attempt; installed |
| Core release (R6) | Total time spent during the waits | 500 `Download failed: took longer than 600 s` |
| Core release (R6) | Each `Range` request starts at `image.url` and follows its redirects | The fake sees the redirect chain again, each hop with the header |
| Core release (R7 a) | A fresh `Releases` | `latest` null; a check reaches the fetcher whatever the cache |
| Core release (R7 a) | The success log line | Names the manifest's address |
| Core release | Every spec 039 test | Unchanged, green (the new field added to the fakes) |
| Host fetch | `get_range` over http against the test server | Sends `Range: bytes=N-`; 206 parsed with `Content-Range` and `ETag` |
| Host fetch | The curl command line with a range | Contains `-H "Range: bytes=N-"` (unit test of the argument list) |
| Host fetch | A captured curl head of a 206 | Parsed: status 206, `Content-Range`, `ETag` |
| Host end-to-end | A test server that cuts the `.sup` after N bytes, then honours `Range` | The empty POST answers 200, the slot written |
| Host end-to-end | A test server without `Range` support (answers 200) | 500 `Download failed: the server does not resume ...` |
| Host end-to-end | The file changed between the two requests (another ETag) | 500 `... the file changed on the server` |
| Host end-to-end | A server that cuts every time | 500 `... (after 3 resumes)` within the total bound (waits shortened through a test seam or the fake pacing: no timing luck) |
| QEMU | In-memory pull with one cut, an 8 KB stand-in touched in the resume's `get_range` | Status 200, at least 1024 bytes of the 20 KB stack never used |
| UI card | `latest` null / available / blocked by minVersion / up to date | The four texts of R4.1; the button only when available; the link only with `releaseUrl` |
| UI card | Check for updates | POST check called; the card refreshed; spinner while it runs |
| UI card | Check refused 429, 503, 500, 409 | The server's text shown |
| UI card | Update, confirmation, Cancel | No request sent |
| UI card | Update confirmed | POST `/system/update` with no body; the elapsed count advances with fake timers |
| UI card | 200, then GET fails twice, then answers a new `current` | Polls every 3 s; "Now running x.y.z" |
| UI card | 200, the box never answers for 3 min | The 3-minute message, polling stopped |
| UI card | 200, answers with the same `current` for 3 min | "The box still runs x.y.z: the new version was not kept" |
| UI card | Status 0 during the request, then the same `current` until 11 min | "The update did not complete: the box still runs x.y.z" (not before 11 min) |
| UI card | Status 0, then a new `current` | "Now running x.y.z" |
| UI card | A poll that hangs | Aborted at 5 s; polls never overlap |
| UI card | Unmount while polling (401 to the login screen) | No further poll |
| UI card | During a pull / an upload | The upload / the pull and the check are disabled |
| UI version | `compareVersions` | 0.10.0 > 0.9.0; equal; lower |
| UI card | Pull refused (400, 409, 500 `Download failed: ...`, 503) | The server's text; the card back |
| UI card | The automatic check toggled | `saveSettings` sends the whole map with `update.autoCheck` |
| UI card | The manifest's address saved, refused 400 | The whole map sent; the server's text shown |
| UI card | The upload of a file | As spec 032 (its tests unchanged) |
| UI Home card | Save | GET then PUT with every key of the map (the `update.*` keys kept) |
| UI | Budget | First load under 170 KB gzip (`npm run check:bundle`) |

## Validation

- [x] Validate command green, QEMU included
- [x] Agent review done, blocking findings fixed
- [ ] Demo seen (autonomous run: the main session shows it to the user)
