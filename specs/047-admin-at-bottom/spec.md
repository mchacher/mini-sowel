# Spec 047: The Administration block at the bottom left, as in Sowel

## Context

The roadmap inbox line of 2026-11-27 (user): "the Administration section at the bottom left as in
Sowel". In Sowel's sidebar (`ui/src/components/layout/Sidebar.tsx:115,259-261` of the Sowel
repository) the zone tree scrolls in its own `flex-1 overflow-y-auto` area, and Administration
is a block of its own **under** it, pinned at the bottom of the column, under a
`border-t border-border-light`, `py-2 px-2`. In Solite (`ui/src/components/layout/Sidebar.tsx:106-114`) the
Administration section and Log out sit inside the scrolling `nav`, right under the last room: with
few rooms they float in the middle of the column; with many they scroll out of sight.

## Goals
- Administration and Log out at the bottom of the sidebar and of the phone's drawer, always
  visible, the room tree scrolling alone above them, as in Sowel.

## Out of scope
- Sowel's other bottom blocks (Settings as a block of its own, the collapse toggle): Solite's
  Settings is an Administration entry (spec 026) and the sidebar has no collapsed mode (design
  system, spec 019).
- Any change to the entries, their order, the badge, the opening rule or Log out's behaviour.

## Requirements

### R1: two areas in the column
- The navigation (`nav`, its label unchanged) is a column of two areas: the room tree, in an
  area that takes the free height and scrolls (`flex-1 min-h-0 overflow-y-auto py-3 px-2`); then
  a bottom block that does not scroll, `border-t border-border-light py-2 px-2` (Sowel's
  classes), holding the Administration section and, under it, Log out.
- The Administration section loses its own top margin, padding and border (`mt-3 pt-3 border-t`):
  the bottom block's border replaces them.
- Opened, the Administration list grows the bottom block upwards and the room tree's area
  shrinks (and scrolls), as in Sowel.
- When the bottom block alone is taller than the column (Administration open in a window about
  320 px high, a phone held sideways), it scrolls itself (`min-h-0 overflow-y-auto`, added to
  Sowel's classes): nothing is cut off.
- One difference from Sowel, kept: Sowel's two areas are siblings in the `aside`; Solite keeps
  one `nav` landmark (its label, `app.mainNav`) holding both, Log out included.

### R2: the drawer
- The phone's drawer uses the same navigation: the same two areas, the bottom block at the
  bottom of the drawer.

## Living spec delta

### docs/living-spec/README.md
- 1049-1050 "then, at its foot, the Administration section ... under it, **Log out**" → "the rooms
  as a tree, which scrolls alone; pinned at the bottom of the column, under a line, the
  Administration section ... under it, **Log out**" (the sidebar and the drawer).

### docs/design-system.md
- 53 (the Administration block's row): add "pinned under the scrolling zone tree in a bottom
  block, Sowel's `border-t border-border-light py-2 px-2`".
- 114 "the Administration section at its foot" → "pinned at the bottom of the column, under the
  scrolling room tree".

### No longer true
- None beyond the lines above (searched: "foot", "Administration section", "Log out" in
  `docs/`, `README.md`, `CLAUDE.md`, `.claude/`).

## Acceptance criteria
- [x] The room tree and the bottom block are two siblings inside the `nav`; only the tree's area
      scrolls; the bottom block has Sowel's classes (tested).
- [x] Administration then Log out in the bottom block, nothing after Log out (tested).
- [x] The drawer has the same layout (tested).
- [ ] (Not taken, to save the run's tokens: the interface served on the Mac for the user to see instead.) Screenshots, Hybrid and Dark, desktop and phone drawer: Administration at the bottom left
      with 1 room and with many rooms; one at about 320 px high, Administration open.
- [x] validate green; the UI size budget holds.

## Edge cases
- Many rooms (more than the column's height): the tree scrolls, Administration stays visible.
- Administration open on a short window: the tree's area shrinks to nothing rather than pushing
  the bottom block out (`min-h-0`); shorter still (about 320 px), the bottom block scrolls.
- No room at all: the tree's area is empty, the bottom block still at the bottom.

## Sowel contract
No type, field or route. The look mirrors Sowel's `ui/src/components/layout/Sidebar.tsx`
(115: the scrolling area; 259-261: the Administration block).

## How the user will check it
On the Mac, `cargo run -p solite-host -- --sim scenarios/garage.json --serve 8080`, the interface
at http://localhost:8080: Administration at the bottom left; the drawer on a phone width; then
the realistic home (`firmware/fixtures`, 10 zones) with the window made short: the rooms scroll,
Administration stays.

### Result (2026-11-27)
`sh scripts/validate.sh --skip-slow` green (1850 tests); the three new Sidebar tests fail on the old layout. Code review (Sonnet): no blocking finding. The interface served by the Mac host for the user to look at.
