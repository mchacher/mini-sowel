# Release notes

What changes for the users of each release, in their words. One section per release, newest first,
under a heading `## x.y.z (date)` (for example `## 0.2.0 (2026-12-01)`); `/solite-release` writes it
in the release commit, and `scripts/release.sh` takes the section whose heading is `## <version>`
alone or `## <version>` followed by a space and more text (`## 0.2.0` and `## 0.2.0 (date)`, never
`## 0.2.01`) as the text of the GitHub release and of `manifest.json`. A release with no section here is
refused before anything is built.
