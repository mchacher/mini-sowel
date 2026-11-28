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
| `ui/src/lib/logo.ts` (commit `5da84dc1`, Sowel spec 210: the revised mark, a master and a small weight) | `ui/src/lib/logo.ts` | derived: the house, eyes, wink and smile number for number; plus Solite's antenna (a mast from the ridge, a ball, one wave per side) in each weight |
| `ui/src/components/layout/SowelLogo.tsx` (commit `5da84dc1`) | `ui/src/components/layout/SoliteLogo.tsx` | derived: renders `logo.ts`, the weight chosen by size (small below 32 px), the same wink; the wordmark is an option |
| Nunito Bold (700), SIL Open Font License | `ui/src/lib/wordmark.ts` | the word "solite", lowercase, outlined into one SVG path; licence in `ui/src/lib/NUNITO-OFL.txt` |
| `ui/public/favicon.svg` (commit `5da84dc1`) | `ui/src/assets/favicon.svg` | derived: the small weight with the antenna; Vite publishes it as `/assets/favicon-<hash>.svg`, which the host and the box's radio image both serve; `logo-sync.test.ts` checks it against `logo.ts` |

The Solite mark (Sowel's smiling house with an antenna and a radio wave on each side of the roof)
is the one the sidebar, the drawer and the browser tab show. The house is the
ink (`--color-primary`), the face, the ball and the waves the mid-blue (`--color-primary-mid`);
the favicon, an image, holds the same two values as hex. The wordmark is a path, not text: no web
font is loaded.

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
| `store/useZones.ts`, `store/useEquipments.ts` | same | `reorderZones` and the order timing cut; the four binding actions ported (spec 030); the actions read the tree or the list again after their own request |
| `api/client.ts`, `api/zones.ts`, `api/equipments.ts`, `api/index.ts` | same | no token or refresh; a GET answered 503 retried once after 1 s; only the calls used (zones: get, aggregation, create, update, delete; equipments: get, create, add a data binding, add an order binding, remove a data binding, remove an order binding, update, delete, order; devices: get one, get all, update, delete) |
| `types.ts`, `components/equipments/bindingUtils.ts`, `components/equipments/useEquipmentState.ts` | same | the named subsets the ported files use, copied unchanged except `RecipeInstance.state` (the state keys of `GET /recipe-instances`, spec 024); `bindingUtils.ts` plus the binding plan for the eight types with rules (specs 019, 027) |
| `index.css` utilities `card`, `panel-lid`, `slices`, `slice-rows` | `styles/utilities.css` | unchanged |
| `i18n/index.ts` (i18next, 67 KB of locales) | `i18n/{index,messages,react-i18next}.ts` | replaced: a `t()` with Sowel's keys, French and English |
| `components/layout/SidebarZoneTree.tsx` | `components/layout/ZoneTree.tsx` | replaced: same classes and icons, links in the hash |
| `components/home/{ZoneEquipmentsView,CompactEquipmentCard}.tsx`, `pages/HomePage.tsx` | `components/home/{ZonePage,EquipmentRow}.tsx`, `App.tsx` | replaced, with Sowel's sections and classes |
| `store/useWebSocket.ts` | `store/useLive.ts` | replaced: four topics, Sowel's reconnection |
| `pages/SetupPage.tsx`, `pages/LoginPage.tsx`, `store/useAuth.ts`, `api/auth.ts` | `components/auth/{SetupPage,LoginPage,AuthScreen}.tsx`, `store/useAuth.ts`, `api/auth.ts` | ported (spec 029): no router (the App shows the screen), no second factor or trusted device, no preferences; password 8 to 128 code points; one refresh in flight; `AuthScreen` is Solite's own frame (Solite mark, one `card`) shared by the two screens |
| `api/client.ts` (token, refresh on 401) | `api/client.ts` | ported (spec 029): a 403 with `setupRequired` calls the auth store; `fetchSession` (token, no refresh) for the logout |
| `lib/zone-path.ts` and its test | same | unchanged |
| `pages/ZonesPage.tsx`, `components/zones/{ZoneTree,ZoneForm}.tsx` | `components/zones/{ZonesPage,ZoneTree,ZoneForm}.tsx` | ported (spec 018): no auth, reorder or expand/collapse; names at most 64; no "None" parent; edit and delete in the rows; the modal is `components/ui/Modal.tsx` (Escape and click outside close it) |
| `pages/EquipmentsPage.tsx`, `components/equipments/{EquipmentForm,DeviceSelector,equipment-type-meta}.tsx` | `components/equipments/{EquipmentsPage,EquipmentForm,DeviceSelector,equipment-type-meta}.tsx` | ported (spec 018): the eight types with rules (`light_color` since spec 027), a type outside them kept and shown as written; Sowel's candidates for lights, switches and shutters and the thermostat contract; no search; a row instead of `EquipmentCard`; the plan and one request per binding, no device required (spec 030: an equipment can be created alone and completed on its page); check boxes, 64 devices at most, a warning past 8 values |
| `components/layout/{admin-nav-items,SidebarItem}.tsx`, the Administration block of `Sidebar.tsx` | `components/layout/{admin-nav-items,SidebarItem,Sidebar}.tsx` | ported (spec 019): Devices, Equipments, Topology, Discovery in the place of Integrations, and Settings (spec 026, Solite's own page); hash links instead of `NavLink`; no collapsed mode, roles or badges; a Log out button under Administration (spec 029, Solite's own); pinned under the scrolling zone tree in a bottom block, Sowel's `border-t border-border-light py-2 px-2` (spec 047), which scrolls itself in a very short window |
| `components/layout/{TopBarFrame,SunTimes,HomeTimeNote,ConnectionStatus}.tsx` and their tests, `hooks/useTopBarState.ts`, `store/useTimezone.ts`, the header of `AppLayout.tsx` (`TopbarBreadcrumb`, `MobileTopbarTitle`, `HomeNameSkeleton`) | `components/layout/{TopBarFrame,SunTimes,HomeTimeNote,ConnectionStatus,TopBar}.tsx`, `hooks/useTopBarState.ts`, `store/useTimezone.ts` | ported at Sowel commit `2d360726` (spec 037, D62): `md` in the place of `lg`; no shadow mode or takeover banner, no alarms, updates or restart pills, no user block, no `hit-group`; the link indicator reads `useLive` (`open` is `connected`, `closed` `disconnected`); no router, so `TopBar` takes the page and the zone shown as props, the parts of the context are made by `top-bar-context.ts` (Solite's own); the home's name and whether a location is set come from `store/useHomeSettings.ts` (Solite's own); `SunLocationHint`, a "Set the location" link to Settings where Sowel shows nothing, is Solite's own; `SunTimes`, `HomeTimeNote`, `useTimezone` unchanged |
| `pages/DevicesPage.tsx`, `components/devices/DeviceList.tsx` | `components/devices/{DevicesPage,DeviceList,EquipmentCell}.tsx` | ported, cut (spec 019): the table with the Equipment column and delete; no search, sort, source, battery, LQI or last-seen columns; a row opens the device page (spec 030); `TabButton` in `components/ui/` |
| `pages/EquipmentDetailPage.tsx` (with `DevicesSection`, `ChangeDeviceModal`), `components/equipments/{AddBindingModal,MissingBindingsModal,ConfigRow}.tsx` | `components/equipments/{EquipmentDetailPage,AddBindingModal,MissingBindingsModal,ConfigRow}.tsx` | ported (spec 030): no history section, no panels of the other types, no type change on edit, no router (the page reads the equipment from the store); dialogs on `Modal`, a refusal stops the writing and shows the host's text; `ConfigRow` unchanged; `EnabledRow` is Solite's own (Sowel has no control for `enabled`) |
| `pages/DeviceDetailPage.tsx`, `components/devices/DeviceDataTable.tsx`, `components/RelativeTime.tsx` | `components/devices/{DeviceDetailPage,DeviceDataTable}.tsx`, `components/RelativeTime.tsx` | ported (spec 030): no raw expose section, source badge or admin test; the data come from `store/useDeviceDetail.ts` (Solite's own, followed by `device.data.updated`); `DeviceDataTable` and `RelativeTime` unchanged |
| `src/shared/binding-candidates.ts`, `thermostat-contract.ts` | `lib/binding-candidates.ts` | ported (spec 019): the candidates of the lights (`light_color` since spec 027), switches and shutters (category fallback for shutters) and `isThermostatDevice` |
| `api/recipes.ts`, `store/useRecipes.ts`, `lib/recipe-i18n.ts`, `lib/coalesce-calls.ts` | same paths | ported (spec 024): only the routes of spec 012 (no action call); no `sendAction` or `getLog` in the store; no group label; the instance refetch on `recipe.instance.*` is coalesced (1 s) in `store/useLive.ts` |
| `pages/PluginsPage.tsx`, `components/layout/usePluginUpdates.ts` | `components/plugins/PluginsPage.tsx`, `components/layout/useRecipeUpdates.ts` | ported, cut (spec 042): script recipes only, no category filter, integration rows, detail sheet, confirmations, personal sources or update-all banner; Solite's own `/recipes/*` routes; the update count is read from the held catalogue |
| `components/recipes/{ZoneRecipesSection,RecipePickerPopover,RecipeInstanceRow,RecipeJournal}.tsx` | same paths | ported, cut (spec 024): a Panel of its own with the "+" in its lid; no accounts, groups, bottom sheet, duplicate, cycle pills, marquee, alarm dot or day-period grouping; the tile is `Lightbulb` or `ChefHat` by name; the journal shows `HH:MM` and the message; Settings opens `components/ui/Modal.tsx` |
| `components/recipes/{AddRecipeForm,RecipeSettings}.tsx`, `recipe-slot-helpers.ts`, `lib/recipe-slots.ts` | `components/recipes/RecipeForm.tsx`, `lib/recipe-form.ts` | replaced (spec 024): one form for create and edit, built from the slots; `hiddenWhen` kept (spec 055); `data-key` is a select of the chosen equipment's data aliases; no groups, `time` pickers (a text field), cross-zone equipments; the params logic is pure functions |
| `components/recipes/recipe-form-fields.tsx` | `components/recipes/recipe-fields.tsx` | ported, cut (spec 024): `EquipmentCheckboxList` (an array of ids, no zone beside a name), `DurationInput` (minutes rounded to 2 decimals) and `CountdownTimer` only |
| `components/ui/Switch.tsx` | same | ported (spec 024): the click event's type for Preact; the `hit-44` class cut (no such utility) |

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
| Sidebar: logo and "sowel" in Nunito (web font) | Mark and "solite" in Nunito Bold, lowercase, outlined into an SVG path | No web font: the outlined word costs about 3 KB |
| i18next with its locale files | A `t()` with Sowel's keys, French and English, `react-i18next` aliased to it | About 21 KB of library and 67 KB of locales |
| React Router | The room in the address's hash (`#/zone/<id>`) | About 28 KB, one page |
| `useWebSocket`, eleven stores | `useLive`: zones and equipments, Sowel's reconnection | The tool has no modes, energy or activity |
| Zone pills with sparklines, shutters, water valves | Without them | No history, no such equipments yet |
| `CompactEquipmentCard` and its 35 controls | `EquipmentRow`: the light control and sensor values; other types name, icon and badge | The equipments the tool serves |

Everything else (colours, roles, scales, radii, shadows, motion, components) is Sowel's, unchanged.

## Layout

Sowel's application layout: a sidebar on the left (260 px, `bg-surface`, right border), the mark
and the wordmark in a 49 px header, the rooms' tree below, the Administration section pinned at the bottom of the column, under the scrolling room tree; the room on the right on the
`bg-background` tint: its name as the title, Sowel's lights line and pills beneath it, then its
equipments by family. Above the room, a top bar on every page of the home (spec 037): the home's
name and where you are on the left, the home's time, the sun times and the link indicator on the
right; 56 px (`h-topbar`) below `md`, where it is fixed at the top over a safe-area band and holds
a menu button that opens the sidebar as a drawer (Sowel's pattern; Sowel switches at `lg`), and
49 px from `md` up, level with the sidebar's header. Code:
`ui/src/components/layout/{Sidebar,ZoneTree,TopBar,TopBarFrame,top-bar-context}.tsx`,
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
  commits and dates, and check the budget (first load 77.9 KB gzip after spec 040, 83.0 KB after spec 042: the Resources card of Settings and the Plugins page of Administration, which replaced the Recipes card, add 3.6 KB gzip over spec 040, the page 1.8 KB over the card).
