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

## Phase 1: Ready?

1. On `main`, up to date (`git pull` if there is a remote), nothing uncommitted.
2. `sh scripts/validate.sh`: everything green.
3. List what changed since the last tag (`git log <last-tag>..HEAD --oneline`), grouped as new
   features, fixes and other, in plain words.

> **GATE 1**: clean `main`, checks green, change list ready.

## Phase 2: Version and Notes

1. Propose the version number (`MAJOR.MINOR.PATCH`): PATCH for fixes only, MINOR for new features
   that change nothing existing, MAJOR if something the users rely on changes. Explain the choice
   in one sentence.
2. Write the notes at the top of `docs/release-notes.md`: the version, the date, then what changes
   for the users, in their words.
3. Update the version where the project keeps it: `Cargo.toml` (`[workspace.package] version`), `firmware/Cargo.toml` (`version`) and
   `ui/package.json` (`version`), all three the same; then `cargo check` to refresh `Cargo.lock`.

Present the version and the notes.

> **GATE 2 (STOP)**: the user approved the version and the notes.

## Phase 3: Publish

1. Commit (`chore(release): vX.Y.Z`, no AI attribution line) and tag `vX.Y.Z`.
2. With a remote: push `main` and the tag.
3. The tag is the release while the firmware is not distributed. To make the signed update file
   of a firmware, follow "Signing a release" in `docs/living-spec/README.md` (key outside the
   repository, build with `SOLITE_UPDATE_PUBLIC_KEY`, `espflash save-image`, `--update-sign`);
   where the signed file is hosted and its manifest are not written yet (see the roadmap's inbox),
   and never publish a firmware that is not signed.
   Never flash the user's box as part of a release without their explicit yes.
4. Tell the user where the version is, and how users get it.

> **GATE 3**: tag pushed (or local if no remote), and the publication confirmed to the user.
