# Queries: filters, counts and aggregates

Plugins never write SurrealQL. `db::find`, `db::count` and `db::aggregate` take a **filter**, and
`data_model/query.rs` is the only place that turns it into a `WHERE` clause. Field names are looked up
in the model (and replaced by their stored ids); values are always bound, never spliced in.

```json
{ "status": "active", "salary": { "gte": 1000, "lt": 5000 },
  "or": [ { "dept": "hr" }, { "dept": { "in": ["it", "ops"] } } ],
  "not": { "manager": { "null": true } } }
```

* `"field": value` is an equality; `"field": { op: value, … }` takes `eq ne gt gte lt lte in nin like null`.
  `like` is a case-insensitive substring. `null: true` means the field is empty, `false` that it has a value.
  `nin` is also true for a record where the field is empty.
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

All three need `db::query`, are limited to the plugin's granted models and write a `data_access` row
(an aggregate lists no record ids). The Rust SDK has `db::Filter`, `Find::matching`, `Find::count`,
`db::count` and `db::aggregate`.
