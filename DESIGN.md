---
name: Synbot Console
description: The admin console for a self-hosted personal AI agent runtime — neutral SaaS surfaces, one accent per operator.
colors:
  background: "#ffffff"
  surface: "#ffffff"
  surface-2: "#f4f4f5"
  ink: "#17181b"
  ink-2: "#5c5f66"
  ink-3: "#676a72"
  line: "#e7e7ea"
  line-strong: "#d5d6da"
  accent: "#2f6fed"
  accent-hover: "#255fd6"
  accent-fg: "#ffffff"
  focus: "#2f6fed"
  success: "#0d7a52"
  warning: "#9a5a0c"
  danger: "#c3352d"
  info: "#2a63d4"
  dark-background: "#0b0c0e"
  dark-surface: "#131417"
  dark-surface-2: "#1a1c20"
  dark-ink: "#e9eaec"
  dark-ink-2: "#a2a6ae"
  dark-ink-3: "#8a8f98"
  dark-line: "#242629"
  dark-line-strong: "#33363b"
typography:
  page:
    fontFamily: "Geist, -apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif"
    fontSize: "20px"
    fontWeight: 600
    lineHeight: 1.25
    letterSpacing: "-0.01em"
  section:
    fontFamily: "Geist, -apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.3
  body:
    fontFamily: "Geist, -apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.45
  small:
    fontFamily: "Geist, -apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
  micro:
    fontFamily: "Geist, -apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
  mono:
    fontFamily: "'Geist Mono', ui-monospace, 'SF Mono', Menlo, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  control: "6px"
  panel: "10px"
  pill: "999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "16px"
  page: "24px"
  page-compact: "16px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-fg}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "32px"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
    textColor: "{colors.accent-fg}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.control}"
    height: "32px"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    height: "32px"
  panel:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
  user-turn:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "12px 14px"
  table-row:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    height: "36px"
---

# Design System: Synbot Console

## Overview

**Creative North Star: "The Instrument Panel"**

Synbot is the admin surface of a self-hosted agent runtime. The operator is not browsing; they are
watching a turn stream, approving a command, reading a diff, and switching to the log tail to find
out why a channel dropped. The console therefore behaves like an instrument panel: one neutral
substrate, one accent that only ever means "this is the live control", and typography that keeps
numbers from moving while they update.

The visual language is deliberately borrowed from the current generation of developer tools
(Linear's density, Vercel's restraint) rather than invented. Familiar patterns are the point:
operators arrive with expectations about tables, dialogs, tabs and focus rings, and meeting those
expectations is faster than teaching a new one. Expression lives in the details — tabular numerals,
1px rules instead of shadows, a tool row that reads like a log line.

Both themes carry the same structure: a near-neutral substrate (`#ffffff` / `#0b0c0e`) with a single
accent ramp chosen by the operator (six schemes, light and dark variants each). Colour is a
selection, not a re-skin: switching scheme changes the accent and nothing else.

**Key Characteristics:**
- Neutral substrate, one accent ramp, six operator-chosen hues.
- Flat at rest: 1px rules separate surfaces, shadows belong to overlays only.
- Dense but quiet: 13px body in tables, 12px metadata, mono for every number, ID and timestamp.
- Two languages (zh/en) at equal quality; dates, durations and enum labels are localised.
- Verified against WCAG 2.1 AA with axe on all 13 surfaces in both themes.

## Colors

A single neutral scale plus one accent. The scale is warm-less grey; the accent is the only
saturated colour on the screen.

### Primary
- **Operator Accent** (`#2f6fed` default "blue-ocean", `#12805c` forest, `#6f4df7` mystic,
  `#b84a10` sunset, `#db2777` blossom, `#3f4147` modern): primary buttons, the active rail row's
  2px left rule, focus rings, running indicators, selected table rows. Nothing else.
- **Accent Hover** (`#255fd6` and per-scheme `*-600`): one step down in lightness, used for hover
  and pressed states of accent-filled controls.

### Neutral
- **Paper** (`#ffffff` light / `#0b0c0e` dark background): the page substrate. Never tinted.
- **Sheet** (`#ffffff` light / `#131417` dark surface): panels, dialogs, the AppBar.
- **Inset** (`#f4f4f5` light / `#1a1c20` dark surface-2): code blocks, table headers, hover rows,
  the user's own turn in the transcript.
- **Ink** (`#17181b` / `#e9eaec`): primary text.
- **Ink-2** (`#5c5f66` / `#a2a6ae`): supporting text, table cells that are not the key column.
- **Ink-3** (`#676a72` / `#8a8f98`): metadata only — counts, timestamps, hints, section labels.
- **Line / Line-strong** (`#e7e7ea`/`#d5d6da`, `#242629`/`#33363b`): separators and control borders.

### Status
- **Success** (`#0d7a52`), **Warning** (`#9a5a0c`), **Danger** (`#c3352d`), **Info** (`#2a63d4`),
  each with a lighter dark-theme counterpart. They appear only when they carry meaning: a channel
  failed, an approval is pending, a turn was cancelled.

### Named Rules
**The One Accent Rule.** A screen has exactly one accent. It marks the primary action, the current
selection, the focus ring, or a live process — never decoration, never two of those at once in the
same block.

**The Contrast Floor Rule.** Every text token clears 4.5:1 against every surface it can land on,
including its own 10% tint. Status hues and the two light accents that measured under the floor were
darkened one step to meet it; the ramp is the source of truth in `web/src/styles/tokens.css`.

## Typography

**Display Font:** Geist (self-hosted variable woff2, OFL) with `-apple-system`, `PingFang SC`,
`Microsoft YaHei` fallbacks so Chinese renders in the platform's CJK face.
**Body Font:** Geist, same stack.
**Label/Mono Font:** Geist Mono for numbers, IDs, paths, code, log times and durations.

**Character:** Geist is a neutral grotesque that stays legible at 12px and does not editorialise;
the mono companion gives the console its instrument-panel voice where figures matter.

### Hierarchy
- **Display** (600, 26px/1.2, -0.01em): login page and the chat empty state only.
- **Page** (600, 20px/1.25, -0.01em): every `PageHeader` title.
- **Section** (600, 16px/1.3): panel headings, inspector title, detail headings.
- **Body** (400, 14px/1.45): prose in the transcript, form values, dialog copy.
- **Small** (400, 13px/1.45): table cells, buttons, form labels, navigation.
- **Micro** (400/500, 12px/1.45): metadata rows, column headers (uppercase, `tracking-wide`),
  chips, tool-row names and durations, group labels.
- **Mono** (400, 12px, tabular): IDs, paths, code, log times, token counts, durations.

### Named Rules
**The Tabular Rule.** Any figure that can change while the operator watches it — durations, token
counts, log timestamps, table numerics — is set in Geist Mono with `font-variant-numeric:
tabular-nums`, so the layout never shifts mid-stream.

**The Four-Size Rule.** A screen uses at most four of these steps. Density is not an excuse to
invent a fifth size.

## Layout

A fixed 52px AppBar, a rail that is 232px expanded and 64px collapsed (state in
`localStorage: shell-rail`), and `<main>`. Every route except the workbench scrolls with 24px
padding (16px at or below 768px); the chat route is `h-full overflow-hidden` and keeps the whole
viewport for the transcript.

The transcript column is capped at 760px and centred; the workspace inspector is a fixed 320px.
Detail pages that pair a list with a panel use `minmax(0, 30rem)` for the list so the panel keeps a
readable measure, and tables inside those columns drop the 640px minimum so no column is ever cut.
Below `lg` the pairs stack; below 768px the rail keeps its width and content scrolls rather than
reflowing into a mobile layout.

Spacing is a 4px scale: 4/8/12/16/24. Table rows are 40px comfortable and 36px compact; the table
header is 32px and sticky.

## Elevation & Depth

**Flat at rest.** No resting surface carries a shadow. Depth is expressed by tonal layering
(`surface` → `surface-2` for insets) and by 1px `line` rules between regions. A card is a bordered
sheet, not a floating object.

Shadows exist for exactly one situation: something floats above the page and must read as detached.

### Shadow Vocabulary
- **overlay** (`0 12px 32px rgba(0,0,0,.14), 0 1px 2px rgba(0,0,0,.08)`): dialogs, the command
  palette, popover menus, toasts. Nothing else.

## Shapes

Controls are 6px, panels 10px, pills 999px. Nothing is fully square and nothing is a capsule except
chips, badges and the "jump to latest" pill. Borders are always 1px; there are no 2px panel frames
and no double borders. The one deliberate exception is the 2px left rule on approval cards and the
rail's active row: a rule, not a frame, marking state or consent.

## Components

### Buttons
- **Shape:** 6px radius, 28–36px tall by size, 12px horizontal padding.
- **Primary:** accent fill, `accent-fg` text, one per screen.
- **Ghost:** transparent with `ink-2` text; hover adds `surface-2`. Used for secondary actions.
- **Danger:** ghost or bordered in `danger`; never a filled red block next to a primary.
- **Hover / Focus:** background and border transitions at 120ms `ease-out`; focus is a 2px
  `focus` outline with 2px offset, applied through `:focus-visible` and never removed.

### Chips and Badges
- **Style:** `pill` radius, 12px text, 10% tinted background of the tone with a 35% tinted border.
- **State:** neutral for categories, status hues only for real state, accent for the active filter.

### Cards / Containers
- **Corner Style:** 10px.
- **Background:** `surface`; insets use `surface-2`.
- **Shadow Strategy:** none at rest (see Elevation).
- **Border:** 1px `line`.
- **Internal Padding:** 12–16px.

### Tables
- **Shape:** no card shell, no zebra, no vertical rules; 1px horizontal separators only.
- **Header:** 32px, uppercase 12px `ink-3`, sticky, sortable columns show `aria-sort`.
- **Rows:** 36/40px, hover tints `surface-2`, clickable rows are focusable and answer Enter/Space.
- **Overflow:** the wrapper scrolls horizontally rather than the page.

### Inputs / Fields
- **Style:** 1px `line` stroke on `surface`, 6px radius, 32px tall, 13px text; mono for
  identifiers, values and secrets.
- **Focus:** accent outline, 2px offset; the border does not change colour.
- **Error:** `danger` text under the field plus a 10% danger tint on the border; the label stays.

### Navigation
- **Rail:** grouped (Workspace / Automation / Extensions / System), 32px rows, 13px labels,
  `surface-2` plus a 2px accent left rule on the active row, tooltips when collapsed.
- **AppBar:** product mark, ⌘K search, daemon status dot, appearance, language, account.
- **Command palette:** ⌘K/Ctrl+K, listbox semantics, ↑↓/↵/Esc, Tab trapped inside, focus returned
  to where it came from.

### Transcript rows (signature component)
The chat is a log, not a chat. The user's turn is an inset block (`surface-2`, 10px radius,
12/14px padding); the assistant's is plain text with a 12px metadata line beneath. Tool calls are
28px rows: status dot, mono name, truncated argument summary, tabular duration, chevron; expanded
they become a `surface-2` code block. A turn ends with a 1px rule plus a mono summary line
(duration · tokens · files changed). Notices are 12px `ink-2` text behind a 1px left rule.

## Do's and Don'ts

### Do:
- **Do** keep one accent per screen and let it mark action, selection, focus or liveness.
- **Do** separate regions with a 1px `line` rule; reserve shadows for overlays.
- **Do** set every number that can change in Geist Mono with tabular figures.
- **Do** localise dates, durations and enum labels; a Chinese UI never prints `connected` or
  `2/11/2026, 5:24:18 PM`.
- **Do** keep a keyboard path for every action, and let `:focus-visible` be visible everywhere.
- **Do** ship both themes and both languages as first-class: every new surface is checked in
  light, dark, zh and en.

### Don't:
- **Don't** introduce a second accent, a gradient, a glow, a coloured shadow or a glass panel.
- **Don't** wrap tables and lists in cards, or add zebra striping to a table.
- **Don't** use emoji, icon walls, decorative illustration or a badge for every state.
- **Don't** animate anything that is not `transform`, `opacity`, `background-color` or
  `border-color`, and never at rest.
- **Don't** hard-code English in a component that renders in both languages.
