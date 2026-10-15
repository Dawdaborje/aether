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
| `density` | `compact`, `comfortable` (default) or `spacious`: the spacing scale |
| `layout` | which layout renders the app (default `default`); a built-in name or one of the plugin's `layouts/` |
| `error_pages` | which set of error pages the app shows (default `default`) |
| `nav` | the navigation for that layout; omit it to use the layout's built-in one |
| `home` | `{ "route": "/sales/dashboard" }`: where members land after login and where the brand links lead (default the app launcher, `/apps`) |
| `branding` | `{ "product_name", "logo", "favicon" }`: shown by the `brand` block and the browser tab |

Colour tokens are written into the app's stylesheet as `--name: value`, so names
are lowercase words joined by `-` and values cannot contain `;`, `{`, `}`, `<`, `>`
or `\`. The same holds for the fonts and `radius`. Logos and favicons are files
under the plugin's `assets/` folder (`"assets/brand/logo.svg"`, served from
`/api/ui/plugins/<plugin>/assets/brand/logo.svg`) or `https://` URLs.

`nav` is `{ header?, items }`; an item is `{ label, href?, icon?, children? }`
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

Regions are `banner`, `topbar`, `sidebar`, `aside` (a right-hand column) and
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
| `app-menu` | the current app's pages (or the desk sections) | `source` |
| `nav` | the theme's `nav`: links, groups as dropdowns in a bar | `source`, `orientation` |
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
reads: `theme` (default), `desk` or `manage`. Unknown regions, blocks, props and
unsafe links fail the plugin load, so a theme cannot break at runtime.

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
