# Releasing Solite

How a release is made, signed and published, and how the signing key is kept. The script is
`scripts/release.sh`, the workflow `.github/workflows/release.yml` (decision D63, key custody D64).
The behaviour is in [the living spec](living-spec/README.md), "Publishing a release".

A release holds `solite-<version>.sup` (the signed update file a running box installs),
`solite-<version>-flash.bin` (the whole image for a first USB flash), `SHA256SUMS` and
`manifest.json` (which a running box reads to find the latest release, spec 039). The notes come from [release-notes.md](release-notes.md).

## The key, once

1. Make the signing key **outside the repository**:
   `cargo run -p solite-host -- --update-keygen ~/keys/solite.update-key`. It writes 64 hex
   characters (mode `0600`) and prints `public key: <64 hex>`; it refuses a path inside a git
   working tree.
2. Commit the public key: its 64 hex characters and a newline in `release/update-key.pub`, on a
   branch, through a pull request (it is not a secret; the file makes a release reproducible from
   its tag).
3. Keep the private key: an entry in a password manager and an offline copy (an encrypted volume).
   A lost key means a box can only be updated by a USB flash.

## The GitHub environment and its secrets (typed by the user)

The signing key reaches the CI as an **environment secret** of the environment `release`, which only
a tag `v*` can use: a branch's workflow or a pull request cannot read it. A bridging release (below)
uses a second secret of the same environment, `SOLITE_UPDATE_KEY_PREVIOUS`, the old private key,
set just before the bridge is tagged and deleted after it. Set the environment up
**before the first tag** (GitHub creates a missing environment without protection at the first run).
With `<owner>/<repo>` the repository:

```
# The environment, then its deployment rule: tags v* only.
gh api -X PUT repos/<owner>/<repo>/environments/release \
  -F 'deployment_branch_policy[protected_branches]=false' \
  -F 'deployment_branch_policy[custom_branch_policies]=true'
gh api -X POST repos/<owner>/<repo>/environments/release/deployment-branch-policies \
  -f name='v*' -f type=tag
# The secret, read from the key file (never typed on the command line).
gh secret set SOLITE_UPDATE_KEY --env release --repo <owner>/<repo> < ~/keys/solite.update-key
gh secret list --env release --repo <owner>/<repo>
# Only for a bridging release (see "Rotating the key"), from the OLD key file; deleted after it.
gh secret set SOLITE_UPDATE_KEY_PREVIOUS --env release --repo <owner>/<repo> < ~/keys/old.update-key
gh secret delete SOLITE_UPDATE_KEY_PREVIOUS --env release --repo <owner>/<repo>
```

A required reviewer on the environment is optional: the tag push is already the user's yes, a
reviewer adds one click before each publication.

**A ruleset on the tags** is recommended. The `main` check of the workflow guards against a
mistake, not against a person with push rights (it runs from the tagged commit's own workflow
file, and the environment's rule looks at the tag's name only), so limit who can create `v*` tags:

```
gh api -X POST repos/<owner>/<repo>/rulesets --input - <<'JSON'
{
  "name": "release tags",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
  "rules": [ { "type": "creation" }, { "type": "update" }, { "type": "deletion" } ],
  "bypass_actors": [ { "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" } ]
}
JSON
```

(`actor_id` 5 is the repository's admin role: only an admin creates, moves or deletes a release
tag.)

## A release

`/solite-release` does it: the version in three files and the notes in a pull request, the tag on
the merge commit of `main`, the CI. By hand, the same steps:

1. A branch `chore/release-vX.Y.Z`: the `version` of `Cargo.toml` (`[workspace.package]`),
   `firmware/Cargo.toml` and `ui/package.json`, `cargo check` for `Cargo.lock`, a section
   `## X.Y.Z (date)` at the top of `release-notes.md`. A pull request, CI green, merged.
2. On `main` at the merge commit: `git tag -a vX.Y.Z -m "<the notes>"`, then `git push origin
   vX.Y.Z`. The workflow refuses a tag that is not on `main`, an empty secret and a missing
   `release/update-key.pub`; the tag and the public key are checked before the build, the secret
   (the two secrets of a bridging release) by the `sign` job, after it.
3. `gh run watch`; the release is at `https://github.com/<owner>/<repo>/releases/tag/vX.Y.Z`, and
   `releases/latest/download/manifest.json` points at it.

A tag starts three jobs: `firmware` builds the radio image (no secret, no environment: it runs the
build tools), `sign` (the environment `release`, the secret; two for a bridge) signs and checks the image it is given,
`publish` makes the GitHub release. A secret is never in a job that runs build code.

**Dry run**: Actions, Release, Run workflow. It builds with a throwaway key and publishes nothing
(the artifact `dry-run-<sha>` holds `manifest.json` and `SHA256SUMS` only). Run it once before the
first tag. When `release/update-key.bridge` is in the tree (a merged bridge, before it is tagged),
the dry run makes a second throwaway key and a bridge file of its own (the tree's version, the first
key as `previous`) and runs the bridge's checks and signing with them: it reads no secret, so it
proves the bridge's mechanics, not that the two real secrets are set.

**On the Mac**, the same files: `sh scripts/release.sh <x.y.z> --key-file <file>` (the ESP32
toolchain exported, `ui/node_modules` present); they land in `target/solite-release/<x.y.z>`.

## Rotating the key: a bridging release (spec 040)

The public key is compiled into the image (`SOLITE_UPDATE_PUBLIC_KEY`, committed as
`release/update-key.pub`) and the update file's format 1 has no field for a next key. A rotation is
one **bridging release**: built with the **new** public key compiled in, signed with the **old**
private key, which the boxes in the field still accept. A box that installs it trusts the new key
only from then on.

1. Make the new key outside the repository: `cargo run -p solite-host -- --update-keygen
   ~/keys/solite-new.update-key`. Keep it like the first one (password manager, offline copy).
2. In GitHub, just before tagging the bridge: `SOLITE_UPDATE_KEY_PREVIOUS` is the old private key,
   `SOLITE_UPDATE_KEY` the new one (the `gh secret set` commands above). A normal tag in between
   fails safely: the probe finds the new private key against the old committed public key.
3. A release pull request: the new public key in `release/update-key.pub`, a file
   `release/update-key.bridge` with the release's version and the old public key, and a notes
   section that says "bridging release" (asked, not checked):

   ```
   # A bridging release (docs/release.md): the image carries release/update-key.pub,
   # the file is signed with the private key of the public key below.
   version 0.3.0
   previous 0123...cdef
   ```

   `previous` is 64 hex characters; `version` and `previous` appear once each; the file is for one
   version only.
4. Tag as usual. With the file present, the `sign` job needs both secrets (an empty
   `SOLITE_UPDATE_KEY_PREVIOUS` is an error and no release) and runs `scripts/release.sh ...
   --key-env SOLITE_UPDATE_KEY_PREVIOUS --next-key-env SOLITE_UPDATE_KEY`. The script refuses, before
   the build: a bridge for another version, a `previous` that is not 64 hex characters or is the
   current public key, a signing key that is not `previous`'s, a missing next key (boxes must not
   move to a key nobody holds), a next key that is not `release/update-key.pub`'s. After signing it
   checks that the old key accepts the `.sup` and the new key refuses it, and prints
   `release: key bridge: signed with <old8>, the image carries <new8>`. By hand, the same:
   `sh scripts/release.sh <x.y.z> --key-file <old key> --next-key-file <new key>`.
5. In the next release's pull request, remove `release/update-key.bridge` (the script refuses to
   reuse it for another version), then delete `SOLITE_UPDATE_KEY_PREVIOUS`. Keep the old key
   offline until every box you care about is past the bridge.

**What a box does across a bridge.** On the old image it accepts the bridge (the old key) and boots
it on probation; once confirmed it trusts the new key only, and a file signed with the old key is
refused (`Invalid signature`). A bridge that fails its probation rolls back to the old image, which
still trusts the old key: such a box takes the bridge's `.sup` again (upload, or the pull while the
bridge is the latest release), never a later release signed by the new key. A box that skips the
bridge (below its version, offline) is refused every later release for its signature: it needs the
bridge's `.sup` uploaded by hand (from the bridge's GitHub release page), or a USB flash. The
manifest names the latest release only and has no field for a bridge; the release that follows a
bridge can be made with `--min-version <the bridge's version>`, so that a box below it is told to
take the bridge first rather than failing on the signature.

## A leaked key

- **What someone holding the old private key can do**: sign an update file that every box still
  built with its public key accepts, with any version number. The no-downgrade rule does not stop
  them: they pick a version above every real one (up to 65535.65535.65535), after which such a box
  refuses every genuine release (not newer) until a USB flash. The probation and the rollback stop
  only an image that fails to start or to confirm within 300 s; a malicious image that runs and
  confirms stays, and it can carry the attacker's own key.
- **What they still need**: to get the file to the box. The upload needs the box's account (the
  local network and the password); the pull needs the manifest the box reads to name it: the GitHub
  release (write access to the repository, or the release workflow), the box's `update.manifestUrl`
  setting (the account again), or a forged TLS certificate for GitHub. The key alone does not reach
  a box.
- **What the maintainer does**: rotate at once with a bridging release (above), signed with the
  leaked key (the only key the boxes accept) and carrying a new key; tell the users to update now;
  set the next release's `minVersion` to the bridge. A box that installed the bridge no longer
  accepts the leaked key.
- **What is not protected**: a box that never updates (the automatic check only reports and is off
  by default: a box nobody updates trusts the leaked key for ever); a box the attacker reaches first
  (a high version installed before the bridge wins, and only a USB flash recovers it); a box that
  rolled back from the bridge; and the manifest itself, which is not signed (its source is trusted
  through TLS and the GitHub account, and the `.sup` it names is checked against the box's key).
- **A lost key**: no bridge can be signed; every box needs a USB flash with an image of the new key.

## What `SHA256SUMS` proves

That the files you downloaded are those of the release (`shasum -a 256 -c --ignore-missing
SHA256SUMS`), not that
the release is genuine: `SHA256SUMS` is not signed. The `.sup` is, and a box checks it. The
full-flash image is as trustworthy as the GitHub release it came from. Signing `SHA256SUMS` is in
the roadmap's inbox.

## First flash from a release

For a Dongle Max never flashed with Solite. The user types these (decision D6: the box's original
firmware is replaced). Download two assets of the release into one folder:
`solite-<version>-flash.bin` and `SHA256SUMS` (the `.sup` is for a box already running Solite).

1. `shasum -a 256 -c --ignore-missing SHA256SUMS` (on Linux `sha256sum -c --ignore-missing
   SHA256SUMS`): the line of the image says OK; the lines of files you did not download are
   skipped.
2. `sh scripts/box.sh identify`: it stops on a box it cannot flash (secure boot, flash encryption,
   a chip revision below 3, a flash that is not 16 MB).
3. Optional, `sh scripts/box.sh backup --out <folder>`: the way back to the original firmware
   (D35).
4. Erase the whole flash (`storage` too, which the image does not cover), then write the image at
   `0x0`:

   ```
   esptool.py --chip esp32 erase_flash
   esptool.py --chip esp32 write_flash 0x0 solite-<version>-flash.bin
   ```

   With esptool v5 (`pip install esptool`) the tool is `esptool` and the commands `erase-flash`
   and `write-flash`.
5. The box starts. Give it its Wi-Fi settings with `sh scripts/box.sh wifi` (see `docs/hardware.md`);
   the interface then asks for a password at the first start.

A box that already runs Solite is **never** flashed this way: it would lose its home and its
network. It takes the `.sup`: Settings, the Firmware update card (or `POST /api/v1/system/update`).
