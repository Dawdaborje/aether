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
      { "label": "Apps", "href": "/apps" },
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
| `layout` | which layout component renders the app (default `default`) |
| `error_pages` | which set of error pages the app shows (default `default`) |
| `nav` | the navigation for that layout; omit it to use the layout's built-in one |

`nav` is `{ header?, items }`; an item is `{ label, href?, icon?, children? }`
and needs an `href`, `children`, or both. Hrefs are app paths (`/apps`) or
`http(s)` URLs: `javascript:` and `//host` are refused. At most 50 items, nested
at most 3 deep.

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

## From plugin to screen

1. `aether --load-plugin <dir>` validates the theme (a bad layout name, nav or
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

The web app keeps a registry of layouts in
`web/src/lib/components/layout/registry.ts`:

| Name | What it is |
|---|---|
| `default` | top navbar |
| `custom` | top navbar styled with the theme's colours |
| `desk` | sidebar workspace |
| `bare` | no application chrome |

An unknown name falls back to `default` (with a console warning). To add a
layout, create the component and list it in the registry; any theme can then
name it.

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

Two sets ship: `default` (a neutral card) and `custom` (a bold variant drawn in
the theme's colours). The words for each status are shared
(`errors/copy.ts`), so a set only decides how things look. To add a set, create
its components and list it in the registry; any theme can then name it. An
unknown name falls back to `default`.

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
