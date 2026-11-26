# Release notes

What changes for the users of each release, in their words. One section per release, newest first,
under a heading `## x.y.z (date)` (for example `## 0.2.0 (2026-12-01)`); `/solite-release` writes it
in the release commit, and `scripts/release.sh` takes the section whose heading is `## <version>`
alone or `## <version>` followed by a space and more text (`## 0.2.0` and `## 0.2.0 (date)`, never
`## 0.2.01`) as the text of the GitHub release and of `manifest.json`. A release with no section here is
refused before anything is built.

## 0.1.0 (2026-11-26)

The first release: Solite runs by itself in a Sonoff Dongle Max, with no server, no Docker and no
cloud.

- It forms its own Zigbee network through the Dongle Max's radio, pairs sensors and relays, and
  keeps them across a restart.
- It organises the home in zones and equipments, with Sowel's names and JSON, and runs the
  `motion-light` recipe: motion switches the light on, then off after the delay.
- It serves Sowel's interface and API (live events included) from the box itself, with Sowel's top
  bar: the home's name, the sun times and the connection.
- It asks for a password at the first start, protects every route with it, and can be reset to
  factory settings.
- It backs the whole box up to one encrypted file and restores it.
- It restarts itself when its engine stops, and falls back to a safe mode if it keeps failing.
- It updates itself from a signed release: it checks for a newer version, downloads it, installs it
  in the spare slot and goes back to the old one if the new one does not start.

Known limits: the Wi-Fi is set from the serial console (`scripts/box.sh wifi`), the access point for
a first set-up and Ethernet are not there yet, and the box serves HTTP, not HTTPS.

