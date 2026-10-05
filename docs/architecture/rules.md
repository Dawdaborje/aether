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

## What each command does

| Command | Effect |
|---|---|
| `db::get`, `find`, `count`, `aggregate`, `tree`, `related` | the read condition is added to the query; a record that may not be read is as if absent |
| `db::create` | the new record must match the create condition, checked in the same transaction (otherwise undone) |
| `db::update`, `increment`, `relate`, `unrelate` | the record must match the write condition before the change |
| `db::delete` | the record must match the delete condition |
| `db::transaction` | each write is checked as above, all in the one transaction |

A refused write fails with `not allowed: …`; the SDK passes it on to the caller as the call's message.
