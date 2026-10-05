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
can be renamed too), `link` (a record id of another model, its `target`: of the same plugin, or of another plugin, see below) and `json`.
A field can be `required`, have a `default`, and be indexed (`"index": "plain"` or `"unique"`).
The tables are schemafull: the database enforces the types as well.

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
  plugin. Whether the record exists is not checked, here or for links inside a plugin.
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

Composite indexes, `decimal` and `file` fields, a data migration for unsafe type changes (shipped
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
