# Design system

How the interface of mini-Sowel looks: **Sowel's design system**, adapted to a web app served by
the box (decision D4). A screen of mini-Sowel looks like a screen of Sowel. A feature that needs a
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
| [`ui/src/styles/embedded.css`](../ui/src/styles/embedded.css) | What mini-Sowel changes in the tokens for the box, loaded after them |
| [`ui/src/styles/theme.css`](../ui/src/styles/theme.css) | The Tailwind 4 theme: the tokens as utilities (`bg-surface`, `text-primary`, `text-small`, `rounded-md`...) |
| [`ui/src/index.css`](../ui/src/index.css) | Loads Tailwind, then the three files above in this order |
| [`ui/eslint/design-classes.js`](../ui/eslint/design-classes.js) | Sowel's design rules, run by ESLint |
| [`ui/scripts/check-bundle-budget.mjs`](../ui/scripts/check-bundle-budget.mjs) | The first-load budget |

The theme is chosen by `data-theme="hybrid"` or `data-theme="dark"` on `<html>`; Hybrid is the
default.

## Embedded: what changes from Sowel

| Sowel | mini-Sowel | Why |
| ----- | ---------- | --- |
| Inter (variable) and JetBrains Mono, served from `@fontsource` | System fonts: `system-ui` for text, `ui-monospace` for code (`embedded.css`) | The box serves its pages from its flash: no room for web fonts |
| React 19 | Preact, with `preact/compat`: Sowel's React components work unchanged | About 4 KB instead of about 45 KB gzip |
| Lucide icons by name, one rule against the whole set | The same rule; icons by name only | Every icon counts in the budget |
| First load under 1.4 MB raw | First load under **170 KB gzip**, checked by `npm run check:bundle` in the validate command | The framing note: 100 to 170 KB gzip, served from flash |
| Sidebar: logo and "SOWEL" in Nunito | Logo and "MINI-SOWEL" in the system font | No web font |

Everything else (colours, roles, scales, radii, shadows, motion, components) is Sowel's, unchanged.

## Layout

Sowel's application layout: a sidebar on the left (260 px, `bg-surface`, right border), the logo
and the tool's name in a 49 px header, the navigation below; the page on the right on the
`bg-background` tint, its title in `text-display`. Code: `ui/src/components/layout/Sidebar.tsx`,
`ui/src/App.tsx`.

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
  buttons, `strokeWidth={1.5}` as in Sowel.
- **Updating from Sowel**: copy the four files again from a newer Sowel commit, update the commit
  and date under Source, and check the budget.
