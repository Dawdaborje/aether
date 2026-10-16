---
title: "More views and saved reports"
description: "Pages have list, form, kanban, dashboard, board and calendar views. tree and pivot fall back to a plain list, and gantt, graph and map do not exist."
section: architecture
status: proposal
order: 440
---
# More views and saved reports (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

[Pages](../plugin/pages.md) have list, form, kanban, dashboard, board and calendar views. `tree` and `pivot` fall back to a
plain list, and `gantt`, `graph` and `map` do not exist. The [report](../plugin/pages.md) facet answers
`report::query` (dimensions, measures, periods) but there is no way for a user to save a report they built. This page adds
the views and the saved report, on the existing contract.

## The contract stays the same

A view gets its data from a plugin function named in `source`, and acts through functions named in attributes (`move`,
`open`), exactly as `calendar` and `board` do. A new view therefore adds no new data path and cannot bypass rules.
Compiling a view type is generic in `plugin_manager/pages.rs`; the work is the widget and its hints.

## The views

| View | Source returns | Interactions | Notes |
|---|---|---|---|
| `tree` | rows with `parent` | expand/collapse, drag to re-parent (`move`) | uses the `under` filter to load a branch lazily |
| `pivot` | `report::query` result | choose rows, columns, measure; drill into a cell | reuses `PivotCardView.svelte` from the board |
| `graph` | `report::query` result | bar, line, area, pie, stacked; click to drill | reuses the board's chart types |
| `gantt` | `{ id, label, start, end, parent?, depends_on[], progress }` | drag to move/resize (`move`), dependency arrows | one scale at a time: day, week, month |
| `map` | `{ id, label, lat, lon, group? }` | pan, zoom, cluster, click to open | tiles from a map bridge (`open_street_map` exists); no tile key in the browser |

Frontend work per view follows the steps already used for calendar: the `ViewType` union and `knownTypes` set in
`web/src/lib/dsl/{types,buildPage}.ts`, a widget under `web/src/lib/components/widgets/`, a branch in
`ViewRouter.svelte`, a theme part name in [Themes](themes.md), and an entry in the widget-status section of the pages doc.

Every view works with the keyboard, shows a text alternative for charts (a table toggle), and respects the theme tokens.

## Saved reports

A user builds a report (pick a model, dimensions, measures, filter, period, chart) in a builder page and **saves** it:

```
saved_reports: owner, title, model, query (the report::query JSON), view (pivot|graph|table), filter, visibility
```

- The saved object is only the *query*; running it always goes through `report::query` as the viewer, so a shared report
  shows each person only what they may see.
- Visibility: private, a role, or the whole organization; the owner can pin it to a dashboard board.
- Schedule and send: a saved report can be emailed as XLSX/PDF (`report::export`, [Print](print.md)) on a cron, a built-in
  [automation](automation.md) action.
- **Snapshots**: a report can be frozen on a schedule (`report_snapshots`: the result rows and when), which is how
  "revenue as it was reported on the 5th" is answered without rerunning old data (time analysis of live data is limited, see
  [History](history.md)).

Builder safety: it can only choose fields the user can read; limits (`MAX_GROUPS` = 10 000 in `facets/report/src/spec.rs`)
apply; heavy queries are queued as jobs with a progress page.

## Commands

| Command | Capability | Does |
|---|---|---|
| `report::save` / `delete` / `list` | `report::save` | saved report CRUD |
| `report::run { saved }` | `report::query` | run as the caller |
| `report::snapshot { saved }` | `report::query` | store the current result |

## Settings and tables

Settings: `report.snapshot_keep_days`, `report.max_saved_per_user`. Tables: `saved_reports`, `report_snapshots`.

## Not in scope

Free-form SQL or query editors, a spreadsheet engine in the browser, 3D or geospatial analysis beyond points.

## Build order

1. `pivot` and `graph` (data path exists; widgets and hints).
2. `tree`.
3. Saved reports and snapshots.
4. `gantt`.
5. `map` with the tile bridge.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | New data paths for the views? | **No**, `source` functions only. |
| 2 | What does a saved report store? | **The query only**; results are always computed as the viewer. |
| 3 | Map tiles | **Served by the kernel through a bridge**, no third-party key in pages. |
| 4 | Order of delivery | **pivot and graph first**; most value, least new code. |
