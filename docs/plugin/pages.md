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
| `menu_group_icon="users"` | Lucide icon for that section (otherwise the first page icon, or a folder). |
| `menu_description="Leads not yet contacted"` | A line under the menu entry where the layout has room for it (the `mega` navigation of some themes). At most 200 characters. |
| `menu_badge="open_leads"` | A count beside the menu entry: the web app calls this function (`fn` of this plugin, or `plugin.fn`) for the person looking, through the ordinary call API, every minute and after each navigation. A positive number (`99+` above 99), a list (its length), `{ "count": n }` or short text shows; zero, `null` or a failure shows nothing. |
| `icon` / `sort` | Lucide icon id drawn on the sidebar link, and order among menu entries. Lowercase letters, digits and `-` (for example `calendar-days`). |
| `roles="hr_user,hr_manager"` | Who may open the page and see it in menus (OR). Short names become `<plugin>.name`. Omit = any member who holds a role (of the plugin, when it offers roles). `roles="member"` = any signed-in member, even one with no role: use it for pages meant for customers. `org_admin` always passes. |

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
| `pane="true"` | With `open`: the row's page opens beside the list, in a resizable pane, instead of replacing it. |
| `edit="contact"` | Clicking a row opens the `<modal name="contact">` of the same page (or tab) on that row, instead of going anywhere: see [A dialog that adds and edits](#a-dialog-that-adds-and-edits-modal). Wins over `open` when that modal is there. |
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
| `redirect` | Where to go after saving; `{field}` is filled in from the route values, the form's values and what the function answered (a function that creates a record answers `{ id }`, so `redirect="/notes/{id}"` opens the new one). |
| `clear` | Whether the form empties after saving. By default a form without `source` does. |

`<field name label fieldType required options help/>` inside a form edits that field: `fieldType` is
`char` (default), `text`, `integer`, `float`, `currency`, `boolean`, `date`, `datetime`,
`file` (attachments: `multiple="true"` for several, `guard="<plugin>"` to upload them under that plugin's media guard so only the uploader can read them until the plugin reads them with `media::read`; `readonly="true"` shows and downloads only), `selection` (with `options="a, b, c"`), `image` / `binary` (file picker that stores a resized
data URL string on the form), or `many2one` / `link` (searchable picker), or `many2many` with a `source` (the picked rows as chips with a ×, and a search box to add more; the value is a list of ids), or `markdown` (shows the value as formatted text: headings, lists, code, links (web addresses, and paths inside the app such as `/wiki/p/abc`), bold and italic; it never draws HTML, and it is for display, so use `text` to edit it). For links, set
`source` to a plugin function that returns rows (`list_departments`, or `{ rows, total }`);
optional `optionLabel` picks the label field (default `display_name` / `name`), and `entity`
is the plural noun in empty/search hints. `fills="currency_code"` (names separated by commas) copies those values from the chosen row into the form's fields of the same names, so picking a country can also set its currency; a value the row lacks leaves the field as it was. `depends="job,grade"` (one or more other fields of the form, separated by commas) sends their current values to `source` and reloads the picker when any of them changes, dropping a chosen value the new list no longer offers: a position's "Reports to" follows the job and grade being typed, not the saved ones. Fields can be grouped with `<group>` and `<notebook>`.

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
| A button with a `function` | When the function answers `{ "open_url": "https://..." }` the browser opens that address in a new tab (this tab if the browser blocks the new one). Only `http` and `https`. Used to join a meeting or open an editor on another site. When it answers `{ "copy_text": "...", "copied_notice": "..." }` the text goes on the clipboard and the notice (optional) shows by the button; if the browser refuses, the text is shown to copy by hand. When it answers `{ "open_page": "/some/page" }` the person goes to that page of the app instead (a path starting with one `/`). |
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

### A dialog that adds and edits: `<modal>`

`<modal label title>` is a button that opens a dialog holding its children, usually a form that adds
a row to a list beside it. The dialog closes when its form saves, and the list loads again.

Give the modal a `name` and the list `edit="<name>"`, and clicking a row opens the same dialog on that
row. The form inside then loads the row with `edit_source` and saves it with `edit_function` (both are
sent the row's `id` with the page's route values), so one set of fields both adds and edits:

```xml
<modal name="contact" title="Add emergency contact" edit_title="Emergency contact" label="Add contact">
  <view type="form" function="add_emergency_contact" submit="Add" success="Contact added"
        edit_source="get_emergency_contact" edit_function="update_emergency_contact"
        edit_submit="Save" edit_success="Contact saved">
    <field name="name" label="Name" required="true"/>
    …
  </view>
</modal>
<view type="list" source="list_emergency_contacts" edit="contact">…</view>
```

| Attribute | Meaning |
|---|---|
| `name` (modal) | What a list's `edit` names. Names are per page; a tab another plugin adds has its own. |
| `edit_title` (modal) | The dialog's title while editing (default `title`). |
| `width` (modal) | `sm`, `md`, `lg` (default) or `xl`. |
| `edit_source` / `edit_function` (form) | Load and save the row being edited. Without `edit_function` the form only adds. |
| `edit_submit` / `edit_success` (form) | The button text and saved message while editing (default `submit` / `success`). |

The modal's own button always opens it empty, to add. An edited row keeps no draft and shows no
record bar or chatter in the dialog; those belong to a record's own page.

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
| `columns` | Optional function returning `[{ "id", "label" }]`: the columns, in that order, empty ones included. Rows whose value is not listed go to `Unassigned`. |
| `move` | On drag between columns, called with `{ id, group_by_field, from, to }` then the page refreshes. Dropping on a card adds `before` (that card's id) so the plugin can keep an order. |
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
* **Cards** (`kind`): `number` (`readings: [{name, label, value, previous, change, tone?}]`, optional `target`,
  `progress`, `status` (`on_track` or `behind`), `lower_is_better`, `trend: [{label, value}]`; `tone` is `good`,
  `bad` or `flat`, and decides the colour of the change), `bar` `stacked_bar` `row` `line` `area` (`categories`, `keys`, `series:
  [{name, key, values}]`, `dimensions`), `pie` `donut` `funnel` (the first series; a funnel draws the stages from the largest down), `table` (`columns`, `rows: [{cells, keys}]`,
  `totals`), `pivot` and `heatmap` (`rows`, `columns`, `matrix`, `row_totals`, `column_totals`, `grand`; a heatmap shades every
  cell and leaves out the totals), `map` (like a pie: `categories` that are country names or two or three letter codes,
  `series[0].values`; a bubble sits on each country, and categories that are not countries are listed under it),
  `bubble` (`points: [{label, key, x, y, size?}]`, `axes: {x, y, size?}`, `dimensions`), `sankey` (`nodes: [{level, field, key, name, value}]`,
  `links: [{source, target, value}]` where `source` and `target` index `nodes`, `measure`, `dimensions`; clicking a node
  drills with `{field: key}`, a band with both ends) and `text` (`text`, a note between charts). A card with a `target` draws it as a
  dashed line on a bar, row, line or area chart. Every card may carry
  `title`, `description`, `info` (definitions, behind an info mark), `format` (`number`, `compact`, `currency`,
  `percent`, `days`) and `currency`. `meta.skipped` lists filters that did not fit the card.

Colours come from the theme's `--chart-1` … `--chart-5`. The board refreshes when data changes on the page, and
every `refresh_secs` when the source asks for it.

## Chart builder: `<view type="chart_builder">`

A form for building a chart of a `board`, with the fields of the chosen data offered as pickers and a live preview
drawn by the same card views a board uses. Prism uses it for creating and editing charts.

```xml
<page route="/prism/charts/new" query="workbook" title="New chart">
  <view type="chart_builder"/>
</page>
```

Every attribute names a function of the page's plugin and has a default, so the view works without any:
`get` (`get_chart`, called with the route's `id`, only on a page that has one), `create` (`create_chart`), `save`
(`set_chart`, called with `id`), `run` (`run_chart`, called with an unsaved `spec`; answers a card), `sources`
(`search_sources`, rows of `{id, name}`), `fields` (`get_source`, called with `source`; answers `{fields: [{name, label,
kind, options, groupable, numeric, temporal}]}`), `values` (`field_values`, called with `source` and `field`; rows of
`{value, label}`) and `workbooks` (`search_workbooks`). `redirect` names where a new chart goes after saving
(`{id}` is the new chart); by default its own page. The view reads `workbook` from the page's query to preselect one.

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

## Showing a file: `<viewer>`

`<viewer source="get_file" height="480"/>` shows a file inline with the [universal viewer](../architecture/document_viewer.md).
`source` is a function of the plugin, called with the page's route values, that returns a stored file
(`{ "key", "name", "mime", "size" }`, the value of a file field) or `{ "file": ... }`. Without `source`, give the file with
`key`, `name`, `mime` and `size`. `height` is in pixels (120 to 1200, default 480). A file field on a form opens the
same viewer in a side panel that steps through the record's files, with a link to the full page at `/view/<key>`.

## Pages that fill the window: `shell`, `panes`, optional route values and `query`

Most pages are one scrolling sheet under a title bar. An app made of several regions (a channel list, the messages, a
thread) takes the whole area instead.

```xml
<page route="/chat/{channel?}" shell="none" query="thread" title="Chat">
  <panes focus="{channel}" back="/chat">
    <pane name="rail" title="Channels" width="260" min="200" max="400" collapsible="true">
      <view type="list" source="my_channels" open="/chat/{id}"/>
    </pane>
    <pane name="main">
      <view type="list" source="channel_messages"/>
    </pane>
    <pane name="thread" width="360" show="{query.thread}">
      <view type="list" source="thread_messages"/>
    </pane>
  </panes>
</page>
```

| Attribute | On | Meaning |
|---|---|---|
| `shell="none"` | `<page>` | No title bar and no sheet; the page fills the area under the app chrome and each pane scrolls by itself. `shell="chrome"` (default) is the usual page. |
| `{name?}` | last segment of `route` | An optional route value. `/chat/{channel?}` answers `/chat` and `/chat/7`; `channel` is missing from the values on `/chat`. A page with a lone optional tail gets a sidebar entry that opens the route without it (`/chat`). Only the last segment may be optional. |
| `query="thread,q"` | `<page>` | The query-string values the page reads (up to 8 names). They are available as `{query.thread}` in attributes such as `show`, `href` and `open`, and are passed to the page's `source` functions as `query: { thread: "…" }` (only when present). Other query values are ignored. Changing them does not load the page again. |
| `<panes direction="row\|column" focus back>` | region | Holds the `<pane>` children. `direction` defaults to `row`. |
| `focus="{channel}"` | `<panes>` | On a narrow screen only one pane shows: the last visible one, unless this value is empty, in which case the first (the list to choose from). |
| `back="/chat"` | `<panes>` | Where the Back button goes on a narrow screen; without it, the browser's previous page. |
| `<pane name title width min max grow collapsible show>` | inside `<panes>` | A region. A `width` (pixels) gives a fixed size that the person can drag between `min` and `max` (arrow keys work on the edge); without it the pane takes the space left. `collapsible="true"` adds a button that folds the pane to a strip. Size and fold are remembered per person in the browser. `show` is a condition: the pane is shown when it is not empty, `false`, `0` or `no` after `{name}` and `{query.name}` are filled in. |

## A title with buttons: `<heading>`

`<heading source="channel_header" topic="channel.{channel}"> <button .../> </heading>` shows the title of what the page is about,
with its children (buttons) on the right. `source` is a function called with the page's values that answers an object;
`title_field` and `subtitle_field` (default `title` and `subtitle`) pick what is shown. Without `source`, `title` and
`subtitle` are used as written. With `topic` it asks again when something is published there.

`{me}` is a page value like a route value: the key of the signed-in person. It is useful in per-person topics
(`topic="inbox.{me}"`).

## A checklist: `<checklist>`

```xml
<checklist source="task_checklist" add="add_check" toggle="toggle_check" remove="remove_check" title="Checklist" topic="planner.{id}"/>
```

`source` returns `[{ id, text, done }]`. `add` is called with the page's arguments and `{ text }`, `toggle` with `{ id, done }`
and `remove` with `{ id }`. `topic` makes the list reload when the plugin publishes to a live topic.

## Connecting a person's own login: `<login>`

```xml
<login bridge="jmap" title="Your mailbox" help="Your user name and password on the mail server."/>
```

Where a person connects their own login for a bridge that works on their behalf (their mailbox; see [Bridges](../bridges.md)). The kernel checks the
login with the service before keeping it, stores the password encrypted and never shows it again; the plugin never sees it. The element shows who is
connected and offers to replace or disconnect.

On `<files>`, `editor="open_in_editor"` names the function that opens an office file in the browser's editor: it is called with `{ file }` and answers
`{ open_url }`; the row menu then offers "Edit online" (or "Open in editor" to someone who may only read).

## A video or audio player that reports: `<player>`

```xml
<player source="lesson_player" report="watch_here" interval="15"/>
```

Plays a video or audio file and tells the plugin how far the person got. `source` is called with the page's values and answers
`{ src, mime?, title?, position?, no_skipping?, enrolled?, link? }` (`src` is an address on this site, such as `/api/media/<key>` for an
uploaded file; a guarded file is served only if its plugin allows it). The player starts at `position` seconds. While it plays
(every `interval` seconds, 5 to 120, default 15), and when it pauses or ends, `report` is called with the page's values and
`{ seconds, ended }`; `seconds` is the furthest point reached by *playing*, never by jumping, and `ended` is true when the file
reached its end. With `no_skipping` a jump ahead of that point is undone. Nothing is reported when `enrolled` is `false`. When there
is no `src` but a `link` (a web address), the player shows a link to it instead, because a video on another site cannot report.
The server must not trust `seconds`: it should limit how far it can move between two reports.

## A form made of data: `<questionnaire>`

```xml
<questionnaire source="questionnaire" submit="submit_response" redirect="/forms"/>
```

For forms whose questions are records (a survey), not page XML. `source` is called with the page's values and answers
`{ questions: [{ id, label, kind, required, help, options }], locked?, intro?, anonymous?, thanks? }`. `kind` is `text`, `long`,
`choice`, `dropdown`, `multi`, `rating` (1 to 5), `yes_no`, `number` or `date`. A `locked` text replaces the questions with that
message (closed, already answered, a preview). `submit` is called with the page's values and `{ answers: { <question id>:
answer } }` (only the questions that were answered; a `multi` answer is a list); it answers `{ thanks? }`, which is shown, and the
page moves to `redirect` a moment later. A required question left empty stops the form in the browser before anything is
sent; the plugin must check again.

**Timed mode** (a quiz attempt). When `source` also answers `seconds_left`, the page shows a countdown from that number (the
server's clock, so a wrong clock in the browser changes nothing) and sends the answers by itself when it reaches zero, required
questions or not. `saved` is `{ <question id>: answer }` already given, to carry on from. With `save="save_answer"` that function is
called, a moment after each change, with the page's values and `{ question, answer }` (`answer` is `null` when cleared), so a closed
tab loses nothing. With `watch="report_violation"` it is called with the page's values and `{ kind: "left_tab" }` each time the person
switches away from the tab. A question may carry `points`, shown beside it as marks. The plugin must still check the time itself:
the countdown is for the person, not a guard.

## A file browser: `<files>`

```xml
<files source="drive_browse" open="/drive/{id}" register="register_file" create_folder="create_folder"
       rename="rename_item" move="move_item" trash="trash_item" restore="restore_item" purge="purge_item"
       empty_trash="empty_trash" share="share_item" unshare="unshare_item" shares="list_shares" people="list_people"
       versions="file_versions" restore_version="restore_version" new_version="new_version" tree="folder_tree"
       search="search_files" favourite="toggle_favourite" comments="file_comments" comment="add_comment"
       delete_comment="delete_comment" links="list_links" create_link="create_link" revoke_link="revoke_link"
       usage="storage_used" guard="true" settings="/drive/settings"/>
```

A table of folders and files with breadcrumbs, upload (the button, or dropping files on the list), new folders, a menu on
each row, and dialogs to rename, move, share and look at versions. Files open in the [document viewer](../architecture/document_viewer.md)
panel, which steps through the folder's files. `source` is called with the page's route values and query (a `folder` value
and `query: { view }` for the drive) and answers:

```json
{ "view": "folder", "folder": { "id": "…", "name": "Projects", "level": "edit" },
  "path": [ { "id": "…", "name": "Projects" } ], "can_write": true,
  "folders": [ { "id": "…", "name": "Q3", "owner_name": "Ann", "modified_at": "…", "level": "owner", "shared": true } ],
  "files": [ { "id": "…", "name": "plan.pdf", "mime": "application/pdf", "size": 1234, "version": 2, "owner_name": "Ann",
               "modified_at": "…", "level": "view", "shared": false, "media": { "key": "media/…" } } ] }
```

`view` is `mine`, `shared`, `recent`, `trash`, `search` or `folder`; `level` on a row is `view`, `edit` or `owner` and
decides which actions its menu offers (view: open and download; edit: also rename, move, trash; owner: also share). Each
other attribute names a function of the plugin, and an action whose function is not named is not offered:

| Attribute | Called with | Used for |
|---|---|---|
| `register` | `{ folder?, media }` (`media` is what the upload returned) | Making an uploaded file part of the folder |
| `create_folder` | `{ name, parent? }` | New folder |
| `rename`, `move`, `trash`, `restore`, `purge` | `{ kind, id, name? , to? }` (`kind` is `file` or `folder`) | Row actions; `empty_trash` takes `{}` |
| `share`, `unshare`, `shares` | `{ kind, id, user, level? }` / `{ kind, id }` | The share dialog; `people` lists `[{ id, name }]` to choose from (`org` means everyone) |
| `versions`, `restore_version`, `new_version` | `{ file }`, `{ file, version }`, `{ file, media }` | The versions dialog |
| `tree` | `{}` | The folders offered when moving (`[{ id, name }]`) |
| `search` | `{ q }` | The search box; it answers a listing. A row may say `found_in: "text"` (the words are inside the file) |
| `favourite` | `{ kind, id, on }` | "Add to favourites" / "Remove from favourites"; rows say `favourite: true` and get a star |
| `comments`, `comment`, `delete_comment` | `{ file }`, `{ file, body }`, `{ id }` | The comments dialog: `[{ id, author_name, body, at, can_delete }]` |
| `links`, `create_link`, `revoke_link` | `{ kind, id }`, `{ kind, id }`, `{ id }` | Sharing by link, inside the share dialog: `[{ id, path, text, expired }]`; the dialog shows the full address with a Copy button |
| `usage` | `{}` | `{ bytes, quota_bytes }`, shown in the header (`quota_bytes` 0: no limit) |
| `guard` | `"true"` | Not a function: uploads are guarded for this page's plugin from the start ([Media](../architecture/media.md#files-a-plugin-guards)) |
| `settings` | a route | Not a function: a link to a settings page, shown when the listing says `features.settings` |

The listing may also carry `features: { links, comments, settings }` (all on when missing): an organization that switched
comments or links off hides those actions. `view` may be `favourites` (and `link`, for a page that opens a shared link).

`open` is where a folder leads (`{id}` is filled in). The stored files are best guarded by the plugin, see
[Media storage](../architecture/media.md#files-a-plugin-guards).

## Moving around an app: `<rail>`

```xml
<rail source="my_channels" topic="inbox" active="{channel}" open="/chat/{id}" search="true" title="Channels"/>
```

A list of places to go (channels, teams, people, folders) with unread badges and online dots. `source` is a function of
the plugin called with the page's values. It answers rows, or `{ groups: [...] }`:

```json
[ { "id": "chat_channel:7", "name": "general", "group": "Channels", "unread": 3, "online": false, "muted": false },
  { "id": "chat_channel:9", "name": "secret",  "group": "Private" } ]
```

or `{ "groups": [ { "id": "team-a", "label": "Team A", "items": [ { "id": "1", "name": "Ops" } ] } ] }`. Rows without an
`id` are left out; the label is `name`, then `label`, then `title`; rows with no `group` are listed without a heading.

| Attribute | Meaning |
|---|---|
| `source` | The function above. |
| `open` | Where an item leads; `{id}` (the key of the record) and any other `{field}` of the row are filled in. |
| `active` | The id of the item to mark as current, usually a route value (`{channel}`). The current item is highlighted, has `aria-current="page"`, and shows no badge. |
| `default` | The id marked when `active` is empty (a page opened without choosing a place, such as "My files"). |
| `topic` | A [realtime](../architecture/realtime.md) topic: when something is published there the list asks `source` again, so badges and dots follow what happens in other channels. Keep the answer small: it is asked again on every message. |
| `search="true"` | A filter box over the labels (`placeholder` changes its text). |
| `avatar="true"` | Draws a round avatar before each label: the row's `image` (a URL) when it has one, otherwise the first letter of a direct chat (`kind: "direct"`), `+` for a group (`kind: "group"`) and `#` for any other row. The online dot moves onto the avatar. |
| `id_field`, `label_field`, `badge_field`, `mention_field`, `online_field`, `group_field`, `icon_field` | Field names, default `id`, `name`, `unread`, `mentions`, `online`, `group`, `icon`. An item with `mentions` above 0 shows a red `@N` mark instead of the plain count. |
| `empty`, `title` | Text for an empty list; the accessible name of the list. |

Group headings fold, and the person's choice is remembered in their browser. The arrow keys, Home and End move between
entries. A badge reads `99+` above 99; a muted item (`"muted": true`) is drawn dimmer and left out of totals.
`<rail>` fills a pane: put it in a `<pane>` of `<panes>`.

## A home page of places: `<tiles>`

```xml
<heading title="Welcome" subtitle="Everything in one place" logo="/api/ui/plugins/atrium/logo"/>
<tiles title="Work" cols="4">
  <tile label="Drive" description="Files and folders" href="/drive" plugin="atrium_drive" icon="hard-drive"/>
</tiles>
```

`<tiles>` is a titled grid of `<tile>`s drawn like the tiles of the Apps launcher. A tile leads to `href`; it shows the
logo of `plugin` (`/api/ui/plugins/<plugin>/logo`) when that plugin ships one, else `icon` (a name from the launcher's
icon set), else the first letter of its `label`. `cols` is 1 to 6 (default 4) and falls to fewer on a narrow screen.
`<heading logo="…">` shows an image before the title. `requires_route="/hr/enrollments"` on a tile shows it only while
a page the person can open at that route is listed in an app menu, so a home page can offer what an optional add-on
provides and show nothing when it is not installed (or the person lacks its role).

## Who is here: `<presence>`, and online dots in `<rail>`

`<presence topic="channel.{channel}" source="people"/>` shows who is connected to a [realtime](../architecture/realtime.md)
topic as a row of avatars, and "N people here". The topic must be declared with `presence = true`; being on the page counts as
being connected. `source` (optional) is a function of the plugin called with `{ users: [ids] }` that answers
`[{ id, name, image? }]`; without it people are shown by the end of their id. `max` (default 6) is how many avatars show.

In a `<rail>`, `presence="online"` joins a topic everyone on the page joins, and an item whose `user` field (change it with
`user_field`) is connected shows a green dot that follows people arriving and leaving.

## Live data: `topic`, and `<messages>`

A data widget (`list`, `kanban`, `board`, `dashboard`, `calendar`, `stat`, `chart`) may carry `topic="channel.{channel}"`.
While it is on screen it joins that [realtime](../architecture/realtime.md) topic of the page's plugin, with the
`{name}` and `{query.name}` parts filled in from the page (the topic is `chat:channel.7` for `/chat/7`), and loads its
`source` again whenever something is published there. A burst of messages causes one load, not one per message. While a
value is still missing (no channel chosen) nothing is joined. The plugin must declare the topic with `[[realtime]]`;
the person is let in only if the plugin's `subscribe` function allows it.

`<messages>` is a conversation:

```xml
<messages source="channel_messages" topic="channel.{channel}" typing="true" limit="50"/>
```

| Attribute | Meaning |
|---|---|
| `source` | A function called with the page's values and `{ limit }` for the newest rows, or `{ limit, before }` where `before` is the id of the oldest row held. It answers the rows **oldest first**, or `{ rows, more }`; without `more`, a full page means there may be more. |
| `topic` | The live topic (above). A message published there with `{ row }` adds the row, or replaces the row with the same id (an edit). The event `deleted` with `{ id }` removes one. A message with neither makes the widget ask `source` for the newest rows again, and so does a `resync`. |
| `typing="true"` | Shows who is typing, from the topic's `typing` client events (the topic must list `typing` in `client_events`). |
| `limit` | Rows per load, 10 to 200 (default 50). |
| `id_field`, `author_field`, `time_field`, `body_field` | The row fields used, default `id`, `author`, `created_at`, `body`. An author may be text or an object with `display_name` or `name`. |
| `empty` | Text shown when there are no messages. |
| `react`, `pin`, `edit`, `delete` | Functions of the plugin for reactions, pinning, editing and deleting. `pin` adds a pin button to the hover toolbar (called with `{ id }`) and shows a pin beside the name of a message whose `pinned` is true. With `react`, a hover toolbar offers a smile button that opens a row of quick reactions, and each reaction shows as a chip with its count that toggles the person's own; a message's `reactions` is `{ "👍": [user ids] }`. With `edit` and `delete`, a person's own messages (the `author` field equals the signed-in person) get edit (Enter saves, Esc cancels) and delete (asks first) buttons. The functions are called with `{ id, emoji }`, `{ id, body }` and `{ id }`, and may answer `{ row }`. A message with `edited_at` is marked "(edited)". |
| `mention_names` (a field of the row) | The names mentioned in the body, a list of strings: `@Name` in the text is highlighted. `@channel` and `@everyone` always are. |
| `author_name_field`, `author_image_field`, `files_field` | The field with the author's display name (default `author_name`; without it the `author` field is shown with any `table:` prefix removed), the field with the address of the author's profile picture (default `author_image`; others' messages show it, or the initials without one), and the field with attached files (default `files`, a list of `{ key, name, mime, size }`). Attached files open in the [universal viewer](../architecture/document_viewer.md) side panel, which steps through the message's files. |

`<composer send="post_message" topic="channel.{channel}" typing="true" attach="true"/>` is the message box that goes with it:
Enter sends, Shift+Enter starts a new line, the box grows to eight lines. `send` is a function of the plugin called with the
page's values, `{ body }` (the name can be changed with `field`) and, when files are attached, `{ files: [...] }`. With
`emoji="true"` shows a smiley button with a searchable picker (group tabs, skin tone, recent), `:smi` suggestions and `:smile:` replacement, using the emoji that enabled plugins register with `[[emoji_pack]]` (see [Emoji packs](emoji.md)); `emoji="name"` uses one pack. The web app has no emoji of its own. `gif="true"` adds a GIF button the same way, with the packs plugins register as `[[gif_pack]]` (see [GIF packs](gifs.md)); the GIF chosen is attached to the message with a preview. With `attach="true"` a paperclip uploads files through the media store first, and files dragged onto the window are attached the same way. Files waiting to be sent show as preview cards above the box (a thumbnail for pictures, a typed tile for the rest, a spinner while uploading, a button to remove). With `typing="true"` the box tells the others on
`topic` that the person is typing (at most every two seconds; the topic must list `typing` in `client_events`). When the
function answers with a `row`, the `<messages>` of the same topic show it at once, so a plugin may publish to everyone but
the sender. With `mentions="list_people"`, typing `@` lists people from that function (called with `{ q }`, answering rows with `name`), the arrow keys, Enter or Tab choose one, and `@channel` is offered too. Other attributes: `placeholder`, `label`, `submit` (the send button's label), `max` (characters, default 4000).

The list opens at the newest message and stays there while new ones arrive; when the person has scrolled up it keeps
their place and offers "N new messages". Scrolling to the top loads earlier rows without moving what is being read.
Consecutive rows by one author within five minutes are grouped, with a heading for each day. The body is shown as plain
text (line breaks kept); nothing in a message is used as markup. The list is virtual: only the rows near the visible part
are in the page, with spacers for the rest, so a long conversation costs about what a short one does. At most 2000 rows
are kept in memory (the oldest are let go while the person is at the bottom, and loaded again when they scroll up).

### `<posts>`: a channel drawn as posts

`<messages>` is a stream: bubbles in the order they were written, right for a chat or a group. A *channel* is better
as a board of posts, the way team workspaces draw it: each post is a card with an author, an optional subject and a
body, its replies under it, and a box to reply in place. `<posts>` draws that:

```xml
<posts source="channel_posts" replies="thread_messages" topic="channel.{channel}" send="post_message"
       react="react" pin="pin_message" edit="edit_message" delete="delete_message"
       emoji="true" gif="true" attach="true" mentions="list_people" start="Start a new post"/>
```

| Attribute | Meaning |
|---|---|
| `source` | A function called with the page's values and `{ limit }` (the newest posts) or `{ limit, before }`; it answers the **top-level** messages oldest first, or `{ rows, more }`. A post row carries `reply_count` and `latest_replies` (its newest replies, oldest first). |
| `replies` | A function called with `{ thread }` (the post's id) that answers the post and all its replies, oldest first; used by "View earlier replies". |
| `topic` | The live topic. A post published with `{ row }` is added or replaces the one with that id; a reply is shown by publishing its post again with fresh `latest_replies`. `deleted` with `{ id }` removes a post. |
| `send` | Called as for `<composer>`: `{ body }`, plus `subject` for a new post, or `parent` (the post's id) for a reply, and `files`. The new-post bar offers a subject line. |
| `react`, `pin`, `edit`, `delete`, `emoji`, `gif`, `attach`, `mentions` | As for `<messages>` and `<composer>`. |
| `start`, `empty`, `limit` | The new-post bar's label, the text for a board with no posts, and posts per load (5 to 50, default 20). |

`<composer subject="true">` shows a subject line above the box and sends it as `subject`.

### `<when>`: draw by what a function answers

`<when source="channel_header" field="kind" is="channel">…</when>` shows its children only when the object that `source`
answers (called with the page's values) has `field` equal to `is`; `isnot` means "different from". Nothing is shown while
the answer is awaited. It lets one page draw a channel as `<posts>` and a chat as `<messages>` and `<composer>`.

## Widget status

`list` (with optional server `page_size`), `form`, `kanban`, `dashboard`, `calendar`, `group`,
`notebook`, `stat`, `chart` and chatter work. `tree` and `pivot` currently render as a plain
list, and `statusbar` and `buttonbox` are generic header and action containers with no state
behaviour. `board` draws data-driven dashboards. `gantt`, `graph` and `map` views do not exist yet.
