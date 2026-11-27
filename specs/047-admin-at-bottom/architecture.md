# Architecture: Spec 047

## Overview
One component changes: `Nav` in `ui/src/components/layout/Sidebar.tsx`, used by both `Sidebar`
and `MobileDrawer`, so both get the layout at once.

## Data
None.

## Files changed
| File | Change |
| --- | --- |
| `ui/src/components/layout/Sidebar.tsx:106-114` | `nav` becomes `flex-1 min-h-0 flex flex-col`; the `ZoneTree` goes into `div.flex-1 min-h-0 overflow-y-auto py-3 px-2`; `AdminSection` and `LogoutButton` into `div.border-t border-border-light py-2 px-2 min-h-0 overflow-y-auto` |
| `ui/src/components/layout/Sidebar.tsx:34` | `AdminSection`'s root drops `mt-3 pt-3 border-t border-border-light` |
| `ui/src/components/layout/Sidebar.test.tsx:56-64` | the "foot" test checks the two areas and the bottom block's classes |
| `docs/living-spec/README.md:1049-1050`, `docs/design-system.md:53,114` | the delta |

## Reused
Sowel's classes for the bottom block (`border-t border-border-light py-2 px-2`) and for the
scrolling area (`flex-1 overflow-y-auto py-3 px-2`); `min-h-0` lets a flex child shrink below its
content (Tailwind 4 core class).
