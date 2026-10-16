---
title: "Pages and data"
description: "A page is XML. The widgets on it get their data by calling functions of the plugin the page belongs to, as the person looking at the page (so the kernel's rules for logged-in members and anonymous…"
section: plugin
status: stable
order: 60
---
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
| `menu="false"` | Hide from the app sidebar (create/hire forms; open them from list **New**). |
| `menu_group="Employee"` | Collapsible sidebar section (collapsible). |
| `icon` / `sort` | Lucide icon id drawn on the sidebar link, and order among menu entries. Lowercase letters, digits and `-` (for example `calendar-days`). |
| `roles="hr_user,hr_manager"` | Who may open the page and see it in menus (OR). Short names become `<plugin>.name`. Omit = any member. `org_admin` always passes. |

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

The list homepage uses a control panel: **New** under the breadcrumbs
(from `create`), **faceted search** on the right (from `<search/>` / `<filter/>` /
`<groupby/>`). Chips sync to the URL (`f`, `g`). The create form lives on its own page and
`redirect`s back after save.

When another tab (or user) changes rows through `db::create` / `update` / `delete`, the kernel
publishes `aether` / `data.changed` on the notification stream. Open pages whose `model`
matches reload their list, kanban, calendar, and form views (forms keep unsaved edits).
See [Events](../architecture/events.md).

## List: `<view type="list">`

| Attribute / child | Meaning |
|---|---|
| `source` | Function that returns the rows: an array, or an object holding one under `rows`, `items` or `data`. Called with the page's route values (`{ "id": "abc" }` for `/notes/{id}`). |
| `page_size` | When set (e.g. `50`), the list pages on the server: the call also gets `limit`, `offset`, `filter`, and when searching `q` / `search_fields` (alias `text` / `fields`), plus optional `order` / `group_by`. Prefer answering `{ "rows": […], "total": N }` (or use `db::search`). Without `page_size`, free-text still filters loaded rows client-side unless `<filter/>` / `<search/>` forces server mode; then the default page size comes from the `ui.list.page_size` setting (global default **10**, org override allowed) and the table offers a rows-per-page control. |
| `default_order` | Initial `order` when paging (`title` or `-updated_at`). |
| `<columns><column field label fieldType sortable/>` | The columns. Without them, the fields of the first row. `fieldType`: `badge`, `boolean`, `date`, `datetime`, `currency`. With `page_size`, `sortable="true"` toggles `order` on that field. |
| `open="/notes/{id}"` | Clicking a row goes here. `{field}` is filled from the row; `{id}` is the record's key. |
| `create="/notes/new"` | Control-panel **New** button opens this route. |
| `create_label` | New button text (default `New`). |
| `<search fields="…"/>` | Enables the faceted search bar. `fields` is a comma list of **text** model fields; free-text (`q`) becomes an OR of `{ field: { like: q } }` via `db::search` (the kernel does **not** guess fields — the page/plugin must name them). The placeholder shows those field labels. When omitted, the first few non-boolean columns are used. |
| `<filter field options label/>` | A facet in the **Filters** menu (status, kind, active, …). `options` is a comma list: plain tokens (`draft,posted`) or `Label=value` (`Active=true,Inactive=false`). Choosing one adds an equality chip on that field inside `filter`. |
| `<groupby field label/>` | A Group By option (`group_by` on the payload). Alias: `<group_by/>`. |
| `<actions><action function label confirm danger/>` | A button per row that calls `function` with `{ "id": <the row's key> }`. With `confirm`, it asks first. The list reloads afterwards. |
| `<empty title description/>` | Shown when there are no rows. |

### How a linked record is named

A model names its records with `title_field` in `models/<name>.json` (like Django's `__str__`). Wherever a page shows a link field (a list column, a group heading), the kernel asks
`POST /api/ui/records/titles` for the titles of the linked records and shows those instead of ids;
no `source` function is needed for it. Link pickers work the same way: a link field whose model
names the target needs no `source`, and the kernel lists that model's records by title
(`POST /api/ui/records/options`), searching as you type. Kanban cards and column headings show names
too. The titles come only from models the person may read without a record-level condition in the
plugin's `rules/`; otherwise the id stays.

When one field is not enough, a model can set `title_template` in `models/<name>.json`:

```json
{ "name": "contact", "title_field": "last_name", "title_template": "{first_name} {last_name}", … }
```

Each `{name}` is a field of the model; a blank one drops out. `title_field` is still used for
searching the options.

### Grouping

The Group by menu (and `?g=department,status` in the address) puts the rows under headings, one
level per field, in the order chosen. Each heading shows the field, the value and how many rows it
holds, and can be folded. While a list is grouped it loads up to 500 rows at once, with no pager,
so each group is whole; the function still receives `group_by` and may ignore it.

A heading shows a readable value when the column is a selection (its option label), when the field
is a link (the kernel shows the linked record by its model's `title_field`, see below), when the row
carries `<field>_name` or `<field>_label`, or when the `<groupby>` names where to look the value up:

```xml
<groupby field="department" label="Department" source="list_departments" optionLabel="name"/>
```

Server paging / search call shape (merged with route params):

```json
{
  "q": "needle",
  "search_fields": ["display_name", "employee_no"],
  "filter": { "status": "active" },
  "group_by": ["department"],
  "status": "active",
  "order": "-updated_at",
  "limit": 50,
  "offset": 0
}
```

Prefer `filter` (kernel filter JSON) and `db::search` / `db::find` + `db::count`. Treat `q` as
free-text (alias `text` is fine). Flat keys from equality chips remain for backward compatibility.
When `model` is set, users can save the current chips as favorites via `/api/ui/search/favorites`.

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

`<field name label fieldType required options help/>` inside a form edits that field: `fieldType` is
`char` (default), `text`, `integer`, `float`, `currency`, `boolean`, `date`, `datetime`,
`selection` (with `options="a, b, c"`), `image` / `binary` (file picker that stores a resized
data URL string on the form), or `many2one` / `link` (searchable picker). For links, set
`source` to a plugin function that returns rows (`list_departments`, or `{ rows, total }`);
optional `optionLabel` picks the label field (default `display_name` / `name`), and `entity`
is the plural noun in empty/search hints. Fields can be grouped with `<group>` and `<notebook>`.

```xml
<view type="form" function="hire_employee" submit="Create employee">
  <group title="Personal details" cols="2">
    <field name="first_name" label="First name" required="true"/>
    <field name="last_name" label="Last name"/>
    <field name="notes" label="Notes" fieldType="text" colspan="2"/>
  </group>
  <group title="Photo" cols="1">
    <field name="image" label="Photo" fieldType="image"/>
  </group>
  <notebook>
    <page title="More">
      <group title="Extra" cols="2">…</group>
    </page>
  </notebook>
</view>
```

| Child / attribute | Meaning |
|---|---|
| `<group title cols>` | A titled section. `cols` is `1`, `2` (default), or `3`. |
| `colspan` / `span` on a child | `2`, `3`, or `full` to span the section width. |
| `<notebook><page title>` | Tabs; each `<page>` is one tab body. |

### The form's top bar: Save, buttons and state

Save, the form's own buttons and its state tracker sit on the right of the page's title bar (the
form's buttons, then Save, then the state steps), so they stay in view while the form scrolls. A form with no
title bar to use, such as one in a pop-up, keeps them in a sticky bar at the top of the form. Put
them at the top level of the `<view type="form">`:

```xml
<view type="form" source="get_employee" function="update_employee">
  <header>
    <button label="Confirm" function="confirm_employee" confirm="Confirm this employee?"/>
  </header>
  <statusbar field="status" options="active, probation, on_leave, resigned"
             function="change_employee_status"/>
  …
</view>
```

| Element | Meaning |
|---|---|
| `<header>` | Its children (`<button>`, `<action>`, `<buttonbox>`) go in the bar, next to Save. A bare `<button>`/`<action>` at the top level of the form does the same. A button's `href` may hold the page's route values: `href="/crm/deals/{id}/edit"`. |
| `<statusbar field function>` | The record's state as steps, the current one highlighted, on the right of the bar. The steps come from `options="a, b, c"`, else from `source` (a function returning rows; `optionLabel` names the label field, `sourceparam` a record field whose value is sent as `id`), else from the model field's own selection options, so `<statusbar field="status"/>` is usually enough. With `function`, clicking a step calls it with the route values, the record's `id`, and the chosen step as `status` and as `<field>`; without it the bar only shows the state. |

A `<buttonbox>`, `<header>` or `<statusbar>` written *beside* the form view on a page with one form is moved into the form's bar for you.

### Unsaved changes

The kernel tracks edits on every form; plugins need to do nothing.

- A `*` appears after the page name while the form holds changes that are not saved.
- Leaving the page (a link, the menu, the browser's back button) asks what to do: **Save**, **Keep a
  draft** (stored on this device, in `localStorage`), **Discard**, or **Keep editing**. Closing the
  tab keeps a draft and lets the browser ask.
- Opening the form again offers to **Restore** or **Discard** a draft kept earlier. A draft is
  removed when the form is saved.

#### Reusing the bar: `base_ui`

The `base_ui` plugin of the base workspace ships the bar as components, so forms across plugins
look and behave the same without repeating attributes. List `base_ui` under `dependencies`:

```xml
<!-- just the state steps -->
<use component="base_ui.status_bar" field="status" function="change_employee_status"/>

<!-- buttons and state together; the content is the buttons -->
<use component="base_ui.form_bar" field="stage" source="crm.list_stages" sourceparam="pipeline"
     optionlabel="name" function="set_stage">
  <button label="Mark won" function="win_deal" confirm="Mark this deal as won?"/>
</use>
```

Props: `field`, `options`, `source`, `optionlabel`, `sourceparam`, `function`, `payload` (as for
`<statusbar>`; all optional) and, for `form_bar`, `save_label`. Put the `<use>` at the top of the
form view, or beside it on the page.

`form_bar` includes the form's Save button as a `<save label="…"/>` widget, which saves the form it
is in; a bar with a `<save/>` replaces the built-in Save, so it can be moved or relabelled. Use
`payload="record"` on a state bar whose function saves the whole record (a notes form's
`update_note`), so a click sends every field with the new state.

An error the function reports with the SDK's `Error::msg(...)` is shown under the form.
Saving or deleting anything makes every list on the page load again.

### Adding a tab to another plugin's page: `extends`

A plugin can contribute a tab to a page it does not own. Give the page `extends` (the route of
the page to extend, written as that page declares it) instead of `route`:

```xml
<!-- hr_documents/pages/employee_documents.xml -->
<page extends="/hr/employees/{id}" title="Documents" params="employee={id}" sort="10">
  <use component="documents_panel"/>
</page>
```

| Attribute | Meaning |
|---|---|
| `extends` | Route of the page that receives the tab; several may be given, comma separated (`/hr/employees/{id}, /hr/me`). The page is not served or listed in menus itself. |
| `title` | The tab's label. |
| `params` | Renames route values for this plugin's functions: `employee={id}` sends the host's `id` as `employee` (`name=literal` sends a fixed value). A value the host page does not have is left out, so on `/hr/me` the plugin sees no `employee` and uses the caller. The host's own values are still sent. |
| `sort` | Tab order (lower first, then by plugin name). |
| `roles` | Who sees the tab (OR). Short names are qualified with the contributing plugin. |

When the target page is served, every enabled plugin's contributions for it are appended to its
first `<notebook>` (one is added to the first view if there is none). Put the body in a [component](components.md) (as above) so the same panel can be reused on other
pages; short component names resolve against the contributing plugin. The tab's functions run
against the **contributing** plugin, not the page's. If the contributing plugin is not installed
or enabled, the tab is simply absent. Text (`%key%`) and field types in a tab come from the contributing plugin, and several plugins may
contribute a tab with the same title. Declare the target's plugin in `dependencies` so it is
installed first.

### Changing another plugin's page: `<xpath>`

A page that `extends` another and holds `<xpath>` elements is a **patch**: instead of adding a
tab, it edits the page it extends. Use it to add a column, rename a label, or drop a filter
without copying the page.

```xml
<page extends="/hr/employees" title="Badge column">
  <xpath expr="//column[@field='status']" position="after">
    <column field="badge_no" label="Badge"/>
  </xpath>
  <xpath expr="//column[@field='hire_date']" position="attributes" set_label="Joined"/>
  <xpath expr="//filter[@field='status']" position="remove"/>
</page>
```

| `position` | What happens to the first element `expr` finds |
|---|---|
| `after`, `before` | the content is added next to it |
| `inside` | the content is added at the end of it |
| `replace` | it is replaced by the content |
| `remove` | it is removed |
| `attributes` | each `set_<name>="value"` sets that attribute; an empty value removes it |

`expr` is a small subset of XPath: steps joined by `/` (a child) or `//` (any descendant);
each step is an element name or `*`, with `[@name='value']` conditions and an optional
`[n]` (the n-th match). `@type` on a `view` is its view type (`list`, `form`). The element
names are the ones in this document (`view`, `column`, `field`, `group`, `filter`, …).

* Patches are checked when the plugin loads: a bad `expr`, an unknown `position` or a
  position that needs content and has none refuses the plugin.
* A patch that finds nothing at serve time is skipped and logged; it never breaks the page.
* A page's `roles` limit who sees the patch, as for tabs. Patches are applied before tabs,
  in `sort` then plugin-name order.
* Added fields are drawn on the host page, so their `source` and `function` call the
  *host's* plugin unless written `plugin.function`. A patch with a `model=` has its fields
  typed from its own plugin's model.

## Parts: what a theme stylesheet can target

Stock widgets carry a `data-part` attribute, so a theme's stylesheet (see
`docs/architecture/themes.md`) can restyle them without depending on class names:

| Part | Element |
|---|---|
| `shell.topbar`, `shell.sidebar`, `shell.aside`, `shell.banner`, `shell.footer`, `shell.content` | the regions of the shell |
| `page.sheet` | the white sheet a page's content sits on |
| `page.control-panel` | the bar with the title, actions and search |
| `page.header`, `page.button` | a `<header>`, a `<button>` |
| `list.table`, `list.row` | a list view's table and each of its rows |
| `form.group`, `form.field` | a `<group>`, and each field (`data-field` is its name) |
| `kanban.column`, `kanban.card` | a kanban column and each card |
| `dashboard.stat`, `dashboard.chart` | a stat tile and a chart (a board's cards are `dashboard.chart` too) |
| `board`, `board.filters` | a board and its filter bar |
| `apps.tile` | a tile of the app launcher |

## Kanban: `<view type="kanban">`

Rows from `source` are grouped into columns by `group_by`. Cards are declared with `<card>`
(or `<templates><card/><menu/></templates>`).

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

Board-style layout uses `layout` weights and titled columns that embed
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

## Board: `<view type="board">`

A dashboard whose cards are **data**: the source decides how many there are, what they show and which
filters sit above them, so a person can build one without a plugin author editing XML. Prism draws all of
its charts and dashboards with it.

```xml
<view type="board" source="dashboard_board" drill="dashboard_drill"/>
```

| Attribute | Meaning |
|---|---|
| `source` | Function called with the page's route values and `filters` (the filter bar's values by filter name, left out until the bar has been drawn once). Answers a board, below. |
| `drill` | Optional function called when a bar, slice, table row or pivot cell is clicked, with the route values plus `chart` (the item's `chart`, else its `id`), `segment` (dimension field → raw value), `filters`, `limit` and `offset`. Answers `{ columns: [{key, label}], rows: [{id?, cells}], total, limit, offset }`, shown in a pop-up with paging. Without it cards are not clickable. |

A board is `{ title?, description?, refresh_secs?, filters: [], items: [] }`.

* **Filters** (`name`, `label`, `kind`, `value`): `period` (a named period such as `this_month`, or
  `{from, to}`; the bar adds "Custom range"), `select` (pick values from `options: [{value, label}]`; the value
  is a list) or `text`. The bar sends every value back under its name on each change; a name left out means
  "not drawn yet" (apply the default), an empty value means the person cleared it.
* **Items** (`id`, `chart?`, `width?`, `card?`, `error?`): `width` is columns out of 12 (full width on a phone).
  An item with `error` shows the message in place of its card, so one failing chart never blanks the board.
* **Cards** (`kind`): `number` (`readings: [{name, label, value, previous, change}]`, optional `target`,
  `progress`, `trend: [{label, value}]`), `bar` `stacked_bar` `row` `line` `area` (`categories`, `keys`, `series:
  [{name, key, values}]`, `dimensions`), `pie` `donut` (the first series), `table` (`columns`, `rows: [{cells, keys}]`,
  `totals`) and `pivot` (`rows`, `columns`, `matrix`, `row_totals`, `column_totals`, `grand`). Every card may carry
  `title`, `description`, `info` (definitions, behind an info mark), `format` (`number`, `compact`, `currency`,
  `percent`, `days`) and `currency`. `meta.skipped` lists filters that did not fit the card.

Colours come from the theme's `--chart-1` … `--chart-5`. The board refreshes when data changes on the page, and
every `refresh_secs` when the source asks for it.

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
behaviour. `board` draws data-driven dashboards. `gantt`, `graph` and `map` views do not exist yet.
