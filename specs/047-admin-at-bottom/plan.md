# Plan: Spec 047

## Steps
- [x] 1. Living spec and design system delta.
- [x] 2. `Nav` and `AdminSection` (`Sidebar.tsx`).
- [x] 3. Tests (`Sidebar.test.tsx`).
- [ ] 4. Screenshots, Hybrid and Dark, desktop and drawer, 1 room and 10 rooms.
- [x] 5. validate; code review; demo (served on the Mac).

Not applicable: core types, core logic, adapters, API (no change outside the interface).

## Test plan
| Module | Scenario | Expected |
| --- | --- | --- |
| Sidebar | nominal: the column's layout | `nav` holds two children: the tree's area (`overflow-y-auto`, `flex-1`, `min-h-0`) then the bottom block (`border-t border-border-light py-2 px-2`, and `min-h-0 overflow-y-auto` for a very short window) |
| Sidebar | the bottom block's content | the Administration title then Log out, nothing after; the section has no `border-t` of its own |
| Sidebar | many rooms (20) | every room is inside the scrolling area, none in the bottom block |
| Sidebar | no room | the bottom block is still the `nav`'s last child, with Administration and Log out |
| MobileDrawer | the drawer | the same two areas, Administration and Log out in the bottom block |
| Sidebar | unchanged: entries, opening, badge, Log out | every existing test passes, except the "foot" test (`Sidebar.test.tsx:56-64`), replaced by the two rows above |

The short window (`min-h-0`) is a layout effect jsdom does not compute: checked by the classes in
the nominal test and seen in the screenshots (one at about 320 px high).

## Validation
- `sh scripts/validate.sh` green (UI budget included).
