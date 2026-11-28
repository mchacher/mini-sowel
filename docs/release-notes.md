# Release notes

What changes for the users of each release, in their words. One section per release, newest first,
under a heading `## x.y.z (date)` (for example `## 0.2.0 (2026-12-01)`); `/solite-release` writes it
in the release commit, and `scripts/release.sh` takes the section whose heading is `## <version>`
alone or `## <version>` followed by a space and more text (`## 0.2.0` and `## 0.2.0 (date)`, never
`## 0.2.01`) as the text of the GitHub release and of `manifest.json`. A release with no section here is
refused before anything is built.

## 0.2.1 (2026-11-28)

Recipes become scripts you install without a new firmware, and the box recognises many more kinds
of devices. (0.2.0 was tagged but never published: its build failed; 0.2.1 is the same release.)

- **Recipes from a catalogue.** A recipe is now a JavaScript script, Sowel's own, signed with the
  recipe key: the Plugins page of Administration lists the catalogue
  (`mchacher/solite-recipes`), installs, updates and removes a recipe, or takes a signed file. No
  recipe is built in any more: `motion-light` (Sowel's 1.1.2, same settings and texts) is
  installed from the catalogue. An instance whose recipe is missing says "Recipe not installed".
- **More devices, recognised from the Zigbee standard** (no list of models):
  - metering plugs: power (W), energy (kWh) and current (A);
  - buttons and remotes: their `action` (on, off, toggle, brightness moves, scene recalls), each
    press reaching the recipes;
  - shutters: their position and the open, close, stop and go-to-position orders;
  - thermostats and radiator valves: the measured temperature, the heating setpoint and setting
    it (5 to 30 °C).
- **A Restart button** in the interface; an update now checks that the PSRAM is there before it
  keeps the new image.
- **Steadier on the box**: the scripts' stack moved out of the internal RAM, so two recipes of
  eight instances each fit and installs are no longer refused after a few of them; tighter bounds
  for the largest homes; a request whose body never comes holds the server 2 s instead of 5; a
  quiet mains device is asked before it is called offline.
- **Interface**: the Administration block sits at the bottom left, as in Sowel.

## 0.2.0 (2026-11-28)

Recipes become scripts you install without a new firmware, and the box recognises many more kinds
of devices.

- **Recipes from a catalogue.** A recipe is now a JavaScript script, Sowel's own, signed with the
  recipe key: the Plugins page of Administration lists the catalogue
  (`mchacher/solite-recipes`), installs, updates and removes a recipe, or takes a signed file. No
  recipe is built in any more: `motion-light` (Sowel's 1.1.2, same settings and texts) is
  installed from the catalogue. An instance whose recipe is missing says "Recipe not installed".
- **More devices, recognised from the Zigbee standard** (no list of models):
  - metering plugs: power (W), energy (kWh) and current (A);
  - buttons and remotes: their `action` (on, off, toggle, brightness moves, scene recalls), each
    press reaching the recipes;
  - shutters: their position and the open, close, stop and go-to-position orders;
  - thermostats and radiator valves: the measured temperature, the heating setpoint and setting
    it (5 to 30 °C).
- **A Restart button** in the interface; an update now checks that the PSRAM is there before it
  keeps the new image.
- **Steadier on the box**: the scripts' stack moved out of the internal RAM, so two recipes of
  eight instances each fit and installs are no longer refused after a few of them; tighter bounds
  for the largest homes; a request whose body never comes holds the server 2 s instead of 5; a
  quiet mains device is asked before it is called offline.
- **Interface**: the Administration block sits at the bottom left, as in Sowel.

## 0.1.1 (2026-11-26)

A small release that proves the update path: a box running 0.1.0 checks GitHub, downloads this one
and installs it by itself. Nothing else changes.

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

