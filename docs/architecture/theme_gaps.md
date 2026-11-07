---
title: "Theme breaking points"
description: "Where the theme engine stops: what a theme plugin cannot express today when it tries to reproduce a full design system, and the kernel and web changes that would close each gap."
section: architecture
status: proposal
order: 421
---
# Theme breaking points (proposal)

**Status: built.** G1 to G22 are all built; "Themes" ([themes.md](themes.md)) describes each. The per-person choices (folded sidebar, pinned pages, open tabs, pane width) are kept in the browser under the person's id, so they follow the person on one device, not to another. The sections below keep the original reasoning for each gap.

[Themes, layouts and navigation](themes.md) describes what a theme plugin can do today: colour
tokens, three font stacks, one radius, a density, a composed layout of regions and blocks, a
`nav` tree and a checked stylesheet. That is enough for a recolour. It is not enough to make
Aether *feel* like a complete design system, because complete systems put the navigation in
different places and add shell behaviour that a stylesheet cannot fake.

This page lists every point where a theme breaks. Each gap was found by drawing the same screen
in fifteen public design systems (Material 3, Fluent 2, Apple HIG, Atlassian, Primer, Ant
Design, Lightning, Fiori, GOV.UK, Geist, EUI, Spectrum, Pajamas, Clarity, Polaris). Systems
appear below only as short examples. The theme plugins themselves are documented in their own
repository.

A gap is real only if a theme cannot work around it with `theme.css`. Translucent surfaces
(`backdrop-filter`), custom fonts (`@font-face` from `assets/`), active-item indicators and
row styling are already possible and are not listed.

## Summary

| Id | Gap | Kind | Themes that hit it |
|---|---|---|---|
| G1 | Icon set (**built**) | token | 11 |
| G7 | Nav display variants (**built**) | block | 6 |
| G10 | Search and command block (**built**) | block | 6 |
| G6 | Rail and collapsible sidebar (**built**) | region | 5 |
| G4 | Type scale (**built**) | token | 5 |
| G9 | Breadcrumbs block (**built**) | block | 5 |
| G2 | Shape scale (**built**) | token | 3 |
| G3 | Elevation and motion tokens (**built**) | token | 3 |
| G8 | Nav badges and counters (**built**) | nav | 3 |
| G11 | Second top-bar row (**built**) | region | 3 |
| G15 | Scope switcher (**built**) | block | 3 |
| G5 | Bottom bar region (**built**) | region | 2 |
| G14 | Global create action (**built**) | block | 2 |
| G16 | Per-app accent (**built**) | token | 2 |
| G17 | Page-state shell surfaces (**built**) | state | 2 |
| G18 | Pinned nav items (**built**) | state | 2 |
| G22 | Footer link columns (**built**) | block | 2 |
| G12 | Workspace tabs (**built**) | state | 1 |
| G13 | Back button and live tiles (**built**) | block | 1 |
| G19 | List-detail panes (**built**) | region | 1 |
| G20 | Focus style and skip link | token | 1 |
| G21 | Seed palette tooling (**built**) | tooling | 1 |

Kinds: **token** is a new key in the theme file; **region** and **block** extend composed layouts
(`layout_blocks.rs` and `composed/registry.ts`); **nav** extends `NavItem`; **state** needs
per-user or per-page state in the shell; **tooling** is build-time only.

## Tokens

### G1 Icon set (built)

Icons were imported straight from `carbon-icons-svelte`, so a Material or Fluent theme kept
Carbon glyphs.

*Built:* `"icons": "icons.svg"` names an SVG sprite of `<symbol id="name">` icons. The catalog
checks it (`theme_icons.rs`: plain shapes and presentation attributes only, each symbol
rebuilt), and the theme API returns it as `shell.icons`.

In the web app every icon is drawn by name with `<AppIcon name="…" />` (`web/src/lib/icons`).
Carbon is the default set behind the names (`registry.ts`, 191 names) and nothing else
imports it: an ESLint rule refuses a direct import, and tests check that every name used in
the code exists and that the docs list matches the registry. Shared components that took an
icon component (`Empty`, `SearchPicker`) take an icon name. A theme can therefore replace
any icon in the app. The *Decide* below was settled: sprites only, no second built-in set.

### G2 Shape scale (built)

`radius` was one length, and buttons, fields and cards were hard-coded square, so even that
length did not reach them.

*Built:* `shape` in the theme file: a `scale` (`xs` … `4xl`) and `roles` (`button`, `field`,
`card`, `chip`, `menu`, `dialog`, `tooltip`, `tile`, `avatar`), each a step, `none`, `full` or a
length. The server checks it (`validate_shape`), the web app writes `--shape-*` variables, and
Tailwind's `rounded-<step>` and `rounded-<role>` utilities read them. The UI primitives and
the themeable page surfaces use the role utilities. A role takes one value for every corner
or up to four (top-left, top-right, bottom-right, bottom-left), so Material's filled field is
`"field": "xs xs none none"`.

### G3 Elevation and motion tokens

`--shadow-*` is set in `routes/layout.css` and happens to be overridable as a colour token, but
this is undocumented, and the token check is meant for colours. There are no motion tokens.

*Proposal:* document `shadow-xs` … `shadow-xl` as a group of the theme file (`"elevation": {}`),
with values checked as shadow lists. Add `"motion": { "duration-short", "duration-medium",
"easing-standard", "easing-emphasized" }`. Components read them, and
`prefers-reduced-motion` zeroes the durations.

### G4 Type scale (built)

Only the families were themeable. GOV.UK's 48px bold headings, Material's 400-weight display,
Fiori's light KPI numbers and Spectrum's heavy titles all need sizes and weights.

*Built:* `type` in the theme file: a `scale` (`xs` … `5xl`, Tailwind's `--text-<step>`) and
`roles` (`display`, `headline`, `title`, `heading`, `subheading`, `body`, `label`, `caption`,
`metric`, `button`, `column`), each with `size`, `line_height`, `weight`, `tracking`, `family` and `case`. The
server checks it (`validate_type`), the web app writes `--type-<role>-*` variables, and the
`type-<role>` utilities read them. Page and section headings, card, dialog and sheet titles,
labels, descriptions, stat values, button labels, table column headers and the body text use
the roles.

### G16 Per-app accent

Some shells colour the header for each app (Clarity, Lightning console). A theme cannot know
which app is open.

*Proposal:* `"apps": { "<app route or plugin>": { "primary": "…", "header": "…" } }` overrides
a few tokens while that app is current. The shell sets `data-app="<name>"` on the root, so a
stylesheet can also target it.

### G20 Focus style and skip link

`ring` sets the focus colour only. GOV.UK's focus is a yellow fill with a thick black underline.

*Proposal:* `"focus": { "width", "offset", "style": "ring" | "fill" }`, and a `skip-link` block
that every built-in layout places first.

## Regions

### G5 Bottom bar region

Phones need bottom navigation (Material) or a tab bar (Apple). The only regions are `banner`,
`topbar`, `sidebar`, `aside` and `footer`, and the footer scrolls with content.

*Proposal:* a `bottombar` region, fixed, padded by the safe-area inset, with `show_below`
(`sm`, `md`). A `nav` block inside it draws icon-over-label items. The usual pattern is a
sidebar with `hide_below: "md"` and a bottombar with `show_below: "md"`, so `show_below` also
becomes a general block and region option.

### G6 Rail and collapsible sidebar

The sidebar is drawn or not. Material's rail, Fluent's collapsing pane, EUI's dock and undock,
and Clarity's and Pajamas' collapse button all need states.

*Proposal:* sidebar options
`"mode": "full" | "rail" | "collapsible"`, `"collapsed_width"`, `"overlay": true` (floats over
content instead of pushing it) and a `sidebar-toggle` block. The collapsed state is
remembered for each person (see G18 for where that state lives). In `rail` mode, `nav` and
`app-menu` draw icons with short labels underneath. Items need an `icon` (G8).

### G11 Second top-bar row (built)

GitHub-style tabs under a header, Clarity's subnav, and Lightning's object tabs plus workspace tabs
all need more than one bar.

*Built:* a `subbar` region rendered directly under `topbar` with the same block options (`tone`).
It is not sticky. Primer, Clarity, Lightning put their tabs in it, and Ant and GOV.UK their
breadcrumbs. More rows can come later if a theme needs them.

### G19 List-detail panes

Apple's sidebar, list and detail layout cannot be done with regions, because the list belongs to the page.

*Proposal:* this is a page feature, not a shell one. A list view gains `open="pane"`, which
opens the record form beside the list instead of on a new route, with a draggable divider.
Theme CSS styles it through new parts `list.pane` and `form.pane`.

## Blocks

### G7 Nav display variants (built)

`nav` and `app-menu` drew links, with groups as dropdowns. The systems need:

| Variant | Example | Shape |
|---|---|---|
| `tree` | Fluent, Pajamas, Polaris | groups expand in place; Polaris shows only the active group's children |
| `tabs` | Primer, Fiori spaces, Clarity subnav | underlined horizontal tabs, overflow into a "More" menu |
| `mega` | Ant Design | a group opens a panel of columns; items may carry a `description` |
| `tiles` | Fiori | groups become tile sections (see G13) |

*Built:* `variant` (`links`, `tree`, `tabs`, `mega`, `columns`, `tiles`) and `expand` (`toggle`, `active`, `all`)
on `nav` and `app-menu`, checked by `layout_blocks.rs`, and `description` on theme nav items.
Both blocks draw through one component (`layout/composed/nav/`), which also marks only the
longest matching link current. App menu entries take their `description` from the page's
`menu_description`; `columns` is for footers (G22) and `tiles` is a launchpad (G13). Tabs share
the top bar with the other blocks unless the layout puts them in the `subbar` (G11).

### G9 Breadcrumbs block (built)

`components/layout/crumbs.ts` built trails for some desk pages, but only the page's control
panel drew them, and no block could draw one in a region.

*Built:* a `breadcrumbs` block with `style` (`chevron`, `slash`, `arrows`) and `scope` (`true`
puts the organization first). Pages publish their control panel's crumbs to the shell
(`pageTrail`); the block adds the app and, for pages that give only a title or nothing, the
app menu path. While the block is shown, the control panel keeps only the page title. With
`scope`, the organization crumb opens the organization switcher. Ant and GOV.UK put the trail
in the `subbar` of G11.

### G10 Search and command block (built)

`ScreenSearchPalette.svelte` (Ctrl+K) was mounted only in the old `Navbar.svelte`, so composed
layouts lost it. Geist and Pajamas are built around it, and Primer and Atlassian put a search
box in the header.

*Built:* the palette is mounted in every composed and custom layout. A `search` block
(`style`: `field`, `button`, `icon`; `shortcut`: `k`, `/`) puts it on screen. Plugins add
entries with `[[app.palette]]` (`label`, `href`, `icon`, `keywords`, `shortcut`, `roles`;
`[[command]]` was already taken by management commands). The server checks them when the plugin
loads and filters them by role in `GET /api/ui/apps`; the palette lists them first and their
shortcuts work anywhere. An entry can also run a plugin function, and a `search` function adds
records to the palette's results. The `field` style of the block still lists screens only.

### G13 Back button and live tiles (built)

Fiori opens apps full screen with a back arrow in the shell, and its launcher tiles show live
numbers.

*Built:* a `back` block (browser history, else the home page; hidden on the home page), the
`[app.tile]` table (`function`, `label`, `unit`) whose number the launcher tile shows, and the
`tiles` nav variant (a launchpad of tiles with icon, description and badge). Tile numbers use
the same refresh as nav badges: on appearing, every minute, on focus, and when a plugin event
or notification arrives.

### G14 Global create action

Material's FAB and Atlassian's Create button open "new record" from anywhere.

*Proposal:* a `create` block (`style`: `button`, `fab`) listing the create actions of installed
apps that the person may use. Apps declare them as `[[app.create]] label, href`.

### G15 Scope switcher (built)

`org-switcher` and `company-switcher` are fixed. Atlassian projects, Pajamas groups and
Geist teams switch a plugin-defined scope.

*Built:* a `scope-switcher` block whose `source` is a `plugin.function` answering the scopes
(`{ id, label, href }`, one current). The organization crumb of `breadcrumbs` with `scope`
opens the organization switcher. Themes do not place the block themselves: it needs a plugin
that defines the scopes.

### G22 Footer link columns (built)

The footer took inline blocks. GOV.UK and Ant footers are columns of grouped links.

*Built:* `nav.footer` in the theme file (a list of items, groups become columns) and the
`columns` variant, drawn by a `nav` block with `source: "footer"` in the footer region. The
footer region wraps, so the columns sit above any text or links beside them.

## Navigation data

### G8 Nav badges and counters (built)

`NavItem.icon` was "reserved for layouts that draw icons", and most did not draw it. There was no
count. Primer tabs and Pajamas' top row show counts.

*Built:* every nav variant draws `icon` and a badge. A page sets `menu_badge="<function>"` (its
own plugin's function, or `plugin.function`), which `GET /api/ui/apps` passes on as the menu
entry's `badge`; a theme nav item sets `badge` to fixed text or `{ "function": "plugin.fn" }`.
The web app calls each shown function through `POST /api/plugins/{plugin}/{fn}` (so the
function's own permission checks apply) once, then every minute, on window focus and after a
navigation. Counts are polled, not pushed; live counts would need a push channel.

## Shell state

### G12 Workspace tabs

Lightning's console keeps opened records as tabs.

*Proposal:* a `workspace-tabs` block. The shell keeps an ordered list of opened routes for each
person and tab (not synced across devices at first), with close, pin and a limit.

### G17 Page-state shell surfaces

Polaris replaces the top bar with a save bar while a form has edits. Spectrum shows a floating
action bar when list rows are selected.

*Proposal:* the shell exposes page state (`form.dirty`, `list.selection`) through
`shellContext`. Two blocks react to it: `save-bar` (drawn in place of its region's other blocks
while a form is dirty) and `action-bar` (floats over content with the list's bulk actions).

### G18 Pinned nav items

EUI and Pajamas let a person pin destinations.

*Proposal:* a per-person UI preference store (also used for G6's collapsed state). A `pinned`
block lists pins, and `nav` variants show a pin button on hover when the block exists.

## Tooling

### G21 Seed palette tooling

Material derives every colour role from one seed colour. That is an authoring step, not a runtime
one.

*Proposal:* `aether theme palette --seed "#6750a4" --scheme tonal` prints `light` and `dark`
token maps into a theme file. Nothing changes at runtime.

## Build order

1. **G1, G2, G4** (built): icon sets and token scales. Nearly every theme hits these, they touch only
   the theme file and the web app, and they unblock the most visual fidelity.
2. **G7, G8, G9, G10** (built): nav variants, breadcrumbs, the search block and item icons and badges.
   These cover most navigation patterns.
3. **G5, G6, G11** (built): new regions and sidebar modes.
4. **G3, G20, G22, G14, G15, G16** (built): smaller token and block additions.
5. **G17, G18, G12** (built): shell state, which needs a per-person preference store.
6. **G13, G19, G21** (built): launcher tiles, panes, palette tooling.

## Decide

- G6, G12, G18: decided for now: the browser, under the person's id. Moving it to the server (so it
  follows the person across devices) needs a per-person preference store in the kernel, which does not exist yet.
