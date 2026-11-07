---
title: "Chapter 5: Permissions"
description: "Declare a role, write a rule that only leads may delete a task, and see what a member without the role gets."
section: plugin
status: stable
order: 17
---
# Chapter 5: Permissions

**Goal:** only people with a `lead` role can delete a task; everyone else can still read, add and edit.

Until now everyone could do everything. Three layers decide who may do what, and each does a different job:

| Layer | Question it answers | Where |
|---|---|---|
| `access_models` | May this *plugin* touch this model at all? | `plugin.toml` (chapter 2) |
| A **role** | May this *person* open pages and do things? | `[[roles]]` in `plugin.toml`, given by an administrator |
| A **rule** | May this person do *this* to *this* record? | `rules/<model>.json` |

## 1. Declare a role

Add this to `plugin.toml`, below the `[app]` table:

<!-- file: ch05/tasks/plugin.toml#18-21 -->
```toml
[[roles]]
name = "lead"
label = "Task lead"
description = "May delete tasks, reopen finished ones and change them."
```

A role is only a name that an administrator can give to people. Declaring it does nothing by itself.

## 2. Write the rule

Create `plugins/tasks/rules/task.json`:

<!-- file: ch05/tasks/rules/task.json -->
```json
{
  "model": "task",
  "access": [
    {
      "name": "leads_delete",
      "roles": [
        "lead"
      ],
      "operations": [
        "delete"
      ]
    }
  ]
}
```

An `access` row *grants*. This one says: the `lead` role may `delete`. The rule has no row for `read`, `create` or
`write`, and **a model with no grant for an operation is open for it**, so everyone keeps those. The moment a model has a
grant for an operation, only the people a grant applies to may do it. So this single row makes deleting a leads-only
action and leaves everything else alone. See [Record and field rules](../../architecture/rules.md).

The pages from chapter 3 already say `roles="member"`, which is why a person with no role can still open them. A page
without `roles` would only open for people who hold some role of the plugin once the plugin declares roles.

## 3. Try it as a member

Create two people:

```sh
aether -c aether.toml --create-user --username bob --email bob@example.com --password 'choose-one' --org-db-name main
aether -c aether.toml --create-user --username lee --email lee@example.com --password 'choose-one' --org-db-name main
aether -c aether.toml --grant-role tasks.lead --username lee --org main
```

Upgrade the plugin (bump `version` to `0.5.0`, load, upgrade), then sign in as **bob** in a private browser window:

1. Open Tasks. Bob sees the list and can add and edit tasks.
2. Select **Delete** on a row. Bob is refused: *not allowed: you may not delete `task` records here*.

Sign in as **lee** and delete the same task: it works.

> **Why not as `admin`?** The administrator is exempt from every rule, by design, so deleting would always work. Test
> permissions with an ordinary account. Remember this for every chapter that follows.

## What you learned

- A plugin declares roles; an administrator gives them to people with `--grant-role plugin.role`.
- A rule grants an operation to roles. Leaving an operation out leaves it open.
- `roles="member"` on a page opens it to everyone signed in.
- The administrator is not held to rules, so test as someone else.

> **Stuck?**
> - *403 on a page*: add `roles="member"` (or the role the page needs).
> - *bob can still delete*: the plugin was not upgraded, or `rules/task.json` is not in the plugin folder.
> - *`--grant-role` says unknown role*: the role must be declared in the installed version, so upgrade first.

Previous: [Chapter 4](04-detail-page.md). Next: [Chapter 6, A rule](06-rule.md).
