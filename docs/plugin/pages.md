# Pages and data

A page is XML. The widgets on it get their data by calling **functions of the plugin the page
belongs to**, as the person looking at the page (so the kernel's rules for logged-in members
and anonymous visitors apply as usual: a public page can only use functions listed in
`public_functions`).

```xml
<page route="/notes" title="Notes" model="note">
  <view type="form" function="add_note" submit="Add note" success="Note added">
    <field name="title" label="Title" required="true"/>
    <field name="body" label="Body" fieldType="text" required="true"/>
  </view>

  <view type="list" model="note" source="list_notes" open="/notes/{id}">
    <search/>
    <columns>
      <column field="title" label="Title"/>
      <column field="body" label="Body"/>
    </columns>
    <actions>
      <action name="remove" label="Delete" function="remove_note"
              confirm="Delete this note?" danger="true"/>
    </actions>
    <empty title="No notes yet" description="Add the first one above."/>
  </view>
</page>
```

## List: `<view type="list">`

| Attribute / child | Meaning |
|---|---|
| `source` | Function that returns the rows: an array, or an object holding one under `rows`, `items` or `data`. Called with the page's route values (`{ "id": "abc" }` for `/notes/{id}`). |
| `<columns><column field label fieldType/>` | The columns. Without them, the fields of the first row. `fieldType`: `badge`, `boolean`, `date`, `datetime`, `currency`. |
| `open="/notes/{id}"` | Clicking a row goes here. `{field}` is filled from the row; `{id}` is the record's key. |
| `<search/>` | A search box that filters the loaded rows. |
| `<actions><action function label confirm danger/>` | A button per row that calls `function` with `{ "id": <the row's key> }`. With `confirm`, it asks first. The list reloads afterwards. |
| `<empty title description/>` | Shown when there are no rows. |

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

Widget status: `list`, `form`, `group`, `notebook` and chatter work. `kanban`, `dashboard`, `chart` and `stat`
are placeholders with no data binding. `tree` and `pivot` currently render as a plain list, and `statusbar`
and `buttonbox` are generic header and action containers with no state behaviour. `calendar`, `gantt`, `graph`,
`map` and report views do not exist yet. A `list` searches only the rows it has loaded: it has no server-side
sort, filter or pagination yet.
