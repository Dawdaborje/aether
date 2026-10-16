---
title: "Automation: rules that act, assignment, repeats, notifications, approvals"
description: "A business wants to say \"when a deal is won, notify finance and create an invoice draft\", \"assign new tickets in turn to the support team\", \"repeat this invoice every month\", \"a purchase over 5 000…"
section: architecture
status: proposal
order: 70
---
# Automation: rules that act, assignment, repeats, notifications, approvals (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

A business wants to say "when a deal is won, notify finance and create an invoice draft", "assign new tickets in turn
to the support team", "repeat this invoice every month", "a purchase over 5 000 needs two approvals". Today each
plugin writes this in code with [events](events.md) and the [scheduler](scheduler.md). This page makes it **data an
administrator can read, change, test and switch off**, with the same safety as the rest of the kernel.

## One file per model

`automations/<model>.json` in the plugin (organizations can add their own through [Customization](customization.md)):

```json
{
  "model": "ticket",
  "automations": [
    { "name": "assign_round_robin",
      "on": { "create": true },
      "when": { "assignee": { "null": true } },
      "do": [ { "assign": { "field": "assignee", "from": "role:helpdesk.agent", "strategy": "round_robin" } } ] },
    { "name": "escalate_overdue",
      "on": { "schedule": { "every": "15m" } },
      "when": { "status": { "ne": "closed" }, "due": { "lt": "$now" } },
      "do": [ { "notify": { "to": "$record.team.lead", "template": "overdue" } },
              { "set": { "priority": "high" } } ] }
  ]
}
```

## Triggers (`on`)

`create`, `update` (optionally `fields: [..]` or `to: { state: "won" }`), `delete`, a workflow `transition`, an
`event` from another plugin (`crm.lead_won`), `schedule` (cron or `every`, evaluated over matching records), and
`manual` (a button). Triggers come from the write transaction's outbox, not from polling, so none is missed and none
runs twice.

## Conditions (`when`)

The filter language of [Queries](queries.md), with `$record`, `$old` (value before the change), `$now`, `$actor`. One
language for rules, queries, automations and views.

## Actions (`do`)

| Action | Does |
|---|---|
| `set` | change fields on the record (goes through the normal write: checks, rules, workflow) |
| `create` | create a record in any model the plugin may write |
| `call` | call a function of the plugin (`function`, `payload`) |
| `notify` | in-app, email or SMS using a [template](#templates) |
| `assign` | pick a user: `round_robin`, `least_loaded` (fewest open records matching a filter), `by_field` |
| `webhook` | send a [webhook](api_and_webhooks.md) |
| `approve` | start an approval (below) |
| `repeat` | schedule a copy of this record (below) |

Each action runs as a scheduler job under the **plugin's grants and the automation's declared acting role**, never
silently as an administrator. Every run is recorded.

## Assignment

`round_robin` and `least_loaded` keep a small counter per (automation, pool) so concurrent creates are fair; a pool
can respect availability (a user's out-of-office flag or a [calendar](../plugin/pages.md) entry). If nobody
qualifies the record is left unassigned and an alert is raised, not skipped.

## Repeats (auto-repeat)

```json
{ "repeat": { "every": "1 month", "on_day": 1, "until": "2027-12-31", "copy": ["customer","lines"], "reset": { "status": "draft" } } }
```

Creates the next record from the current one on schedule, through the same write path, with a `repeat_of` link. Pausing
or ending is one flag; a missed run is caught up or skipped by a setting (`catch_up`, as the scheduler does).

## Approvals

Approvals extend [workflow](rules.md#workflow-states-and-the-moves-between-them) instead of replacing it. A transition may
require an approval step:

```json
{ "name": "approve", "from": ["submitted"], "to": "approved",
  "approval": { "approvers": [ { "role": "finance.manager" }, { "field": "owner.manager" } ],
                "mode": "all", "min": 2, "timeout": "3d", "on_timeout": "escalate" } }
```

The record stays in `submitted` with an open **approval request** until the quorum answers; the last approval performs the
transition inside its own transaction, with all the usual guards. Approvers get a notification and a task list; they
can approve, reject (with a reason) or delegate. Everything is in the record's chatter and the audit. Conditions
choose the chain (`when: { amount: { gt: 5000 } }` adds a second step). An approver cannot approve what they created
(separation of duties, on by default).

## Templates

Message templates are data: `templates/<name>.json` with per-language subject and body, using the same placeholders as
[print templates](print.md) (`{{record.name}}`, `{{actor.name}}`, formatted per the recipient's locale). The mail
and SMS go through `communication::send`, the in-app one through [notifications](notifications.md).

## Safety and testing

- **Loops**: an automation's own write can trigger others up to a depth of 4 (as events do); a run that would repeat
  the same (automation, record, state) in one chain is stopped and logged.
- **Limits**: records per run, actions per hour per organization, per-run time.
- **Dry run**: `automation::simulate { name, record_or_filter }` shows which records match and what each action would do,
  and executes nothing. [Verification](verification.md) uses it for change previews.
- **Static checks**: a condition naming a missing field, a `set` that the workflow forbids, or an unreachable
  automation is refused when the plugin loads.
- **Run log**: `automation_runs` with trigger, record, outcome, duration; failures retry like jobs and then notify the owner.
- Administrators can switch an automation off per organization without editing the plugin.

## Commands

| Command | Capability | Does |
|---|---|---|
| `automation::list` / `runs` | `automation::read` | definitions and recent runs |
| `automation::run { name, record }` | `automation::run` | trigger a `manual` one |
| `automation::simulate` | `automation::read` | dry run |
| `automation::enable` / `disable` | `automation::manage` | per organization |
| `approval::pending` / `answer` / `delegate` | `approval::answer` | approver side |

## Settings and tables

Settings: `automation.max_depth`, `automation.max_per_hour`, `automation.run_keep_days`. Tables: `automation_state`
(enabled, counters), `automation_runs`, `approval_requests`, `approval_answers`, `repeat_series`.

## Not in scope

A drag-and-drop flow designer (the JSON is the model; a designer can be a Studio page over it later), arbitrary loops
or variables (this is not a programming language; use a plugin function via `call`), cross-organization automation
([Federation](federation.md)).

## Build order

1. Outbox for triggers, `set`, `notify`, `call`, run log, dry run.
2. Schedule triggers and `assign`.
3. Templates.
4. Approvals.
5. Repeat.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Who do actions act as? | **The plugin under a declared role**, never an implicit administrator. |
| 2 | Are approvals a separate engine? | **No**, an extension of workflow transitions. |
| 3 | Triggers: events or polling? | **Write outbox**, so none is lost or doubled. |
| 4 | Can an approver approve their own record? | **No**, by default. |
| 5 | Visual designer in the first release | **No**; JSON first. |
