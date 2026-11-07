---
title: "Verification: static checks, previews and upgrade safety"
description: "Rules, workflows, automations and model changes decide who can see money and what a company's data looks like. Today the kernel checks each file for being well-formed when a plugin loads."
section: architecture
status: proposal
order: 430
---
# Verification: static checks, previews and upgrade safety (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

Rules, workflows, automations and model changes decide who can see money and what a company's data looks like. Today
the kernel checks each file for being well-formed when a plugin loads. It cannot say whether the *whole* is right:
a state nobody can reach, two rules that contradict, a role that can read a salary through a side door, an upgrade that
fails halfway on real data. This page designs three tools that answer those questions **before** anything changes.

## 1. Static checks: `aether check`

```
aether check [--plugin crm] [--org acme] [--json]
```

Runs over the loaded definitions (models, rules, workflows, automations, pages, shares) with no database writes.

| Check | Finds |
|---|---|
| Workflow reachability | states no path reaches from `initial`; states with no way out that are not final; transitions whose `when` can never be true given the model's checks |
| Rule consistency | `restrict` rules that cancel an `access` rule for every role; overlapping rules with different outcomes; roles named but never defined |
| **Access matrix** | for every (role, model, field): can it read, create, write, delete, and *through which rule, share or workflow move*. Also lists fields readable by **everyone** and fields that **no one** can write |
| Query hygiene | filters that cannot use an index on a large model, unbounded lists, `like` on encrypted fields |
| Automation safety | conditions on fields that do not exist, `set` actions that the workflow forbids, possible loops |
| Page contracts | a page that calls a function the viewing role cannot call; a form field a role cannot write |
| Capability use | capabilities declared but unused, or used but not declared |

Output has a severity (`error` blocks load or publish, `warn` is shown, `info` is the matrix). The checker is written
as a library (`facets/verify`) over the same parsed structures the kernel enforces (`data_model/rules.rs`,
`definition.rs`), so a check cannot disagree with enforcement. Checks that need to reason about filters (can this
`when` ever be true? do these two overlap?) use a small solver over the filter language: comparison, `in`, `null`,
`and/or/not`, field-to-field comparison, bounded depth. When the solver cannot decide it says `unknown`, never
guesses.

The access matrix is also a page in the desk, and a diff of two matrices is the first thing a reviewer reads when a
rule changes: *"after this change, `hr.intern` can read `salary` of own team"*.

## 2. Previews: what-if on a copy

A **change set** is anything that could change behavior: a plugin upgrade, edited rules, an automation, an
[overlay](customization.md), a bulk import.

```
aether preview --org acme --change plugin:crm@1.4 [--sample 10000] [--report prism.sales_by_month]
```

1. **Branch** the organization's database: an in-memory/temporary copy (SurrealDB export into a scratch namespace, or a
   sample of N rows per model with all rows a rule or link needs).
2. **Apply** the change set to the copy: DDL plan, migrations, rule changes, new automation definitions.
3. **Compare**: access matrix before/after; for chosen reports, the same query on both; for named record sets, which
   records change visibility; for automations, which records they would match and what they would do (via `simulate`);
   schema effects (columns added, constraints that existing rows violate).
4. **Report** with a verdict and a diff. Nothing touches the real organization; the copy is destroyed.

The same engine powers the desk's **"Preview before apply"** button on rule edits and the dry run of an import.
Previews run as a low-priority job in a separate process so a large copy cannot slow the server.

## 3. Upgrade pre-flight

`upgrade_plugins` (`facets/core/src/plugin_manager/catalog.rs`) already plans the DDL with `data_model/apply.rs::plan`.
Pre-flight adds, before applying:

1. **Compatibility**: dependency versions, capability changes (new capabilities are shown for approval, as for install),
   removed commands that other plugins call, changed event names.
2. **Data check**: for every new `required`, `unique`, `check` or type change, count the existing rows that would
   violate it, with sample ids. The upgrade is blocked, or offers the plugin's migration command (`on_upgrade`) first.
3. **Rollback point**: with `history` on ([History](history.md)) a restore point is enough; otherwise a snapshot of the
   affected tables is taken (export to storage) so `aether plugin rollback` can restore the previous revision and
   data. Plugin file revisions already exist (`plugin_manager/revisions.rs`).
4. **Canary**: optionally apply to one organization first, run the plugin's `healthcheck` command, then continue.

A plugin states the compatibility it guarantees in `plugin.toml`:

```toml
[upgrade]
from = ">=1.2"          # versions it can upgrade from without manual steps
reversible = true       # a down-migration exists
healthcheck = "crm.healthcheck"
```

## Commands

CLI first (`aether check`, `aether preview`, `aether plugin preflight`), desk pages second. Plugin-visible:
`verify::matrix { model }` (capability `verify::read`), so an admin plugin can show it.

## Settings and tables

Settings: `verify.block_on_error` (default on at publish and upgrade), `verify.preview_sample`, `verify.snapshot_keep_days`.
Tables: `preview_runs` (change set, verdict, diff summary), `upgrade_snapshots`.

## Not in scope

Proving the *plugin code* correct (WASM code is isolated by capabilities, not verified), formal proofs of the whole
kernel, checking third-party bridges' behavior.

## Build order

1. `facets/verify` with workflow reachability and the access matrix (read-only, no database).
2. Rule consistency with the filter solver.
3. Upgrade data check and rollback point.
4. Preview on a copy; report diff.
5. Automation and page checks; canary.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Do `error` findings block publishing/upgrading? | **Yes** by default; an owner may override with a recorded reason. |
| 2 | When the solver cannot decide | **Report `unknown`**, never pass silently. |
| 3 | Copy for previews | **Sample-based** by default, full copy by flag. |
| 4 | Where do checks live | **A library crate sharing the enforcement types**, not a second parser. |
| 5 | Is a rollback snapshot automatic? | **Yes** when the upgrade changes data or constraints. |
