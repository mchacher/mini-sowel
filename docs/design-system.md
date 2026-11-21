# Design system

How the interface of Solite looks: **Sowel's design system**, adapted to a web app served by
the box (decision D4). A screen of Solite looks like a screen of Sowel. A feature that needs a
new colour, size or component adds it here and in the files below first, in the same change.

## Source

The Sowel repository (`github.com/mchacher/sowel`, usually cloned as `../sowel`), commit
`88727672` (Sowel UI 1.87.0), copied on 2026-11-20:

| Sowel file | Copied to | How |
| ---------- | --------- | --- |
| `design-system/tokens.css` (tokens v1.0, themes Hybrid and Dark) | `ui/src/styles/tokens.css` | unchanged |
| `ui/src/index.css`, its `@theme` and `@theme static` blocks | `ui/src/styles/theme.css` | unchanged, a header added |
| `ui/eslint/design-classes.js` (type scale, sentence case, radius scale) | `ui/eslint/design-classes.js` | unchanged |
| `ui/src/components/layout/SowelLogo.tsx` | `ui/src/components/layout/SowelLogo.tsx` | unchanged |
| `ui/src/components/layout/SowelLogo.tsx` | `ui/src/components/layout/SoliteLogo.tsx` | derived: the same house, face and wink, plus three short rays above the roof for Solite's "light" |

The Solite logo is the one the sidebar and the mobile top bar show; `SowelLogo.tsx` stays as ported.

The interface's components, stores and helpers are ported from Sowel's `ui/src` at commit
`5da84dc1` (UI 1.87.0), read on 2026-11-21 (spec 014); each ported file starts with a header
naming its Sowel source and commit:

| Sowel file (`ui/src/`) | In Solite | How |
| ---------------------- | ------------- | --- |
| `lib/{format,locale,equipment-family,api-error,brightness,coalesce-calls}.ts`, `i18n/translate.ts`, `hooks/useSliderOverride.ts` | same paths | unchanged |
| `components/equipments/{SensorValues,sensorUtils,EquipmentStatusBadge}.tsx`, `components/home/{ZoneHeroLead,Panel}.tsx`, `store/useZoneAggregation.ts` | same paths | unchanged |
| `components/equipments/LightControl.tsx` | same | three type-only changes for Preact's event types |
| `theme.ts` | same | one line sets `data-theme`; the `theme-color` metas cut |
| `lib/format-relative.ts` | same | `parseReadingTime` copied locally (it came from Sowel's backend) |
| `components/home/ZoneAggregationPills.tsx` | same | the sparkline, the history flag, the shutter and water-valve pills cut |
| `store/useZones.ts`, `store/useEquipments.ts` | same | the editing actions and the order timing cut |
| `api/client.ts`, `api/zones.ts`, `api/equipments.ts`, `api/index.ts` | same | no token or refresh; a GET answered 503 retried once after 1 s; only the four calls used |
| `types.ts`, `components/equipments/bindingUtils.ts`, `components/equipments/useEquipmentState.ts` | same | the named subsets the ported files use, copied unchanged |
| `index.css` utilities `card`, `panel-lid`, `slices`, `slice-rows` | `styles/utilities.css` | unchanged |
| `i18n/index.ts` (i18next, 67 KB of locales) | `i18n/{index,messages,react-i18next}.ts` | replaced: a `t()` with Sowel's keys, French and English |
| `components/layout/SidebarZoneTree.tsx` | `components/layout/ZoneTree.tsx` | replaced: same classes and icons, links in the hash |
| `components/home/{ZoneEquipmentsView,CompactEquipmentCard}.tsx`, `pages/HomePage.tsx` | `components/home/{ZonePage,EquipmentRow}.tsx`, `App.tsx` | replaced, with Sowel's sections and classes |
| `store/useWebSocket.ts` | `store/useLive.ts` | replaced: four topics, Sowel's reconnection |

The rules, the colour roles, the components and the contrast measurements are Sowel's, and are not
repeated here: read them in Sowel's `design-system/` folder.

| To know... | Read in Sowel |
| ---------- | ------------- |
| Philosophy, naming, themes | `design-system/README.md` |
| Every token and its role (one meaning per colour: amber is "on or running now") | `design-system/tokens.md` |
| Components: anatomy, states, accessibility | `design-system/components.md`, `design-system/components/*.md` |
| Motion | `design-system/motion.md` |
| Contrast pairs, touch targets, focus, ARIA | `design-system/accessibility.md` |
| How a component is written with Tailwind | `ui/src/components/` |

## Files

| File | What it holds |
| ---- | ------------- |
| [`ui/src/styles/tokens.css`](../ui/src/styles/tokens.css) | The **single source** of every colour, radius, shadow, spacing and type size, as CSS variables, for the Hybrid (light) and Dark themes |
| [`ui/src/styles/embedded.css`](../ui/src/styles/embedded.css) | What Solite changes in the tokens for the box, loaded after them |
| [`ui/src/styles/theme.css`](../ui/src/styles/theme.css) | The Tailwind 4 theme: the tokens as utilities (`bg-surface`, `text-primary`, `text-small`, `rounded-md`...) |
| [`ui/src/styles/utilities.css`](../ui/src/styles/utilities.css) | Sowel's utility classes the ported components use (`card`, `panel-lid`, `slices`, `slice-rows`) |
| [`ui/src/index.css`](../ui/src/index.css) | Loads Tailwind, then `tokens.css`, `embedded.css`, `theme.css` and `utilities.css` in this order |
| [`ui/eslint/design-classes.js`](../ui/eslint/design-classes.js) | Sowel's design rules, run by ESLint |
| [`ui/scripts/check-bundle-budget.mjs`](../ui/scripts/check-bundle-budget.mjs) | The first-load budget |

The theme is chosen by `data-theme="hybrid"` or `data-theme="dark"` on `<html>`, set by the
ported `theme.ts` from `prefers-color-scheme` and its changes; `index.html` holds `hybrid` until it
runs.

## Embedded: what changes from Sowel

| Sowel | Solite | Why |
| ----- | ---------- | --- |
| Inter (variable) and JetBrains Mono, served from `@fontsource` | System fonts: `system-ui` for text, `ui-monospace` for code (`embedded.css`) | The box serves its pages from its flash: no room for web fonts |
| React 19 | Preact, with `preact/compat`: Sowel's React components work unchanged | About 4 KB instead of about 45 KB gzip |
| Lucide icons by name, one rule against the whole set | The same rule; icons by name only | Every icon counts in the budget |
| First load under 1.4 MB raw | First load under **170 KB gzip**, checked by `npm run check:bundle` in the validate command | The framing note: 100 to 170 KB gzip, served from flash |
| Sidebar: logo and "SOWEL" in Nunito | Logo and "SOLITE" in the system font | No web font |
| i18next with its locale files | A `t()` with Sowel's keys, French and English, `react-i18next` aliased to it | About 21 KB of library and 67 KB of locales |
| React Router | The room in the address's hash (`#/zone/<id>`) | About 28 KB, one page |
| `useWebSocket`, eleven stores | `useLive`: zones and equipments, Sowel's reconnection | The tool has no modes, energy or activity |
| Zone pills with sparklines, shutters, water valves | Without them | No history, no such equipments yet |
| `CompactEquipmentCard` and its 35 controls | `EquipmentRow`: the light control and sensor values; other types name, icon and badge | The equipments the tool serves |

Everything else (colours, roles, scales, radii, shadows, motion, components) is Sowel's, unchanged.

## Layout

Sowel's application layout: a sidebar on the left (260 px, `bg-surface`, right border), the logo
and the tool's name in a 49 px header, the rooms' tree below; the room on the right on the
`bg-background` tint: its name as the title, Sowel's lights line and pills beneath it, then its
equipments by family. Below `md` the sidebar gives way to a top bar with a menu button that
opens it as a drawer (Sowel's pattern; Sowel switches at `lg`). Code: `ui/src/components/layout/{Sidebar,ZoneTree,Banner}.tsx`,
`ui/src/components/home/{ZonePage,EquipmentRow}.tsx`, `ui/src/App.tsx`.

## Rules

- **Tokens only**: colours through the Tailwind theme classes or `var(--token)`; ESLint refuses a
  raw hex colour in a component. `tokens.css` is the only file with colour values.
- **Sowel's scales**: type sizes `text-micro` … `text-value`, radii `rounded-xs/sm/md/lg/full`,
  sentence case (uppercase only in table headers); ESLint refuses the rest.
- **One meaning per colour**, as in Sowel's `tokens.md` §1.2: amber is "on or running now" and
  nothing else; orange asks for attention, red reports a fault, green says it works.
- **A Sowel component is ported, not redrawn**: copy it from Sowel's `ui/src/components/`, keep
  its classes, adapt only what the box lacks (routing, stores, i18n), and say what changed in the
  spec.
- **A token or component Sowel lacks** is added here first, with its reason; when it would serve
  Sowel too, it becomes a Sowel issue.
- **Icons**: Lucide (`lucide-react` through `preact/compat`), imported by name, 16 px in lists and
  buttons, `strokeWidth={1.5}` as in Sowel; ported files keep Sowel's sizes.
- **Updating from Sowel**: copy the files of the two tables under Source again from a newer Sowel
  commit, compare each with the commit in its header, apply the cuts listed again, update the
  commits and dates, and check the budget (first load 34 KB gzip in spec 014).
