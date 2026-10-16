---
title: "Closing the framework gaps: roadmap"
description: "This page is the index of the design proposals below. Each linked page marks its open choices Decide; the last section here collects the cross-cutting ones."
section: architecture
status: proposal
order: 190
---
# Closing the framework gaps: roadmap (proposal)

**Status: proposal, nothing built.** This page is the index of the design proposals below. Each linked page
marks its open choices *Decide*; the last section here collects the cross-cutting ones.

**Decisions:** the proposed answers in every *Decide* table have been accepted as the working choices. Building
starts with [History](history.md). A choice is reopened only if the build shows it does not hold (for example the
audit chain throughput measurement in [Audit chain](audit_chain.md)).

Two groups of work are covered:

1. **Gaps against other ERP frameworks**: integration, administration and output features a business
   expects (webhooks, an importer, inbound mail, second factor, sharing, automation, print formats, more views,
   customization).
2. **Things no framework of this kind offers**: verifiable history, tamper-evident audit, customer-held
   encryption keys, static checking of rules, previews of a change, safe upgrades, governed exchange between
   organizations, and later offline sync.

## Three rules every proposal follows

1. **Declarative data, not code.** A plugin ships JSON or XML (like `models/` and `rules/`); the kernel
   validates it when the plugin loads and enforces it. No feature adds a place where a plugin runs
   unchecked code in the server.
2. **One filter language, one audit transaction.** Conditions are the filters of [Queries](queries.md).
   Writes are audited in the same transaction as in [Access](access.md). No feature gets its own query dialect or
   its own permission check.
3. **Dry-runnable.** An import, an automation, a rule change or an upgrade can be previewed before it takes
   effect ([Verification](verification.md)).

## The shared substrate

Most of the second group stands on one piece: a **record version store** ([History](history.md)), written in
the same transaction as the audit row. It serves point-in-time reads, undo, restore, the baseline for previews,
the anchors of the [audit chain](audit_chain.md), and later offline sync. It is designed once.

## The proposals

| Doc | Closes |
|---|---|
| [History](history.md) | point-in-time reads, undo, restore, later offline sync |
| [Audit chain](audit_chain.md) | tamper-evident audit, `--audit-verify` |
| [API and webhooks](api_and_webhooks.md) | API tokens, record REST + OpenAPI, outbound and inbound webhooks |
| [Import and export](import_export.md) | CSV/XLSX importer with dry run, exports |
| [Inbound mail](inbound_mail.md) | email into records |
| [Auth hardening](auth_hardening.md) | TOTP, passkeys, SAML, per-organization sign-in policy |
| [Field encryption](field_encryption.md) | per-field encryption with customer-held keys |
| [Sharing](sharing.md) | share a record with a person or a link, with expiry |
| [Automation](automation.md) | when-this-then-that rules, assignment, auto-repeat, notifications, approvals |
| [Verification](verification.md) | static checks, previews, upgrade pre-flight |
| [Print](print.md) | print templates and PDF |
| [Views and reports](views_reports.md) | pivot, graph, gantt, map, tree; saved reports |
| [Customization](customization.md) | per-organization fields, layouts and hooks |
| [Federation](federation.md) | signed, consented exchange between organizations |

## Build order

1. **Substrate**: History, then Audit chain.
2. **Integration edge**: API and webhooks, Import and export, Inbound mail.
3. **Trust**: Auth hardening, Field encryption, Sharing.
4. **Logic**: Automation, Verification.
5. **Output**: Print, Views and reports, Customization.
6. **Reach**: Federation, then offline sync.

Why this order: 2 to 6 each need either the version store (undo, previews, restore of an import) or the job
and callback pattern that already exists for [OCR](ocr.md) and [AI](ai.md). Doing the substrate first means
nothing is built twice.

## Not in scope (here)

A website and portal builder, live chat and telephony, push notifications, privacy-request tooling,
multi-company and analytic accounting (plugin work, not kernel), shipping and bank connectors (bridges, added
one at a time as in [Bridges](../bridges.md)), a mobile app.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Build the version store before any other proposal? | **Yes.** Undo, previews and import rollback depend on it. |
| 2 | Is history on or off by default for a model? | **Off**; a model opts in with `"history": true`, like `chatter`. Cost is only paid where asked. |
| 3 | One new capability group per proposal, or one shared? | **One group each** (`history::`, `import::`, `automation::`...), as `ocr::` and `ai::` do. |
| 4 | Do new CLI flags go on the single `Args` struct or become subcommands? | **Subcommands** for the new families (`aether audit verify`, `aether check`, `aether import`), keeping the old flags working. |
| 5 | Ship docs as *proposal* until the code lands? | **Yes**; flip the status line when built, and add the commands to the plugin commands table with `done`. |
