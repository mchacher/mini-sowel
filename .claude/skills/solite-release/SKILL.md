---
name: solite-release
description: |
  Publishes a new version of Solite: version number, release notes, tag, and
  the package or deployment. Use when the user types /solite-release, or says
  "publish a version", "sortir une version", "release".
argument-hint: "[version, optional: patch, minor or major]"
---

# Solite Release

Version: $ARGUMENTS

A release is what other people will use: everything is checked first, and nothing is published
without the user's explicit yes. Talk in the user's language; files in English.

## Phase 0: First time only

`release/update-key.pub` is committed and `gh secret list --env release` shows
`SOLITE_UPDATE_KEY`; else stop and point the user at `docs/release.md` (the key, the environment
`release` with its rule for tags `v*`, the secret: typed by the user, never by Claude). A key
rotation is a bridging release with a second secret, set just before its tag: `docs/release.md`,
"Rotating the key".

## Phase 1: Ready?

1. On `main`, up to date (`git pull`), nothing uncommitted.
2. `sh scripts/validate.sh`: everything green.
3. List what changed since the last tag (`git log <last-tag>..HEAD --oneline`), grouped as new
   features, fixes and other, in plain words.

> **GATE 1**: clean `main`, checks green, change list ready.

## Phase 2: Version and Notes

1. Propose the version number (`MAJOR.MINOR.PATCH`): PATCH for fixes only, MINOR for new features
   that change nothing existing, MAJOR if something the users rely on changes. Explain the choice
   in one sentence.
2. On a branch `chore/release-vX.Y.Z`: write the notes at the top of `docs/release-notes.md` under
   `## <version> (<date>)`, what changes for the users, in their words.
3. Update the version in `Cargo.toml` (`[workspace.package] version`), `firmware/Cargo.toml`
   (`version`) and `ui/package.json` (`version`), all three the same; then `cargo check` to
   refresh `Cargo.lock`.

Present the version and the notes.

> **GATE 2 (STOP)**: the user approved the version and the notes.

## Phase 3: Publish

1. Commit (`chore(release): vX.Y.Z`, no AI attribution line), push the branch, open a pull request
   (`gh pr create`); wait for its CI to be green; merge with the user's explicit yes.
2. On `main` at the merge commit (`git pull`): an annotated tag `vX.Y.Z` whose message is the notes
   (`git tag -a vX.Y.Z -F <notes file>`). The CI refuses a tag that is not on `main`.
3. **The user's explicit yes**, then `git push origin vX.Y.Z`: the tag starts
   `.github/workflows/release.yml`, which builds (job `firmware`), signs (job `sign`, the environment secret) and publishes the
   GitHub release. Never make the signed file by hand for a release, never publish an unsigned one,
   never flash the user's box as part of a release without their explicit yes.
4. `gh run watch` on the run; if it fails, read the reason (`gh run view --log-failed`) and tell
   the user; a release that exists is never replaced, a fix is a new version.
5. Give the user the release's address (`gh release view vX.Y.Z --json url`) and how users get it:
   `docs/release.md`, "First flash from a release", or the Firmware update card.

> **GATE 3**: tag pushed, the run green, the release's address given to the user.
