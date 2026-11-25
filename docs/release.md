# Releasing Solite

How a release is made, signed and published, and how the signing key is kept. The script is
`scripts/release.sh`, the workflow `.github/workflows/release.yml` (decision D63, key custody D64).
The behaviour is in [the living spec](living-spec/README.md), "Publishing a release".

A release holds `solite-<version>.sup` (the signed update file a running box installs),
`solite-<version>-flash.bin` (the whole image for a first USB flash), `SHA256SUMS` and
`manifest.json`. The notes come from [release-notes.md](release-notes.md).

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

## The GitHub environment and its secret (typed by the user)

The signing key reaches the CI as an **environment secret** of the environment `release`, which only
a tag `v*` can use: a branch's workflow or a pull request cannot read it. Set the environment up
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
   by the `sign` job, after it.
3. `gh run watch`; the release is at `https://github.com/<owner>/<repo>/releases/tag/vX.Y.Z`, and
   `releases/latest/download/manifest.json` points at it.

A tag starts three jobs: `firmware` builds the radio image (no secret, no environment: it runs the
build tools), `sign` (the environment `release`, the secret) signs and checks the image it is given,
`publish` makes the GitHub release. The secret is never in a job that runs build code.

**Dry run**: Actions, Release, Run workflow. It builds with a throwaway key and publishes nothing
(the artifact `dry-run-<sha>` holds `manifest.json` and `SHA256SUMS` only). Run it once before the
first tag.

**On the Mac**, the same files: `sh scripts/release.sh <x.y.z> --key-file <file>` (the ESP32
toolchain exported, `ui/node_modules` present); they land in `target/solite-release/<x.y.z>`.

## What a leak, a loss and a rotation mean (D64)

- **A leak**: whoever holds the key can sign an update every box built with its public key
  accepts. Make a new key and release with it; boxes built with the leaked key stay exposed until
  they are moved to the new key by a release signed with the old one, or by USB.
- **A loss**: no new release can be installed by a box; a USB flash with the new key is the way.
- **A rotation** today: the public key is compiled into the image and the update file's format 1
  has no field for a next key. A release signed with the old key that carries the new public key
  moves the boxes; from it on, releases are signed with the new key. The format and the order are
  step 3 of the roadmap's J5b.

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
