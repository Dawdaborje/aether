# Pages and data

A page is XML. The widgets on it get their data by calling **functions of the plugin the page
belongs to**, as the person looking at the page (so the kernel's rules for logged-in members
and anonymous visitors apply as usual: a public page can only use functions listed in
`public_functions`).

To reuse the same markup on several pages, define a fragment under `components/` and pull it
in with `<use>`; see [UI components](components.md). To wrap a whole page body in a shared
shell (SvelteKit-style children), use `wrap` on `<page>`.

## Shell `layout` vs content `wrap`

| Attribute | What it does |
|---|---|
| `layout="bare"` | App chrome (navbar / sidebar / none). Theme default if omitted. |
| `wrap="notes_shell"` | Content layout: a **component** whose default `<slot />` receives this page's body. |

```xml
<!-- components/notes_shell.xml -->
<component name="notes_shell">
  <group title="{{title}}">
    <slot />
    <slot name="aside" />
  </group>
</component>
```

```xml
<page route="/notes/board" title="Notes board" wrap="notes_shell">
  <view type="kanban" source="list_notes" group_by="status">…</view>
  <set slot="aside">
    <stat label="Tip" value="Drag cards between columns" />
  </set>
</page>
```

The page title is passed as the `title` prop (`{{title}}`). Named slots use `<set slot="…">`
as direct children of `<page>`. Foreign wraps use `wrap="other.shell"` and need `dependencies`.

```xml
<page route="/notes" title="Notes" model="note">
  <view type="list" model="note" source="list_notes" open="/notes/{id}"
        create="/notes/new" create_label="New">
    <search/>
    <columns>
      <column field="title" label="Title"/>
      <column field="body" label="Body"/>
    </columns>
    <actions>
      <action name="remove" label="Delete" function="remove_note"
              confirm="Delete this note?" danger="true"/>
    </actions>
    <empty title="No notes yet" description="Use New to add one."/>
  </view>
</page>

<page route="/notes/new" title="New note" model="note">
  <view type="form" function="add_note" submit="Add note" success="Note added"
        redirect="/notes">
    <field name="title" label="Title" required="true"/>
    <field name="body" label="Body" fieldType="text" required="true"/>
  </view>
</page>
```

The list homepage uses an Odoo-style control panel: **New** under the breadcrumbs
(from `create`), **search** on the right (from `<search/>`). The create form lives on
its own page and `redirect`s back after save.

## List: `<view type="list">`

| Attribute / child | Meaning |
|---|---|
| `source` | Function that returns the rows: an array, or an object holding one under `rows`, `items` or `data`. Called with the page's route values (`{ "id": "abc" }` for `/notes/{id}`). |
| `page_size` | When set (e.g. `50`), the list pages on the server: the call also gets `limit`, `offset`, and when searching `q`, and optional `order`. Prefer answering `{ "rows": […], "total": N }`. Without `page_size`, search stays client-side on the loaded rows. |
| `default_order` | Initial `order` when paging (`title` or `-updated_at`). |
| `<columns><column field label fieldType sortable/>` | The columns. Without them, the fields of the first row. `fieldType`: `badge`, `boolean`, `date`, `datetime`, `currency`. With `page_size`, `sortable="true"` toggles `order` on that field. |
| `open="/notes/{id}"` | Clicking a row goes here. `{field}` is filled from the row; `{id}` is the record's key. |
| `create="/notes/new"` | Control-panel **New** button opens this route. |
| `create_label` | New button text (default `New`). |
| `<search/>` | Enables the control-panel search box (right side). Without `page_size` it filters the loaded rows; with `page_size` it sends `q` to `source` (debounced). |
| `<actions><action function label confirm danger/>` | A button per row that calls `function` with `{ "id": <the row's key> }`. With `confirm`, it asks first. The list reloads afterwards. |
| `<empty title description/>` | Shown when there are no rows. |

Server paging call shape (merged with route params):

```json
{ "q": "needle", "order": "-updated_at", "limit": 50, "offset": 0 }
```

Preferred answer when paging:

```json
{ "rows": [ { "id": "…", "title": "…" } ], "total": 1234 }
```

A bare array still works (page 1, unknown total; next page is offered while a full page of rows comes back).

## Form: `<view type="form">`

| Attribute | Meaning |
|---|---|
| `function` | Called on Save with the field values and the page's route values. |
| `source` | Function that returns the record to edit; its fields fill the form. |
| `submit` | The button's text (default "Save"). |
| `success` | The message shown after saving (default "Saved."). |
| `redirect` | Where to go after saving; `{field}` is filled in. |
| `clear` | Whether the form empties after saving. By default a form without `source` does. |

`<field name label fieldType required options/>` inside a form edits that field: `fieldType` is
`char` (default), `text`, `integer`, `float`, `currency`, `boolean`, `date`, `datetime` or
`selection` (with `options="a, b, c"`). Fields can be grouped with `<group>` and `<notebook>`.

An error the function reports with the SDK's `Error::msg(...)` is shown under the form.
Saving or deleting anything makes every list on the page load again.

## Kanban: `<view type="kanban">`

Rows from `source` are grouped into columns by `group_by`. Cards are declared with `<card>`
(or Odoo-style `<templates><card/><menu/></templates>`).

```xml
<view type="kanban" model="note" source="list_notes"
      group_by="status" open="/notes/{id}"
      move="move_note" create="quick_create_note">
  <progressbar field="pinned" colors='{"true":"success","false":"200"}'/>
  <card>
    <field name="title"/>
    <field name="author"/>
    <footer>
      <field name="status" fieldType="badge"/>
    </footer>
  </card>
  <actions>
    <action name="remove" label="Delete" function="remove_note"
            confirm="Delete this note?" danger="true"/>
  </actions>
  <empty title="No notes" description="Create one to get started."/>
</view>
```

| Attribute / child | Meaning |
|---|---|
| `source` | Same row contract as list. |
| `group_by` | Field whose value becomes the column key. Blank → `Unassigned`. |
| `open` | Card click route; `{field}` / `{id}` interpolated like list. |
| `move` | On drag between columns, called with `{ id, group_by_field, from, to }` then the page refreshes. |
| `create` | Quick-create in a column: called with `{ title, [group_by]: columnKey }`. |
| `<progressbar field colors sum_field/>` | Segment bar per column from `field` values; `colors` is a JSON map (`success`, `danger`, `warning`, `info`, or a muted token). Optional `sum_field` shows a column total. Click a segment to filter cards. |
| `<card>` | Fields shown on each card; optional `<footer>` for badges/avatars. First field is the title. |
| `<templates>` | Optional wrapper: `<card>` and `<menu>` inside are flattened to `<card>` + `<actions>`. |
| `<actions>` / `<menu>` | Overflow menu on each card (same action contract as list). |
| `<empty/>` | Shown when there are no rows. |

Without `source`, a kanban still accepts static `<group title="…">` children (layout-only).

## Dashboard: `<view type="dashboard">`

A responsive widget grid, optionally fed by one shared `source` object. Child `<stat>` /
`<chart>` widgets may also call their own `source`.

```xml
<view type="dashboard" source="notes_dashboard" cols="3">
  <stat label="Draft" field="draft_count"/>
  <stat label="Shared" field="shared_count"/>
  <stat label="Total" field="total"/>
  <chart title="By status" type="bar" field="by_status" category="label" value="value"/>
</view>
```

Board-style layout (Odoo `board` inspired) uses `layout` weights and titled columns that embed
other views:

```xml
<view type="dashboard" layout="2-1">
  <column title="Board">
    <view type="kanban" source="list_notes" group_by="status" open="/notes/{id}" move="move_note"/>
  </column>
  <column title="Stats">
    <stat label="Total" source="notes_dashboard" field="total"/>
    <view type="list" source="list_notes" open="/notes/{id}">
      <columns>
        <column field="title" label="Title"/>
        <column field="status" label="Status" fieldType="badge"/>
      </columns>
    </view>
  </column>
</view>
```

| Attribute / child | Meaning |
|---|---|
| `source` | Function returning one object; `<stat field="x"/>` reads `data.x`. |
| `cols` | Grid columns when not using board columns (`2`, `3`, or `4`). |
| `layout` | Board weights like `2-1` → CSS `2fr 1fr`. |
| `<column title>` | Board column; children are nested views/widgets. |
| `<stat label field source value/>` | KPI card. Uses dashboard `source` + `field`, or its own `source`, or a static `value`. |
| `<chart title type source field category value/>` | `type` is `bar` (default), `line`, or `pie`. Data from `source` rows, or dashboard `field` holding a row list. `category` / `value` name the series fields (defaults `category` / `value`). |

## Calendar: `<view type="calendar">`

A month grid. `source` is called with the visible window (`start`, `end` as ISO datetimes)
and `scale` (`month`).

```xml
<view type="calendar" model="event" source="list_events"
      date_start="starts_at" date_stop="ends_at"
      color="status" open="/events/{id}">
  <field name="title"/>
  <empty title="No events" description="Nothing in this month."/>
</view>
```

| Attribute / child | Meaning |
|---|---|
| `source` | Function returning events in the window (same row unwrap as list). |
| `date_start` | Field for the start (default `starts_at`). |
| `date_stop` | Optional end field; omit for point events. |
| `color` | Optional field used to tint the event chip. |
| `open` | Click route; `{field}` / `{id}` like list. |
| `<field name>` | First field is the title shown on the chip (default `title`). |

Call payload (plus route params): `{ "start", "end", "scale": "month" }`.

## Widget status

`list` (with optional server `page_size`), `form`, `kanban`, `dashboard`, `calendar`, `group`,
`notebook`, `stat`, `chart` and chatter work. `tree` and `pivot` currently render as a plain
list, and `statusbar` and `buttonbox` are generic header and action containers with no state
behaviour. `gantt`, `graph`, `map` and report views do not exist yet.
