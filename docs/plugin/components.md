# UI components

A component is a reusable XML fragment. Define it under `components/`, then pull it into
pages (or other components) with `<use>`. There is no component table in `plugin.toml`;
discovery is filesystem-only, the same way as [pages](pages.md).

Components are registered when the plugin is loaded (`aether --load-plugins`). When a page
is served, the kernel expands every `<use>` against the components of plugins **installed
and enabled** in that organization, then localizes `%key%` text as usual. The browser only
sees built-in widget tags.

```xml
<!-- components/note_card.xml -->
<component name="note_card">
  <group>
    <field name="title" label="{{title}}" />
    <slot />
    <slot name="actions" />
  </group>
</component>
```

```xml
<!-- pages/notes.xml -->
<page route="/notes" title="Notes" model="note">
  <use component="note_card" title="Hello">
    <field name="body" label="Body" fieldType="text" />
    <set slot="actions">
      <action name="remove" label="Delete" function="remove_note" />
    </set>
  </use>
</page>
```

## Names

| Form | Where |
|---|---|
| `note_card` | Local short name: lowercase letters, digits and `_`, starting with a letter. Unique per plugin. |
| `notes.note_card` | Full name. Other plugins must use this form and list `notes` under `dependencies` in `plugin.toml`. |

## Props

Attributes on `<use>` (except `component`) become props. Inside the component body, `{{name}}`
in any string is replaced with that prop. This is separate from i18n `%key%` markers, which
are resolved after expansion.

## Slots

| Markup | Meaning |
|---|---|
| `<slot />` | Default slot: filled with the children of `<use>` that are not `<set>`. |
| `<slot name="actions" />` | Named slot: filled by `<set slot="actions">…</set>` under `<use>`. |

An empty or missing slot contributes nothing.

## Page wraps (content layouts)

A component with a default `<slot />` can wrap an entire page. Set `wrap="component_name"`
on `<page>` (or `wrap="plugin.component"`). When the page is served, the kernel treats the
page body as that component's default slot and passes the page `title` as a prop — like
SvelteKit `{@render children()}`. This is separate from shell chrome `layout="bare"`.

See [Pages and data](pages.md#shell-layout-vs-content-wrap).

## Nesting and cycles

A component may `<use>` another. Nesting deeper than 16 levels, or a cycle (`a` → `b` → `a`),
is refused when the page is built.

## Reserved tags

These are not widgets; they never reach the frontend after expansion:

`component`, `use`, `slot`, `set`

## Cross-plugin use

```xml
<use component="notes.note_card" title="From CRM" />
```

Requires `dependencies = ["notes"]`. Loading a plugin that references a missing foreign
component fails until that dependency is loaded. Upgrading a plugin that removes a component
another installed plugin still uses is refused.
