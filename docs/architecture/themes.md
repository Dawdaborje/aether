---
title: "Themes, layouts and navigation"
description: "What the web app looks like, and how it is laid out, is decided per organization by the theme plugin installed there. Nothing about it is hard-coded in the app."
section: architecture
status: stable
order: 420
---
# Themes, layouts and navigation

What the web app looks like, and how it is laid out, is decided per
organization by the **theme plugin** installed there. Nothing about it is
hard-coded in the app.

## A theme plugin

A theme plugin declares `[theme]` in `plugin.toml` and points `tokens_file` at a
JSON file:

```toml
[plugin]
name = "theme_sea"
version = "0.1.0"
[plugin.meta]
kind = "theme"

[theme]
name = "sea"
label = "Sea"
tokens_file = "./theme.json"
```

```json
{
  "name": "sea", "label": "Sea", "color_mode": "light",
  "radius": "0.5rem", "font_sans": "system-ui",
  "layout": "custom",
  "nav": {
    "header": "Sea Corp",
    "items": [
      { "label": "Apps", "href": "/desk/apps" },
      { "label": "Sales", "children": [
          { "label": "Orders", "href": "/sales/orders" },
          { "label": "Customers", "href": "/sales/customers" } ] },
      { "label": "Docs", "href": "https://example.com/docs" }
    ]
  },
  "light": { "background": "oklch(0.98 0.01 200)", "primary": "…" },
  "dark":  { "background": "oklch(0.2 0.02 220)",  "primary": "…" }
}
```

| Key | Meaning |
|---|---|
| `light`, `dark` | colour tokens (required) |
| `radius`, `font_sans`, `color_mode` | optional; `color_mode` is `light`, `dark` or `system` |
| `font_mono`, `font_heading` | optional font stacks for code and headings (headings use `font_sans` otherwise) |
| `shape` | corner radii: a scale and what each kind of element uses (see below) |
| `type` | text sizes: a scale and how each kind of text is set (see below) |
| `density` | `compact`, `comfortable` (default) or `spacious`: the spacing scale |
| `layout` | which layout renders the app (default `default`); a built-in name or one of the plugin's `layouts/` |
| `error_pages` | which set of error pages the app shows (default `default`) |
| `nav` | the navigation for that layout; omit it to use the layout's built-in one |
| `home` | `{ "route": "/sales/dashboard" }`: where members land after login and where the brand links lead (default the app launcher, `/apps`) |
| `branding` | `{ "product_name", "logo", "favicon" }`: shown by the `brand` block and the browser tab |
| `launcher` | how `/apps` lays out the installed apps (see "The apps screen and the desk") |
| `css` | a stylesheet in the plugin (see below) |
| `icons` | an SVG sprite in the plugin whose symbols replace the app's icons (see below) |

Colour tokens are written into the app's stylesheet as `--name: value`, so names
are lowercase words joined by `-` and values cannot contain `;`, `{`, `}`, `<`, `>`
or `\`. The same holds for the fonts and `radius`. Logos and favicons are files
under the plugin's `assets/` folder (`"assets/brand/logo.svg"`, served from
`/api/ui/plugins/<plugin>/assets/brand/logo.svg`) or `https://` URLs.

The built-in default theme is IBM Carbon (White for light, Gray 100 for dark) set in
IBM Plex, with `radius` `0`. The look it replaced ships as the `theme_neutral` plugin in
the base workspace. Carbon added these tokens. A theme that leaves them out gets them
derived from its own colours (`DERIVED_TOKENS` in `web/src/lib/theme/applyTheme.ts`):

| Token | Used for | Derived from |
|---|---|---|
| `field`, `field-hover` | background of inputs, selects and pickers | `card` |
| `border-strong` | the bottom border of fields | `input` |
| `layer-hover` | hovered rows, menu items and ghost buttons | `card` and `foreground` |
| `primary-hover`, `secondary-hover` | hovered primary and secondary buttons | `primary`, `secondary` |

`nav` is `{ header?, items, footer? }` (`footer` is a list of items for footer link columns; a theme may give only `footer` and `"items": []`); an item is `{ label, href?, icon?, description?, badge?, children? }`
(`description`: at most 200 characters, shown by the `mega` variant; `badge`: fixed text of at
most 12 characters such as `"New"`, or `{ "function": "<plugin>.<function>" }` for a count,
as with a page's `menu_badge` in docs/plugin/pages.md)
and needs an `href`, `children`, or both. Hrefs are app paths (`/apps`) or
`http(s)` URLs: `javascript:` and `//host` are refused. At most 50 items, nested
at most 3 deep.

## A stylesheet

A theme may ship CSS: `"css": "theme.css"` in its theme file, a path inside the plugin. It is
applied to the whole app after the stock styles, so it wins. Target the parts widgets expose
(`[data-part="list.row"]`, `[data-part="form.field"]`; the list is in `docs/plugin/pages.md`)
rather than class names, which change.

```css
[data-part="list.row"]:hover { background: color-mix(in oklab, var(--primary) 6%, transparent); }
[data-part="kanban.card"] { border-left: 3px solid var(--primary); }
@font-face { font-family: "Sea"; src: url("assets/fonts/sea.woff2") format("woff2"); }
```

The file is checked when the plugin loads, and the plugin is refused if it:

* is over 64 KB, or has unbalanced braces;
* uses `@import`, `@namespace`, `@charset`, `@page`, or any at-rule other than `@media`,
  `@supports`, `@layer`, `@container`, `@keyframes` and `@font-face`;
* uses `url()` for anything but a file under the plugin's own `assets/` folder (those are
  rewritten to `/api/ui/plugins/<plugin>/assets/…`), or `image-set()`, `src()`, `element()`;
* uses `expression()`, `behavior`, `-moz-binding`, `javascript:` or a backslash escape.

A stylesheet therefore cannot fetch anything from elsewhere or run anything. It can still
change how the app looks, including hiding parts of it: install themes you trust.

## Shape

`radius` sets the size of the app's corners. `shape` says which corners go where:

```json
"radius": "0.75rem",
"shape": {
  "scale": { "xs": "4px", "sm": "8px", "md": "12px", "lg": "16px", "xl": "28px" },
  "roles": { "button": "full", "field": "xs", "card": "md", "chip": "sm",
             "menu": "xs", "dialog": "xl", "tooltip": "xs", "tile": "xl", "avatar": "full" }
}
```

* `scale` gives steps `xs`, `sm`, `md`, `lg`, `xl`, `2xl`, `3xl` and `4xl` a length (`0`, or a
  number with `px`, `rem`, `em` or `%`). Steps it leaves out follow `radius`
  (`xs` 0.3×, `sm` 0.6×, `md` 0.8×, `lg` 1×, `xl` 1.4×, …). Every `rounded-sm`, `rounded-lg`, …
  in the app uses these, plugin pages included.
* `roles` sets the corners of each kind of element to a step, `none`, `full` (a pill) or a
  length. Up to four values set the corners one by one, in CSS order (top-left, top-right,
  bottom-right, bottom-left): `"field": "xs xs none none"` rounds only the top.

| Role | Used by | Default |
|---|---|---|
| `button` | buttons and button groups | `md` |
| `field` | inputs, text areas, selects, input groups | `md` |
| `card` | cards, the page sheet, list tables, chart and board cards, kanban cards | `lg` |
| `chip` | badges and tags | `full` |
| `menu` | dropdown menus, popovers, select lists, menu bars | `md` |
| `dialog` | dialogs and alert dialogs | `lg` |
| `tooltip` | tooltips | `md` |
| `tile` | app launcher tiles and folders | `lg` |
| `avatar` | avatars | `full` |

The built-in theme's `radius` is `0`, so with no `shape` everything is square except chips
and avatars. The values become `--shape-<step>` and `--shape-<role>` on the page, and the
classes `rounded-<step>` and `rounded-<role>` read them, so plugin components can use
`rounded-card` or `rounded-button` to follow the theme.

## Type

`font_sans`, `font_mono` and `font_heading` choose the fonts. `type` sets sizes, weights and
spacing:

```json
"type": {
  "scale": { "sm": "0.875rem", "4xl": "3rem" },
  "roles": {
    "display":  { "size": "4xl", "line_height": 1.04, "weight": 700 },
    "headline": { "size": "2.25rem", "line_height": "2.5rem", "weight": 700 },
    "caption":  { "size": "xs", "weight": 600, "tracking": "0.06em", "case": "upper" },
    "metric":   { "family": "mono", "tracking": "-0.04em" }
  }
}
```

* `scale` gives the font-size steps `xs`, `sm`, `base`, `lg`, `xl`, `2xl`, `3xl`, `4xl` and
  `5xl` a length (`rem`, `px` or `em`). Every `text-sm`, `text-lg`, … in the app uses these,
  plugin pages included. Line heights stay Tailwind's.
* `roles` sets how each kind of text looks. Each role takes any of:
  * `size`: a step of the scale or a length;
  * `line_height`: a number up to 4 or a length;
  * `weight`: 100, 200, … 900;
  * `tracking`: `normal` or a length in `em`, `px` or `rem` (may be negative);
  * `family`: `sans`, `heading` or `mono`;
  * `case`: `none`, `upper`, `lower` or `capitalize`.

| Role | Used by | Default |
|---|---|---|
| `display` | `h1` without a size class | 2rem / 2.5rem, 400, heading font |
| `headline` | page headers, dialog titles, `h2` without a size class | `xl` / 1.75rem, 400, heading font |
| `title` | the page title in the shell and the custom navbar | `base` / 1.5rem, 600, -0.025em |
| `heading` | section headings, sheet titles, calendar and call headings | `lg` / 1.75rem, 600, heading font |
| `subheading` | card titles, `h3` without a size class | `base` / 1.5rem, 600, heading font |
| `body` | the page's running text, card and field descriptions | `sm` / 1.25rem, 400, 0.16px |
| `label` | field labels | `xs` / 1rem, 400, 0.32px |
| `caption` | small headings over values: stat labels, field titles | `xs` / 1rem, 500, 0.025em, upper case |
| `metric` | key figures (stat values) | `2xl` / 2rem, 600 |
| `button` | button labels | `sm` / 1.25rem, 400, 0.16px |
| `column` | table column headers | `sm` / 1.25rem, 600, 0.16px |

The defaults are the built-in Carbon look. The values become `--type-<role>-size`, `-leading`,
`-weight`, `-tracking`, `-family` and `-case` on the page, and the class `type-<role>` applies
all six. Plugin components can use `type-title` or `type-caption` to follow the theme. A size or
weight class after it (`type-label text-sm`) still wins, and `cn()` keeps only the last
`type-<role>`.

## Icons

A theme may replace the app's icons: `"icons": "icons.svg"` in its theme file, an SVG file in
the plugin holding one `<symbol>` per icon. The symbol's `id` is the icon's name.

```xml
<svg xmlns="http://www.w3.org/2000/svg">
  <symbol id="bell" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
    <path d="M6 16v-5a6 6 0 0 1 12 0v5l1.5 2h-15z"/><path d="M10 21h4"/>
  </symbol>
  <symbol id="folder" viewBox="0 0 24 24"><path d="M3 6h7l2 2h9v11H3z"/></symbol>
</svg>
```

Every icon in the app is drawn by name, and Carbon is only the default set behind those
names, so a sprite can replace three icons or all of them. For each name the app uses the
theme's symbol if the sprite has one and the Carbon icon otherwise. A sprite may also add
names of its own, which apps (`[app] icon`), pages, nav items and `link` blocks can then use.

The names of the default set (`web/src/lib/icons/registry.ts`; a test keeps this list in step):

<!-- icon-names:start -->
`alarm-clock` `app-window` `archive` `arrow-down` `arrow-left` `arrow-right` `arrow-up-right`
`at-sign` `attachment` `badge-dollar-sign` `ban` `banknote` `bell` `book` `book-open` `boxes`
`briefcase` `building` `building-2` `calendar` `calendar-check` `calendar-days`
`calendar-range` `car` `chart-bar` `chart-column` `check` `check-check` `check-circle-filled`
`check-square` `chevron-down` `chevron-left` `chevron-right` `chevron-up` `chevrons-up-down`
`circle-x` `clipboard-check` `clipboard-list` `clock` `close` `cloud-off` `cog` `coins`
`contact` `credit-card` `database` `door-open` `download` `error-filled` `expand-screen`
`face-add` `factory` `file-check` `file-clock` `file-search` `file-spreadsheet` `file-text`
`file-up` `files` `filter` `flag` `flame` `folder` `folder-add` `folder-kanban` `folder-open`
`folder-tree` `form-input` `gift` `git-branch` `globe` `graduation-cap` `hand-coins`
`handshake` `hard-drive` `hash` `headset` `heart-pulse` `history` `home` `house` `image`
`in-progress` `inbox` `kanban` `key` `key-round` `laptop` `layers` `layout` `layout-dashboard`
`layout-grid` `library` `life-buoy` `list` `list-checks` `list-tree` `lock` `mail` `map`
`map-pin` `maximize` `megaphone` `message-circle-heart` `message-square` `message-square-quote`
`messages-square` `microphone` `microphone-off` `minus` `monitor` `moon` `more-horizontal`
`mouse-pointer-click` `music` `network` `newspaper` `notification-off` `package` `package-open`
`palette` `pen-line` `pencil` `pencil-line` `percent` `phone` `phone-off` `pin` `plane` `plug`
`plus` `presentation` `printer` `puzzle` `radio` `receipt` `refresh` `refresh-cw` `repeat`
`reply` `restart` `rotate-ccw` `rotate-clockwise` `save` `scale` `scan-text` `search`
`search-x` `send` `settings` `shield` `shield-check` `shopping-bag` `shopping-cart`
`side-panel-open` `sliders` `sliders-horizontal` `smile` `sparkles` `star` `sun` `table` `tag`
`target` `ticket` `timer` `trash` `trending-down` `truck` `undo` `upload` `user` `user-check`
`user-plus` `user-x` `users` `users-round` `video` `video-off` `view` `view-off` `wallet`
`warehouse` `warning` `warning-filled` `webhook` `wrench` `x-circle` `zap` `zoom-in` `zoom-out`
<!-- icon-names:end -->

Draw symbols in `currentColor` so they follow the text colour. A symbol with no `fill` is
filled with `currentColor`.

The sprite is checked when the plugin loads, and the plugin is refused if the sprite:

* is over 256 KB or holds more than 500 icons, or a name that is not lowercase letters,
  digits and `-`;
* has a symbol without a four-number `viewBox`, or defines a name twice;
* uses any element but `symbol` (in `svg` or `defs`), `g`, `path`, `circle`, `ellipse`,
  `line`, `polyline`, `polygon` and `rect`: no `use`, `image`, `text`, `style`, `script`,
  `foreignObject`, gradients or masks;
* uses any attribute but the shape's geometry, `transform` and presentation attributes
  (`fill`, `stroke`, `stroke-width`, `opacity`, …): no `style`, `class`, `href` or event
  handlers;
* has a value with anything but numbers, letters, spaces and `, . - + # %` (and
  parentheses in `transform`), so no `url(…)`.

The server rebuilds each symbol from what passed and returns the icons as
`shell.icons` on `GET /api/ui/theme`: `{ "bell": { "view_box", "attrs", "body" } }`.

## Elevation, motion, focus and per-app colours

Four more groups in `theme.json`. Each is checked when the plugin loads; a bad value refuses the theme.

```json
"elevation": { "sm": "0 1px 2px rgb(0 0 0 / 0.3)", "lg": "none" },
"motion": { "duration-short": "150ms", "duration-medium": "250ms",
            "easing-standard": "cubic-bezier(0.2, 0, 0, 1)" },
"focus": { "width": "3px", "offset": "0px", "style": "fill" },
"apps": { "crm": { "primary": "#0176d3", "header": "#032d60", "header_foreground": "#fff" } }
```

- **`elevation`**: steps `2xs xs sm md lg xl 2xl`, each `none` or a shadow list (numbers, colours,
  `inset`; no `url()`). The `shadow-*` classes read them.
- **`motion`**: `duration-short` and `duration-medium` (at most `2s`), `easing-standard` and
  `easing-emphasized` (a keyword or `cubic-bezier(a, b, c, d)`). Components use the
  `duration-short` and `duration-medium` utilities. People whose system asks for reduced motion
  get `0ms` whatever the theme says.
- **`focus`**: `width` and `offset` in `px` or `rem`, and `style`: `ring` outlines the focused
  element, `fill` also paints it with the colour tokens `focus-fill` and `focus-text`.
- **`apps`**: colours (`primary`, `header`, `header_foreground`) that replace the theme's while
  that app is open. The shell sets `data-app="<plugin>"` on the root, so a stylesheet can target
  it too.

### Fields and tables

```json
"field": { "style": "outline" },
"table": { "header": "plain", "rows": "zebra", "density": "compact" }
```

- **`field`**: how inputs, text areas, selects and pickers are drawn. `underline` (the built-in
  Carbon look), `outline` (a full border that turns to the primary colour on focus) or `filled`
  (a tinted box with a focus line).
- **`table`**: how lists are drawn. `header` is `filled` (default), `plain` (a line) or `bold`
  (a thick line); `rows` is `lines` (default), `zebra` or `plain`; `density` is `compact`,
  `default` or `relaxed`.

### The apps screen and the desk

`"launcher": { "variant": "grid" | "list" | "cards", "width": "narrow" | "default" | "wide" | "full" }`
sets how `/apps` lays out the installed apps. The grid has `data-part="apps.grid"` and each tile
`data-part="apps.tile"` with `data-variant`, so a stylesheet can go further.

Developers on `/desk/*` get the theme's layout like everyone else. The layout is kept; its
navigation shows the desk's pages in place of the app's.

## The built-in default

An organization with no theme plugin installed (or whose active theme was cleared
on the Appearance page) gets the **built-in enterprise theme**, baked into the web
app (`web/src/lib/theme/enterprise.ts`, mirrored in `routes/layout.css` so the
first paint already matches). It is deliberately simple: a cool off-white canvas,
ink text, one corporate blue, a deep navy sidebar, hairline borders and a modest
radius, in light and dark. Its layout is the built-in shell: a navy sidebar with
grouped navigation, and a top bar with the page title, organization switcher and
account menu. Whenever the server answers `source: "fallback"`, the app uses this
theme and ignores any tokens that came with the response.

When that fallback (or any theme) has no `nav` item for the current path, the
top bar still names the **installed app** whose `[app].route` matches (for
example Countries) and shows that app's page menus from `GET /api/ui/apps`. A
theme that already nests the path under `nav.items` keeps control of the brand
and menus.

## From plugin to screen

1. `aether --load-plugins <dir>` validates the theme (a bad layout name, nav or
   colours fails the load) and stores it in the core catalog.
2. `aether --install-plugin theme_sea --org acme` copies it into the
   organization's `ui_themes`. **The first theme an organization installs becomes
   its active theme**; later ones are added without switching.
3. `aether --activate-theme plain --org acme` switches the active theme.
4. `GET /api/ui/theme` returns the active theme for the caller's organization
   (`source: "organization"`), or the built-in enterprise theme
   (`source: "fallback"`) when there is no organization or no theme.
5. The web app applies the tokens, renders the layout the theme names, and feeds
   it the theme's `nav`.
6. **Staying current.** The theme is cached per organization. Installing, upgrading,
   removing or switching a plugin (or activating a theme) clears that cache and sends
   the organization's open browsers the transient event `kernel` / `plugins.changed`
   (`plugins_changed` in `facets/core_api`). The web app reads the theme and the apps
   list again (`web/src/lib/apps/pluginsChanged.ts`), so a new theme revision shows
   without a page refresh. Reloading the catalog from disk alone changes nothing for an
   organization until it is upgraded to the new revision.

## Layouts

Built-in layouts live in `web/src/lib/components/layout/registry.ts`:

| Name | What it is |
|---|---|
| `default` | top navbar |
| `custom` | top navbar styled with the theme's colours |
| `desk` | sidebar workspace |
| `bare` | no application chrome |

A theme plugin may also ship its own layout under `layouts/<name>.json` and name
it with `"layout": "<name>"`. The simplest picks a first-party chrome:

```json
{ "name": "sea", "label": "Sea shell", "chrome": "sidebar" }
```

`chrome` is one of `top`, `sidebar`, `custom`, or `bare`. The definition is stored
on the theme and returned by `GET /api/ui/theme` as `layout_def`. An unknown
built-in name falls back to `default`.

### Composed layouts

A layout can instead build the whole shell from **regions** and the **blocks** in
them. The built-in `default` and `desk` layouts are written this way
(`web/src/lib/components/layout/composed/builtins.ts`); copy one to start.

```json
{
  "name": "sea", "label": "Sea",
  "regions": {
    "banner":  { "tone": "primary", "blocks": [
        { "type": "text", "props": { "text": "Year-end close: books lock on Friday", "muted": false } } ] },
    "topbar":  { "sticky": true, "blocks": [
        { "type": "home-menu" },
        { "type": "brand" },
        { "type": "nav" },
        { "type": "spacer" },
        { "type": "link", "props": { "label": "Help", "href": "https://help.example", "icon": "life-buoy" }, "hide_below": "md" },
        { "type": "notifications", "when": "user" },
        { "type": "user-menu" } ] },
    "sidebar": { "width": "15rem", "tone": "muted", "blocks": [ { "type": "app-menu" } ] },
    "footer":  { "blocks": [ { "type": "text", "props": { "text": "© Sea Ltd" } } ] }
  }
}
```

Regions are `banner`, `topbar`, `subbar` (a second row directly under the top bar, for tabs,
a subnav or breadcrumbs), `sidebar`, `aside` (a right-hand column) and
`footer`. Each takes `blocks` and optionally `tone` (`default`, `muted`, `primary`
or `inverted`; `default` uses the theme's `sidebar*` tokens), `width` (sidebar and
aside, a length such as `15rem`), `position` (`left` or `right`, sidebar) and
`sticky` (top bar). A region whose blocks would all draw nothing takes no room:
the sidebar of an app without menus disappears.

| Block | What it draws | Props |
|---|---|---|
| `home-menu` | the launcher button (apps, organization, management links) | |
| `brand` | the theme's logo and product name, linking home | `label`, `href`, `show_logo` |
| `app-title` | the current app's name, linking home | |
| `app-menu` | the current app's pages (or the desk sections) | `source`, `variant` (default `tree`), `expand` |
| `nav` | the theme's `nav` | `source`, `orientation`, `variant` (default `links`), `expand` |
| `breadcrumbs` | where the page is: the app, then the page's trail (see below) | `style`, `scope` |
| `search` | opens the search palette (see below) | `style`, `shortcut` |
| `back` | a back arrow (browser history, else the home page); hidden on the home page | `label` |
| `scope-switcher` | a plugin-defined scope (project, group, team): the current one and the others | `source` (`plugin.function`, required), `label` |
| `link` | one link | `label`, `href`, `icon` |
| `text` | a line of text | `text`, `muted` |
| `spacer` | pushes what follows to the far end | |
| `divider` | a rule between blocks | |
| `notifications` | the notification bell | |
| `company-switcher`, `org-switcher` | the working company and organization | |
| `color-mode` | a light/dark toggle | |
| `developer-badge` | a "Developer" tag | |
| `user-menu` | the account menu | |

Any block may also set `when` (`user`, `developer`, `manager` or `admin`), `roles`
(a list; any one shows the block) and `hide_below` (`sm`, `md`, `lg`, `xl`). The
`source` of the first `app-menu` or `nav` block picks the navigation the layout
reads: `theme` (default), `desk` or `manage`. A `nav` block with `source: "footer"` reads the
theme's `nav.footer` list instead (use `variant: "columns"`) and does not pick the layout's navigation. Unknown regions, blocks, props and
unsafe links fail the plugin load, so a theme cannot break at runtime.

### Layouts in XML

A layout can be `layouts/<name>.xml` instead of `.json`. The XML is turned into the same
structure and checked by the same rules; use whichever reads better (the shipped themes use XML).

```xml
<layout name="sea" label="Sea">
  <topbar sticky="true" tone="primary">
    <home-menu variant="launcher" trigger="grid"/>
    <app-title/>
    <app-menu variant="mega"/>
    <spacer/>
    <user-menu roles="org_admin, developer" hide_below="md"/>
  </topbar>
  <footer><text muted="true">© Sea Ltd</text></footer>
</layout>
```

A region is an element named after it (`banner topbar subbar sidebar aside footer`); its
attributes are the region's keys (`width`, `position`, `tone`, `sticky`) and its children are
the blocks. A block is an element named after its type. `id`, `when`, `hide_below` and `roles`
(comma separated) are the block's own keys; every other attribute is a prop; the text of a
`<text>` block is its `text`. `true` and `false` are booleans. A block cannot hold other blocks.

### Navigation written in the layout

A `<nav>` or `<app-menu>` block can hold its own links, written as `<link>` and `<group>`
elements. They replace what the block would read from the app, so a theme can lay out the
navigation itself:

```xml
<nav variant="mega">
  <link label="Home" href="/apps" icon="home"/>
  <group label="Sales" icon="tag">
    <link label="Deals" href="/crm/deals" badge="3"/>
    <link label="Tickets" href="/helpdesk" badge_function="helpdesk.open_tickets"/>
  </group>
</nav>
```

A `<link>` takes `label`, `href` (an app path or an http(s) URL), `icon`, `description` and a
`badge` (up to 12 characters of text, or `badge_function="plugin.function"` for a live count). A
`<group>` takes `label`, `icon` and `href` and holds links and groups. In the layout's JSON the
same list is the block's `items`. Without children the block reads the app's own navigation.

### Which layout draws the app

The app has two kinds of layout and keeps them apart. The built-in ones (`default`, `desk`,
`navbar`, `custom`, `bare`) are components of the app. When the theme reply from the server
carries a layout of the theme plugin, the app draws that instead with one component, which builds
the shell from the definition and from nothing else: there is no fall-back to the default
layout, so a layout with no `sidebar` has no sidebar, on the desk too (the desk keeps the
theme's layout and shows the desk's pages in its menus).

### Switching apps from the top bar

The built-in `navbar` layout (`"layout": "navbar"`) has no sidebar: the grid button opens a
full-width launcher (`<home-menu variant="launcher"/>`), then comes the open app's name and its
menus along the top bar, a group opening as a dropdown (`<app-menu variant="mega"/>`). Copy it
into a theme layout to change any part. `home-menu` also takes `variant="dropdown"` or `"drawer"`
and `trigger="hamburger"` or `"label"`.

`<brand follow="app"/>` shows the product name and logo on the home page and turns into the open
app's name (and its logo, when it has one) as soon as the person switches to an app. Without
`follow` the brand stays the product's. `<app-title/>` always shows the app's name.

### Sign-in screens

A theme plugin can replace the sign-in screens with `screens/<name>.xml`. `login` and
`register` can be replaced today; there is no password-reset flow yet, so there is nothing to
draw for it.

```xml
<screen name="login" layout="split">
  <aside tone="primary" side="left">
    <brand/>
    <heading level="2">Welcome back</heading>
    <image src="assets/hero.svg" alt=""/>
  </aside>
  <main>
    <heading level="1">Sign in</heading>
    <login-form/>
    <oauth-buttons/>
    <link href="/register" label="Create an account"/>
  </main>
</screen>
```

`layout` is `split` (the `<aside>` beside `<main>`, hidden on small screens), `center` or
`stack`. `<aside>` is optional and takes `tone` (`default`, `muted`, `primary`) and `side`.
Blocks: `brand`, `heading` (`level` 1 to 3), `text` (`muted`), `image` (`src` is a file under
`assets/` or an https URL), `link` (`href`, `label`), `divider`, `spacer`, and the forms, which
are the app's own and keep working as they do on the built-in screen: `login-form` and
`oauth-buttons` on `login`, `registration-notice` on `register`. Text is at most 400
characters, and any other element or attribute is refused when the plugin loads.

Screens: `login`, `register`, `forgot-password` and `reset-password` (the page a reset link
opens). Their own blocks: `login-form` and `oauth-buttons` on `login`; `registration-notice` on
`register`; `forgot-password-notice` on `forgot-password`; `reset-password-form` on
`reset-password`. Password reset is by an administrator sending a link; the app has no mail
sender of its own, so there is no "email me a link" form.

The screen follows the theme the server picks for the visitor: the organization the address
names, or the signed-in person's. When neither is known and the instance has exactly one
organization, that organization's theme applies. With several organizations and none named, the
built-in screens are shown.

### Shell regions and blocks for working in the app

More of the shell is the layout's to place.

- **Regions**: `bottombar` (a bar along the bottom, with the safe-area inset) joins `banner`,
  `topbar`, `subbar`, `sidebar`, `aside` and `footer`. Any region takes `show_below="md"` (shown
  only on screens narrower than that, a phone's tab bar), and any block takes `show_below` as
  well as `hide_below`.
- **Sidebar modes**: `mode="full"` (default), `rail` (a narrow strip, each link its icon with a
  short label under it) or `collapsible` (the person folds it; with `collapsed_width` it folds to
  the rail, without it away). `overlay="true"` opens the sidebar over the page below `md`. The
  `sidebar-toggle` block is the button.
- **`skip-link`**: a link hidden until the first Tab that moves focus to the page's content
  (`target`, default `content`). The built-in layouts place it first; put it first in yours.
- **`create`**: a "New" menu (`label`, `style="button"` or `"fab"`) listing the `[[app.palette]]`
  entries that have `create = true` and an `href`.
- **`save-bar`** and **`action-bar`**: show while the open form has unsaved changes (Discard,
  Save) or rows are selected in the open list (count, Clear).
- **`workspace-tabs`** (`limit`, 2 to 20, default 8): the pages the person has opened, one tab
  each, with a close button.
- **`pinned`**: the person's pinned pages, and a star to pin or unpin the open one.

The folded sidebar, pinned pages, open tabs and the detail pane's width are the person's own
choices. They are kept in this browser under the person's id, so they follow the person on this
device but not to another.

### Detail pane beside a list

`<view type="list" open="/notes/{id}" pane="true"/>` opens the row's page next to the list
instead of replacing it. The bar between them is dragged (or moved with the arrow keys) to
resize. The pane is the page itself shown without the shell (any page opens that way with
`?embed=1`); "Open" in its bar goes to the page on its own. Parts: `data-part="list.pane"` (the
list column) and `data-part="form.pane"` (the detail).

### Seeding the colours

`aether --theme-palette "#6750a4"` prints `light` and `dark` colour tokens made from one seed
colour (in OKLCH, so equal lightness looks equal at every hue), ready to paste into a
`theme.json` and adjust. `--palette-scheme` is `tonal` (default, tinted surfaces), `neutral`,
`vivid` or `mono`. Pairs with weak contrast are reported on stderr.

### Fonts

A theme ships font files under `assets/fonts/` and loads them with `@font-face` in its
stylesheet (see "A stylesheet"). The shipped themes include Roboto, Roboto Mono, Inter, Geist,
Geist Mono and Source Sans 3, which are open-licensed; fonts a design system owns and does not
license for redistribution stay on system stacks.

### Breadcrumbs

The `breadcrumbs` block draws the current app, then where the page is in it. A page that
gives a trail of its own (its control panel's crumbs: Organizations › Acme › Roles) shows
that. A page that gives only its title gets the app menu groups above it (HR › People ›
Employees › Ada Lovelace), and a page that gives nothing gets its place in the app menu. The
last crumb is the current page and is not a link.

* `style`: `chevron` (default), `slash` (`Team / App / Page`) or `arrows` (segments that point
  at the next one).
* `scope`: `true` puts the organization first.

While the block is on screen, the page's control panel shows only the page title, so the
trail is not drawn twice. The parts are `data-part="breadcrumbs"` (with `data-style`) and
`breadcrumbs.item`.

### Search

Ctrl/⌘+K opens the search palette in every layout: commands plugins add with
`[[app.palette]]` (see "Palette entries" in [Organizations](organizations.md)), apps, their
pages and the management screens, all filtered by role. The `search` block puts it on screen:

* `style`: `field` (default; a box that lists matches as you type), `button` (a "Search…"
  button with its shortcut) or `icon` (a magnifier).
* `shortcut`: `k` (default; only Ctrl/⌘+K) or `/`, which also focuses the field or opens the
  palette when you are not typing in a field.

The parts are `search.field` and `search.button`. Records come from the plugins' `search`
functions (see "Palette entries"); the `field` style lists screens only.

### Scope switcher

`scope-switcher` calls its `source` function (no arguments) and shows the scope it names as
current, with the others to switch to. The function answers `[ { "id", "label", "href",
"current"? } ]` or `{ "current": "<id>", "items": [...] }`; items without a label or with an
unsafe `href` are dropped, at most 50 are kept. It is asked when the block appears and after
each navigation, through the ordinary plugin call API. The organization crumb of a
`breadcrumbs` block with `scope` opens the organization switcher.

### Navigation variants

`variant` on `nav` and `app-menu` chooses how the items are drawn:

| Variant | In a bar (topbar, banner, footer) | In a column (sidebar, aside) |
|---|---|---|
| `links` | links in a row; a group is a dropdown | links, each group under a heading |
| `tree` | drawn as `links` | groups expand in place, nested up to three levels |
| `tabs` | underlined tabs; those that do not fit go into a "More" menu; a group is a dropdown tab | tabs with a bar on the leading edge, groups under headings |
| `mega` | a group opens a wide panel: nested groups become columns, an item's `description` shows under its label | the panel opens to the side |
| `columns` | for a footer: each group is a column under its heading | the same |
| `tiles` | a launchpad: each group is a section of tiles with icon, label, description and badge | the same |

`overflow="auto"` (on `app-menu` and `nav`, for `mega` in a bar) keeps as many top-level entries as fit the free width and folds only
the ones that would run into the next element; `overflow="6"` (2 to 20) keeps that many whatever the width. The rest go into one panel, labelled by `more_label` (default `More`), so a long menu never runs into the search field.

`expand` applies to `tree`: `toggle` (default; the user opens and closes groups, the one
holding the current page starts open), `active` (one group open at a time, the current one
first) or `all` (every group open, with headings instead of toggles). Theme `nav` items carry
`description` themselves; app menu entries take it from the page's `menu_description`.

Only one link is current: the longest href that covers the page, so "Employees" is not
marked beside "Employees › New". Every variant draws item icons and badges. The rendered parts are `data-part="nav"` (with
`data-variant`), `nav.badge`, `nav.link` (`aria-current="page"` when current), `nav.group` (`data-active`
when it holds the current page), `nav.heading`, `nav.more` and `nav.panel`, for a theme
stylesheet to target.

Logged-in users get the theme's layout. Visitors on public pages get `bare`.

## A page can choose its own layout

```xml
<page route="/landing" layout="bare" title="Landing"> … </page>
```

`layout` on a `<page>` overrides the theme's for that page. The name is validated
when the plugin is loaded. The page's data (including its layout) is fetched in a
SvelteKit `load` function, so the right layout is chosen before anything renders.

## Error pages

Errors are themeable too. The web app keeps a registry of **error-page sets** in
`web/src/lib/components/errors/registry.ts`; a theme chooses one with
`"error_pages": "custom"`. A set has one component per kind of error plus an
inline card:

| Part | Shown for |
|---|---|
| `notFound` | 404 |
| `forbidden` | 401, 403 |
| `tooManyRequests` | 429 |
| `serverError` | 5xx and anything unexpected |
| `card` | a failure inside a page, such as a widget that cannot render |

Two built-in sets ship: `default` (a neutral card) and `custom` (a bold variant
drawn in the theme's colours). A theme plugin may also ship `errors/<name>.json`
and name it with `"error_pages": "<name>"`:

```json
{
  "name": "sea",
  "variant": "custom",
  "not_found": { "title": "Lost at sea", "message": "That page drifted away." }
}
```

`variant` chooses the built-in look (`default` or `custom`); optional
`not_found`, `forbidden`, `too_many_requests`, `server_error`, and `card` objects
override the shared copy (`errors/copy.ts`). The definition is returned as
`error_pages_def` on the theme API. An unknown built-in name falls back to
`default`.

Error pages render inside the themed layout, so the navigation stays available.
`ErrorPage.svelte` shows a page for a status and `ErrorCard.svelte` an inline
card; both read the active theme. SvelteKit's own `+error.svelte` (for the app
and for the root) uses them, as does the plugin page route when the page API
answers 403, 404, 429 or an error.

## Errors outside the web app

Requests that never reach the app get a plain default: JSON for `/api/...` and for
clients that do not ask for HTML, and a small self-contained page for browsers.
`/web` before the app is built explains how to build it (`pnpm build` in `web/`)
instead of answering with an empty body. Unknown paths under `/web` (including
`/web/`) always load the app, which then shows its own 404.

## Choosing a theme

Developers can switch themes without the command line: **Settings → Appearance**
shows the built-in default and every installed theme with a preview drawn in its
own colours. "Use this" calls `POST /api/ui/themes/active` with `{ "name": "ocean" }`
(or `{ "name": null }` for the built-in default); it is developer-only.

## Limits

What a theme cannot express yet, and the plan to close each gap, is in [Theme breaking points](theme_gaps.md).
