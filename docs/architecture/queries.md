# Queries: filters, counts and aggregates

Plugins never write SurrealQL. `db::find`, `db::count`, `db::aggregate` and `db::search` take a
**filter**, and `data_model/query.rs` is the only place that turns it into a `WHERE` clause. Field
names are looked up in the model (and replaced by their stored ids); values are always bound, never
spliced in.

```json
{ "status": "active", "salary": { "gte": 1000, "lt": 5000 },
  "or": [ { "dept": "hr" }, { "dept": { "in": ["it", "ops"] } } ],
  "not": { "manager": { "null": true } } }
```

* `"field": value` is an equality; `"field": { op: value, … }` takes `eq ne gt gte lt lte in nin like null under`.
  `like` is a case-insensitive substring. `null: true` means the field is empty, `false` that it has a value.
  `nin` is also true for a record where the field is empty.
* `under` is for a link field that points at its own model (a `parent`, a `manager`): `{ "parent": { "under": "department:x" } }` matches records whose parent, or parent's parent, and so on up to 12 levels, is that record. A rule can name a whole team tree without a list of ids. It is strictly below: combine with `{ "id": "department:x" }` in an `or` to include the root.
* `eq ne gt gte lt lte` also take `{ "field": "other" }` to compare with another field of the same record
  (`{ "end": { "gte": { "field": "start" } } }`). Both fields must be the same kind of number; for an ordering,
  a record missing either field does not match.
* The keys of one object are ANDed. `and` / `or` take a list of filters, `not` takes one.
* Limits: 8 levels of nesting, 64 comparisons, 500 values in a list. The old flat form
  (`{"field": value}`) is still a filter of equalities.

## Commands

| Command | Payload | Returns |
|---|---|---|
| `db::find` | `model`, `filter`, `order`, `limit` (max 1000), `offset` | records |
| `db::count` | `model`, `filter` | a number |
| `db::aggregate` | `model`, `filter`, `group_by: [field]`, `aggs: { name: "count" \| {sum\|avg\|min\|max: field} }` | one row per group (one row without `group_by`), at most 1000 |
| `db::search` | `model`, `filter`, `q`, `fields`, `order`, `limit`, `offset` | `{ "rows": […], "total": N }` |

`db::search` ANDs `filter` with an OR of `{ field: { like: q } }` over `fields` when `q` is
non-empty, then returns matching rows plus a total count. Prefer it for list pages that take free
text from the faceted search bar.

All four need `db::query`, are limited to the plugin's granted models and write a `data_access` row
(an aggregate lists no record ids). The Rust SDK has `db::Filter`, `Filter::from_json`,
`Find::matching`, `Find::count`, `db::count`, `db::aggregate` and `db::search`.

## Reports

`report::query` is the grouped report. It needs the capability `report::query`. The model is one
the plugin owns, or `dependency.model` when that plugin is in `dependencies` (read only, with that
model's own record rules and hidden fields). One call reads one model and writes one `data_access`
row. The answer has no record ids.

```json
{
  "model": "crm_deal.deal",
  "filter": { "pipeline": "crm_pipeline:1" },
  "dimensions": ["stage", { "field": "expected_close", "grain": "month" }],
  "measures": {
    "n": "count",
    "value": { "sum": "amount" },
    "won": { "count_if": { "closed_at": { "null": false }, "lost_reason": { "null": true } } },
    "slip": { "avg_days": { "from": "expected_close", "to": "now" } }
  },
  "fill": { "model": "crm.crm_stage", "key": "id", "label": "name", "filter": { "pipeline": "crm_pipeline:1" } },
  "order": "-n",
  "limit": 1000,
  "preview": true
}
```

* `dimensions` are field names. `grain` is `day`, `week` or `month`, and only on a date or datetime.
* `measures` are `count`, `{sum|avg|min|max: field}`, `{count_if: filter}` (the same filter language)
  and `{avg_days: {from, to}}`. `to` is a field on the same model, or `"now"`.
* `fill` reads the dimension model and adds groups the fact table did not produce, with zero
  measures, so an empty stage still appears. `fill.model` uses the same own-or-dependency rule.
* `limit` is at most 1000.
* `preview: true` returns only the first `ui.report.preview_rows` groups (5, 10, 25 or 50; default
  10). Organizations override that setting the same way as `ui.list.page_size`. `total` stays the
  full ungrouped measures, so a headline is not clipped. The 1000-row cap does not change.

The answer is `{ rows, total, shown, total_groups, preview_rows? }`.

`report::export` (capability `report::export`) does not query. It turns rows the plugin already has
into a table:

```json
{ "format": "xlsx", "title": "Funnel", "columns": [{ "field": "name", "label": "Stage" }], "rows": [] }
```

`format` is `xlsx` or `pdf`. The answer is `{ filename, content_type, data }` with `data` base64.
At most 1000 rows. The Rust SDK is `report::query` and `report::export`. Rhai and Lua call
`report::query(#{ ... })` and `report::export(#{ ... })`.

Successful `db::create` / `db::update` / `db::delete` also publish a transient `aether` /
`data.changed` UI event so open pages for that model can refresh (see [events](events.md)).

## Faceted list UI

List pages build a kernel filter in the browser (chips → filter JSON) and send it to the plugin
`source` as `filter`, plus optional `q` / `search_fields` / `group_by`. The plugin should AND
`filter` into its query (or pass it through to `db::search`). Per-user saved searches live in core
as `search_favorites` (`GET`/`POST` `/api/ui/search/favorites`, `DELETE` `/api/ui/search/favorites/{id}`).
