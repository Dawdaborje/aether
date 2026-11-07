---
title: "Customization: overlays and hooks"
description: "Every company wants \"one more field on the customer\" or \"this column first\". Today that means editing the plugin, which breaks on the next upgrade."
section: architecture
status: proposal
order: 120
---
# Customization: overlays and hooks (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

Every company wants "one more field on the customer" or "this column first". Today that means editing the plugin, which
breaks on the next upgrade. This page lets an organization **overlay** a plugin's definitions without touching the
plugin, and lets it attach small **hooks** that run sandboxed. It is possible cleanly here because every model and field
has a stable id ([Models](models.md)) and the kernel already treats definitions as data.

## Overlay

An organization stores its changes in `overlays`, a set of operations against a plugin's model, page or rule file,
addressed by stable id:

```json
{ "plugin": "crm", "model": "mdl_4fs4ygtivr", "ops": [
  { "add_field": { "name": "x_region", "label": "Region", "type": "select", "options": ["north","south"] } },
  { "set_label": { "field": "fld_5v6ldprbyc", "label": "Stage" } },
  { "hide": { "field": "fld_prefhbkkqr" } },
  { "list_columns": ["title","x_region","status"] }
]}
```

What an overlay may do: add fields (names carry an `x_` prefix so they never clash with a later plugin field), relabel,
hide a field from pages, change list columns, form layout and defaults, add a `select` option, add a simple validation
(`checks`), add a view filter, add an [automation](automation.md) or [print](print.md) override, add a report.

What it may **not** do: remove or retype a plugin field, loosen a rule, remove a `check`, add a capability, change the
workflow's states, or touch another organization. A hide is a presentation change only; access is decided by
[Rules](rules.md) and an overlay cannot widen it.

### Stored and applied

The kernel merges base definitions and the organization's overlay when it builds the runtime model (`data_model/runtime.rs`,
`apply.rs::plan`): an added field becomes a new column named by its own `fld_` id in that organization's table, and
organizations on different overlays share the plugin code, as organizations on different plugin versions already do.
Because overlay operations name ids, a plugin upgrade that renames or reorders fields keeps them working. If an upgrade
removes a field the overlay depends on, the upgrade [pre-flight](verification.md#3-upgrade-pre-flight) reports which
organization overlays are affected.

Each overlay change is a version with an author, a reason and a diff; it can be reverted, previewed on a copy before it
applies ([Verification](verification.md)), and is audited.

### Editing

A Studio page (the Models editor already exists) writes overlays instead of editing the plugin when the user is an
organization administrator rather than a developer. Developers keep editing plugin files.

## Hooks

For logic the declarative layers do not cover, an organization may attach a **hook**: a short Rhai or Lua script (the
sandboxed languages the kernel already hosts, `plugin_manager/script/`) bound to a model event.

```rhai
// hook: before_save on quote
if record.discount > 30 && !actor.has_role("sales.manager") {
    fail("Discounts above 30% need a manager");
}
```

- They run in the same sandbox as script plugins: no files, no network, only the commands their capabilities allow, with
  fuel and time limits.
- They run **as the saving user**, never with extra rights. A hook can refuse a write or compute a value; it cannot
  grant itself access.
- An organization hook declares its capabilities, which an administrator reviews like a plugin's; the default set is
  read-only on the same model.
- Hooks are checked on save ([Verification](verification.md)): syntax, unknown fields, capability use; and run in
  `automation::simulate` for previews.
- Versioned, enabled per organization, with a run log and error counts that disable a failing hook automatically.

## Commands

| Command | Capability | Does |
|---|---|---|
| `overlay::get { plugin, model }` | `overlay::read` | the organization's overlay |
| `overlay::apply { ops, reason }` | `overlay::manage` | validate, version, apply |
| `overlay::revert { version }` | `overlay::manage` | undo a change |
| `hook::save` / `enable` / `runs` | `hook::manage` | hook CRUD and log |

## Settings and tables

Settings: `overlay.enabled`, `hook.enabled`, `hook.max_per_model`, `hook.fuel`, `hook.error_limit`.
Tables: `overlays` (versioned), `hooks` (versioned), `hook_runs`.

## Not in scope

Client-side scripts in the browser, replacing a plugin's page wholesale (copy the plugin instead), overlays that
alter another plugin's behavior across organizations, a marketplace of overlays (the store handles plugins).

## Build order

1. Overlay read/merge for labels, hidden, columns (no schema change).
2. Added fields and `plan` integration.
3. Revert, versions, preview.
4. Hooks (`before_save`, `after_save`, computed) with logs.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Address overlay targets by name or id | **Id**, so upgrades never break them. |
| 2 | Can an overlay widen access? | **Never.** |
| 3 | Added field names | **`x_` prefix**, reserved for organizations. |
| 4 | Whose rights does a hook run with? | **The saving user's.** |
| 5 | Hook language | **Rhai and Lua**, already sandboxed. |
