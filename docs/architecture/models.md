# Models

A model describes the records of a plugin: which fields they have, their types and rules. It is
a JSON file, `models/<name>.json`, in the plugin. Every model and every field has a stable **id**
next to its **name**:

```json
{
  "model_id": "mdl_4fs4ygtivr",
  "name": "note",
  "label": "Note",
  "title_field": "title",
  "fields": [
    { "id": "fld_prefhbkkqr", "name": "title", "label": "Title", "type": "string",
      "required": true, "max_length": 200, "index": "plain" },
    { "id": "fld_5qya6s4guw", "name": "body", "type": "text", "required": true },
    { "id": "fld_5v6ldprbyc", "name": "status", "type": "select", "default": "draft",
      "options": [ { "id": "opt_exc7qobnhq", "value": "draft" }, { "id": "opt_k3v9xq2m7a", "value": "shared" } ] }
  ],
  "view": { "list": ["title", "status"], "sort": [{ "field": "title" }] }
}
```

## Names are for people, ids are for the database

Plugin code and pages say `title`. The database stores the value under `fld_prefhbkkqr`, in the
table `mdl_4fs4ygtivr`. The kernel translates both ways, and every table and column carries its
readable name as a `COMMENT`. So:

- **Renaming** a field or changing its label changes the definition only. No column is added,
  no record is touched, no migration is written. The column keeps its id.
- Organizations on different versions of a plugin share a table safely: the version that calls a
  field `title` and the one that calls it `heading` mean the same column.
- **Removing** a field **hides** it: plugins stop seeing it, its column and data stay, and its id is
  never reused. Putting the same id back restores it.

## Field types

`string` (short text, optional `max_length`), `text`, `int`, `float`, `bool`, `date`
(`YYYY-MM-DD`), `datetime` (RFC 3339), `select` (one of `options`, each with its own id so an option
can be renamed too), `link` (a record id of another model, its `target`: of the same plugin, or of another plugin, see below), `json`,
`decimal` and `many2many`, described next.
A field can be `required`, have a `default`, and be indexed (`"index": "plain"` or `"unique"`).
The tables are schemafull: the database enforces the types as well.

### Decimal: exact numbers

`{"name": "salary", "type": "decimal", "scale": 2}` holds money, pay or days of leave without
floating point. `scale` is the number of digits after the point (default 2, at most 9) and cannot change
once records exist. Plugins send and receive decimals as **text** (`"1234.50"`; a JSON number is accepted
as input); more digits than the scale are refused, never rounded. The database stores whole units of the
smallest digit (`123450`), so `sum`, `avg`, comparisons, ordering and `db::increment` are exact and done
by the database. Aggregates over a decimal come back as text too.

### Limits on a value

| Option | On | Means |
|---|---|---|
| `min`, `max` | `int`, `float`, `decimal` | the value is at least / at most this (a number, or text for a decimal). `min` above `max` is refused when the model loads |
| `min_length`, `max_length` | `string`, `text` | the fewest / most characters |
| `pattern` | `string`, `text` | a regular expression the **whole** value must match (`"[A-Z]{3}-[0-9]+"`). Size-limited; an invalid one is refused when the model loads |

A value outside its limits is refused with a plain message ("must be at most 10").

### Naming series

`{"name": "number", "type": "string", "index": "unique", "sequence": {"pattern": "INV-{YYYY}-{#####}", "reset": "yearly"}}`
numbers records. A create that leaves the field out gets the next number; one that supplies a value keeps it and uses
no number. The pattern is literal text (letters, digits and ` -_/.:`), `{YYYY}`, `{YY}` and `{MM}` (the date of
creation, UTC) and exactly one `{#####}` (the zero-padded counter; more digits than the width are never cut).
`reset` is `never` (default), `yearly` or `monthly`: it starts a new counter per period. The counter row
(`aether_sequence`) is incremented in the same transaction as the create, so a create that fails gives its number
back: numbers are never skipped or repeated. Give the field a `unique` index if hand-typed values could collide.

### Checks on the whole record

```json
"checks": [
  { "require": { "or": [ { "status": { "ne": "paid" } }, { "paid_on": { "null": false } } ] },
    "message": "a paid invoice needs the date it was paid" },
  { "require": { "or": [ { "end": { "null": true } }, { "end": { "gte": { "field": "start" } } } ] },
    "message": "the end is before the start" }
]
```

`require` is a filter (see [Queries](queries.md); `{ "field": "other" }` compares two fields of the record). After every
create and update of a record the kernel checks that the record still matches each `require`, inside the write's
transaction, and refuses the write with `message` otherwise, leaving nothing behind. Field names are checked when the
model loads. A check runs on whole-record writes (`db::create`, `db::update`, and inside `db::transaction`); a
`db::increment` is not checked. At most 20 per model. A record that already breaks a check cannot be updated at all
until it is fixed, so add a check to a model with data only after the data satisfies it.

### Calculated fields

Two options make the kernel fill a field, so a plugin no longer keeps a copy in step by hand. Both are
**read-only for plugins** (a write is refused), **stored** (so they can be filtered, ordered and indexed), and
refilled by the kernel on **every create and update** of the record, inside the write's transaction.

* `"related": "customer.name"` copies a field of the record a link points at. The link must point at a model of the
  same plugin, and the field must have the same type (and decimal scale); links, many2many, json and select fields
  cannot be copies. It is a *copy*: when the customer is renamed, the copy changes the next time the record is
  written, not before. Clearing the link clears the copy.
* `"compute": "qty * price"` is a number worked out from the record's own `int` and `decimal` fields: numbers, field
  names, `+`, `-`, `*` and parentheses (up to 8 deep). It is integer arithmetic on the stored whole units, so it is
  exact; a result with more digits than the field has is rounded half away from zero (`58.97 * 0.075` into a
  2-digit field is `4.42`). A missing number counts as zero. There is no division, because it cannot be exact:
  work it out in the plugin. A calculated field may use another one declared before it. The expression's
  field names are checked when the model loads.

```json
{ "name": "subtotal", "type": "decimal", "scale": 2, "compute": "qty * price" },
{ "name": "total",    "type": "decimal", "scale": 2, "compute": "subtotal - discount" },
{ "name": "customer_name", "type": "string", "related": "customer.name" }
```

Not yet: totals over child rows (needs a one-to-many field), and propagating a changed source to its copies.

### Several fields in one index

`"indexes": [{"fields": ["person", "post"], "unique": true}]` on the model indexes fields together
(2 to 8, in the order you filter by). Each index has its own stable `idx_` id, so renaming a field never
rebuilds it; `aether --sync-models` assigns the ids.

### Trees and many-to-many: graph edges

* `{"name": "parent", "type": "link", "target": "unit", "hierarchy": true}` on a model that links to
  itself makes a **tree** (org units, reporting lines). The column stays the source of truth, and the
  kernel keeps a SurrealDB graph edge from parent to child in step with it, in the same transaction as
  every write. A parent that does not exist, a parent that would make a loop, and deleting a record that
  still has children are all refused. `db::tree` returns descendants (`down`) or ancestors (`up`) to any
  depth (default and maximum 64) in one query, with `include_self`.
* `{"name": "skills", "type": "many2many", "target": "skill"}` has no column at all, only edges from this
  model's records to records of the target. `db::relate` / `db::unrelate` change them (linking twice is
  harmless, targets must exist and be of the target model), `db::related` reads them, forwards or
  `reverse`. Deleting a record removes its edges.

The edge tables are named after the model and field ids (`edg_<model id>_<field id>`), are `ENFORCED`
relations, and are never named by plugins: they name a field of a model they were granted, so the usual
grants and audit apply. Making an existing link a hierarchy builds its edges from the column.

## Links to another plugin's model

A `link` whose `target` is `plugin.model` points at a model of another plugin:

```json
{ "name": "currency", "type": "link", "target": "currency.currency", "target_id": "mdl_khholv2eis" }
```

* **The ids.** Like every id, `target_id` is written by `aether --sync-models`, which looks the model up in the
  catalog (so the other plugin must be loaded first: load plugins in dependency order). Loading a plugin then
  needs no lookup, and a model keeps its id for ever, so the link survives renames.
* **Checked when the plugin is loaded:** the other plugin is in `dependencies`; the catalog has a model of that
  name with that `target_id` (a typo or a stale id is reported with what to do).
* **Checked on every write:** the value must be a record id into that model's table, as for a link inside the
  plugin, and the record must exist: the check runs in the same transaction as the write and refuses it
  otherwise (for hierarchy links the parent check already did this). A record deleted later can still leave
  a link dangling.
* **Installing** the linking plugin in an organization installs its dependencies first, so the other model's table
  is there.
* Reading the linked record means calling the other plugin (`plugins::invoke`), which applies its own rules.
  Plugins' `access_models` still name only their own models.

## Every write is checked

The kernel refuses a field that is not in the model, a value of the wrong type, a missing required
value, a select value that is not an option, a text that is too long. Defaults are filled in on
create. A `null` clears an optional field on update. Filters and ordering take field names too.

## Ids are assigned by a tool

A hand-written model starts without ids. `aether --sync-models <plugin>` gives every model, field and
option that lacks one an id and writes the file back. It never changes an id that exists. Loading a
plugin whose models have no ids is refused, so a rename can never be mistaken for "delete this
column, add another". `[[models]]` in `plugin.toml` is no longer supported.

## What an upgrade does in an organization

`--install-plugin` and `--upgrade-plugin` compare each model with what was last applied there (the
snapshot in the organization's `model_schema` table):

| Edit | Result |
|---|---|
| Rename, relabel, reorder, change the view | Nothing moves (a renamed column's comment is refreshed) |
| Add a field | The column is defined; records are untouched |
| Add a required field, or make one required, when records exist | Needs a `default`, written into the records that lack a value; otherwise blocked |
| Remove a field | Hidden, data kept |
| Widen a type (int to float, string to text, select to string, ...) | The column is redefined |
| Any other type change | Blocked unless the table is empty: it needs a migration |

Everything is checked first: one blocked model stops the install or upgrade before anything changes.

## Editing in the web app

Developer tools, Models. Pick a model, change its fields, **Preview changes** to see what it would do
in each organization, then **Save**. Saving makes a new version of the plugin (only the model file is
stored; everything else is shared with the version it came from) and moves the organizations you tick
to it. The others keep the version they have.

**Also write the model file** is on by default: the change goes to the database *and* to
`models/<name>.json` in the folder the plugin was loaded from, so the source stays equal to the
database (loading that folder afterwards is recognised as the same content). Turn it off to change the
database only and leave the JSON file alone. If the plugin's folder is not on the machine, only the
database can be changed.

A model's *name* cannot be changed in the editor, because plugin code, pages and `access_models`
refer to it; its label can. A new model also needs an `access_models` entry in `plugin.toml` before
the plugin can use it.

API (developers only): `GET /api/ui/models`, `POST /api/ui/models/{plugin}/plan`,
`PUT /api/ui/models/{plugin}` with `{ model, write_file, apply_to }`.

## Not yet

`file` fields, one-to-many child lines, a data migration for unsafe type changes (shipped
by the plugin), and per-organization custom fields on top of a plugin's models.

## Extending and inheriting models (design, decided; not built yet)

Two ways for one plugin to build on another plugin's model. Both need the other plugin as a
declared dependency (`[dependencies]` in the manifest, with a version range). Dependencies set
the load order, cycles are rejected, and a plugin cannot be uninstalled while another installed
plugin depends on it.

### Extend: add fields to the model

```json
{ "id": "mdl_x1", "extends": "notes.note",
  "fields": [ { "id": "fld_a9", "name": "priority", "type": "int", "default": 0 } ] }
```

* No new table. The extender's columns are defined on the owner's table under their own field
  ids, so they cannot collide with the owner's columns or another extender's.
* The effective schema of a model, per organization, is the owner's fields plus those of every
  installed extender. The planner emits `DefineField` against the owner's table, owned by the
  extender.
* The owner's code only sees the owner's fields (decoding is schema-aware per plugin). The
  extender sees the owner's fields (through its grant) plus its own.
* An extension field cannot be `required` without a default: the owner inserts rows without
  knowing about it.
* Field names only need to be unique within one plugin's view; ids carry identity.
* Uninstalling the extender hides its fields and keeps the data, like any removed field.
* The owner must allow it with `"extensible": true` on the model (default false), and the
  extender holds an `extend` grant on the model.

### Inherit: a new table with the parent's fields

```json
{ "id": "mdl_y2", "inherits": "notes.note",
  "fields": [ { "id": "fld_b1", "name": "due", "type": "date" } ] }
```

* The child has its own table with the parent's fields plus its own. Parent field ids carry
  over unchanged (the same id in another table is fine), so renames follow.
* **Live:** the child follows the parent. A new parent field is added to the child, a hidden one
  is hidden, and a type change the parent's data cannot take is blocked for the child as well.
* The child can add fields and change defaults. It cannot remove or retype inherited fields.
* Parent and child rows are separate; a query on the parent never returns child rows.
* A model may inherit from one model and also be extended. Chatter is configured per model and
  is not inherited.
