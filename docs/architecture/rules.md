# Record and field rules

Who may see and change which records and fields is data a plugin ships, in `rules/<model>.json`
(one file per model, loaded and checked with the plugin like `models/`). The kernel applies it to
every database command, so a plugin cannot forget a check and a refused write changes nothing.

Designed after Odoo 19's merged `ir.access` (permissions OR, restrictions AND, state-dependent
domains) and Frappe's permlevel and approver sharing, with one definition serving both lists
(a filter) and single records (a check in the write's own transaction). ERPNext keeps those two as
separate functions per rule and has to keep them in sync.

```json
{
  "model": "leave_request",
  "access": [
    { "name": "own", "operations": ["read", "create", "write", "delete"], "when": { "employee": "$hr.employee" } },
    { "name": "approvers", "roles": ["approver"], "operations": ["read", "write"],
      "when": { "employee": { "in": "$hr.subordinates" } } },
    { "name": "hr", "roles": ["hr.hr_manager"], "operations": ["read", "write"] }
  ],
  "restrict": [
    { "name": "pending_only", "operations": ["write", "delete"], "exempt_roles": ["hr.hr_manager"],
      "when": { "state": "submitted" } }
  ],
  "fields": [
    { "fields": ["state", "decided_by"], "write_roles": ["approver", "hr.hr_manager"] },
    { "fields": ["notes"], "read_roles": ["hr.hr_manager"] }
  ]
}
```

* **`access` rows grant.** For an operation (`read`, `create`, `write`, `delete`) a person gets the
  records matching *any* grant that applies to them (`roles` empty: every member; otherwise someone
  holding one of the roles). No `when`: every record. A model with no grant for an operation is open
  for it; once it has one, someone none of its grants applies to gets nothing.
* **`restrict` rows narrow.** The record must also match every restriction that applies, except for
  people holding an `exempt_roles` role. This is how "the owner may only change it while pending" is said.
* **`fields`** limit who may read or set named fields. Reading hides them (and searching or summarising
  by them is refused, since that would show what they hold); setting them is refused. Unlisted fields are open.
* **Roles** are `<plugin>.<role>`; a bare name means a role of the plugin that owns the file. Roles are
  declared by plugins (`[[roles]]`) and given by an administrator (`aether --grant-role`).
* **`via:<plugin>` and `via:*`** are held by a call that came through another plugin's function (the
  kernel adds them to the roles a rule sees; a client calling the function directly never has them).
  A plugin lists `via:*` on a grant to trust the plugins that call it, which do their own checks of the
  person: `hr`'s hire holds are placed and released by recruitment and onboarding, whose users need
  not be HR staff.
* **Not restricted:** `org_admin` (the organization's administrators), the kernel's own jobs, and the
  `rule_var_*` functions below.

## Variables

In a `when`, a string starting with `$` is a variable (`$$` writes a literal `$`).

* `$user` is the caller's account (`users:abc`).
* `$<plugin>.<name>` (or `$<name>` for the plugin's own) is the answer of that plugin's function
  `rule_var_<name>` for the caller: their employee record, the ids of their team, their companies. The
  plugin must be a dependency. The answer is worked out once per request and shared by the plugins it calls.
  A `rule_var_*` function runs without rules (it would otherwise need the rules it feeds), so it must only
  read, and only answer about the caller.
* A variable with no value (the caller is not an employee) makes its comparison match nothing. A list
  stands for "one of", so `{"employee": "$hr.subordinates"}` and `{"employee": {"in": ...}}` mean the same.

## Workflow: states and the moves between them

A `workflow` section in the same rule file says how a `select` field (the state) may change. It is built from the
same parts as `restrict`: who (`roles`), what the record must look like before the move (`when`, with the same
`$variables`), and the same exemption for administrators.

```json
"workflow": {
  "field": "state",
  "initial": ["draft"],
  "transitions": [
    { "name": "submit",  "label": "Submit", "from": ["draft"],     "to": "submitted", "when": { "employee": "$user" } },
    { "name": "approve", "from": ["submitted"], "to": "approved", "roles": ["approver"],
      "when": { "employee": { "ne": "$user" } } },
    { "name": "reject",  "from": ["submitted"], "to": "rejected", "roles": ["approver"] },
    { "name": "reopen",  "from": ["rejected"],  "to": "draft" }
  ]
}
```

* **A write that sets the state field is a move.** It is refused unless a transition the caller may make (`roles`;
  none listed: anyone who may change the record) leads to the new state from the state the record is in now and the
  transition's `when` holds. The check runs in the write's own transaction, before the change, so nothing skips a step
  and a refused move changes nothing. Setting the state it already has is not a move.
* **A new record** may only be created in one of `initial` (default: the field's `default`).
* **Other writes** are not affected: say who may change the other fields of a record in a state with `restrict`
  (`"when": { "state": "draft" }`), exactly as before. Field rules can still limit who writes the state field at all.
* **Administrators** (`org_admin`) and the kernel's own jobs are not held to the workflow, like any other rule.
* **History:** mark the state field `"track": true` on a model with chatter on, and every move is written to the
  record's chatter with who made it and when.
* **`db::transitions`** (`{ "model": …, "id": … }`, needs `db::query`) lists the moves the caller can make with that
  record right now, `[{ name, label, from, to }]`: empty when none, or when they cannot read and change the record. A
  page uses it to show only the buttons that will work; the move is checked again when it is made. In Rhai:
  `db::transitions(model, id)`; in Rust: `db::transitions`.
* The field, every state and every `when` are checked against the model when the plugin loads.

## What each command does

| Command | Effect |
|---|---|
| `db::get`, `find`, `count`, `aggregate`, `tree`, `related` | the read condition is added to the query; a record that may not be read is as if absent |
| `db::create` | the new record must match the create condition, checked in the same transaction (otherwise undone) |
| `db::update`, `increment`, `relate`, `unrelate` | the record must match the write condition before the change |
| `db::delete` | the record must match the delete condition |
| `db::transaction` | each write is checked as above, all in the one transaction |

A refused write fails with `not allowed: …`; the SDK passes it on to the caller as the call's message.
