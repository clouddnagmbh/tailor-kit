# UI primitives

The building blocks of every new view. They use only the tokens in
`../styles/tokens.css` (tokens v2), work in light and dark, and handle focus and
keyboard themselves. **New UI uses these primitives and tokens; no ad-hoc buttons,
colours or sizes.** See them all at `?gallery=1` in the web dev server
(`npm run dev -w apps/web`, then `http://127.0.0.1:5173/?gallery=1&theme=both`).

```svelte
<script lang="ts">
  import { Button, Dropdown, EmptyState, Icon, IconButton } from "../ui";
</script>
```

Distributions import the same names from `@requesttailor/ui`.

## What to use when

| Need | Use | Notes |
| --- | --- | --- |
| The main action of a view | `Button variant="primary"` | One per view or dialog. |
| Other actions | `Button` (secondary) | Default variant. |
| Actions in toolbars, rows, headers | `Button variant="ghost"` or `IconButton` | `IconButton` needs `label` (name and tooltip). |
| Destructive confirmation | `Button variant="danger"` | Only on the confirming step, never as the first click. |
| Main action plus variants | `SplitButton` | "Commit" / "Commit and push". |
| A list of actions | `Dropdown` (opens `Menu`) | Context menus: `Menu` with `anchor={{ x, y }}`. |
| Pick one of 2–5 visible options | `SegmentedControl` | View modes, "Side by side / Unified". |
| Pick one of many | `Select` | Native, so long lists and screen readers work. |
| Switch between views of one thing | `Tabs` | Panel: `role="tabpanel"`, ids from `tabId`/`tabPanelId`. |
| A setting that applies at once | `Switch` | |
| A choice submitted later, multi-select | `Checkbox` | `indeterminate` for "some". |
| Text input | `TextField`, `TextArea` | Code and bodies stay in CodeMirror. |
| Counts, Git status, methods, "Pro" | `Badge` | `kind="count" | "status" | "method" | "label"`; `status` with `long` shows the word. |
| Filters, refs, environments | `Chip` | `caret` as a Dropdown trigger, `onremove` for filters. |
| A titled group in a panel | `Section` | `collapsible` for long sidebars. |
| A raised block or a choice tile | `Card` | `onclick` makes it a tile (no `actions` then). |
| A note across a view | `Banner` | info, ok, warn, danger; `dismissible`. |
| Nothing to show yet | `EmptyState` | Icon, what is missing, one sentence, ≤ 2 actions. |
| Waiting, shape unknown / known | `Spinner` / `Skeleton` | Buttons have `loading`. |
| Progress of a long task | `Progress` | `value`/`max` or indeterminate; `label` required (aria). |
| A shortcut | `Kbd keys="Mod+Enter"` | Mod is ⌘ on macOS, Ctrl elsewhere. |
| A person | `Avatar name="…"` | Initials, stable colour, never remote images. |
| A hint on hover/focus | `use:tooltip={"Text"}` or `Tooltip` | Not for essential information. |
| A modal | `Dialog` + `DialogFooter` | Extensions: `AppState.showDialog` (the host uses `Dialog`). |
| An icon | `Icon name="commit"` | Semantic names from `icons.ts`, never Lucide imports in features. |

## Spacing and layout rules

- Spacing only from `--rt-space-1…8` (2, 4, 6, 8, 12, 16, 24, 32 px). Inside a
  control 4–8, between controls in a row 8, between groups 12–16, around a view
  or dialog body 16–24.
- Heights from `--rt-control-sm/md/lg` (24, 28, 32). Rows in lists and trees use
  `--rt-control-md`; toolbars `sm` or `md`, not mixed in one row.
- Radius: `sm` for badges and small controls, `md` for controls and menu items,
  `lg` for cards, menus, dialogs, banners, `pill` for chips and switches.
- Type: `font: var(--rt-font-body)` by default; `--rt-font-title` for card and
  dialog titles, `--rt-font-heading` for page headings only, `--rt-font-small`
  for secondary text, `--rt-font-caption` for uppercase section labels.
- Text colours: `--rt-fg` content, `--rt-fg-muted` secondary, `--rt-fg-subtle`
  placeholders and hints only. All pass WCAG AA on the surfaces.
- Surfaces: `--rt-surface-1` canvas, `-2` cards and fields, `-3` menus and
  dialogs, `-sunken` tracks; `--rt-bg-hover`/`-active`/`-selected` for rows.
  Elevation: `--rt-elev-1` cards, `-2` menus, `-3` dialogs.
- Accent: `--rt-accent` for text, links and marks; `--rt-accent-solid` (+`-fg`)
  for filled controls; `--rt-accent-subtle` (+`-fg`) for selected rows and chips.
- Status: `--rt-{info,ok,warn,danger}-{bg,fg,border}`; method and lane colours
  stay as they were (`--rt-method-*`, `--rt-lane-1…8`).
- Motion: `transition: … var(--rt-dur-fast) var(--rt-ease)`; durations become 0
  under reduced motion.

## Do and don't

- Do give every icon-only control a label; `IconButton` makes it a tooltip.
- Do keep floating layers in the primitives: `Menu` and tooltips render in a
  portal, so `overflow: hidden` never clips them and they take the theme of their
  anchor.
- Do use plain words ("Park changes", "Bring in changes") with the Git glossary.
- Don't style `<button>` yourself or add colours in hex; if a token is missing,
  add it to `tokens.css` (both palettes; the test checks contrast).
- Do pass `data-testid` and `aria-*` straight to `Badge`, `Card`, `Banner`, `EmptyState` and
  `SegmentedControl` (a `class` adds to theirs); a `SegmentOption` takes `testid`.
- Don't nest interactive elements (no buttons in a clickable `Card`).
- Don't change a feature component's look while touching it for something else;
  move it to the primitives on purpose.

## Accessibility built in

- `Menu`: WAI-ARIA menu; arrows, Home/End, first letter, Enter/Space, ArrowRight
  and ArrowLeft for submenus, Escape returns focus to the trigger, Tab closes.
- `Tabs`, `SegmentedControl`: roving focus, arrows select.
- `Dialog`: `aria-modal`, focus in on open, Tab kept inside, Escape closes, focus
  back to where it was.
- Tooltips show on keyboard focus, hide on Escape; delay 500 ms, then instant
  while moving along a toolbar.

Tests: `menu.test.ts`, `tooltip.test.ts`, `primitives.test.ts`,
`../styles/tokens.test.ts`, and `apps/web/e2e/ui/gallery.spec.ts`.
