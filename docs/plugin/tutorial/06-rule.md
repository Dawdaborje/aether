---
title: "Chapter 6: A rule"
description: "Give a task a workflow, open to doing to done, lock a finished task, and add a status bar."
section: plugin
status: stable
order: 18
---
# Chapter 6: A rule

**Goal:** a task moves `open → doing → done` and no other way, and a finished task cannot be changed.

Rules are data, not code: you describe what is allowed and the kernel enforces it on every path, the web app, the
command line and your own functions. A refused write changes nothing.

## 1. Two more parts in the same rule file

Replace `rules/task.json` with this:

<!-- file: ch06/tasks/rules/task.json -->
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
  ],
  "restrict": [
    {
      "name": "done_is_final",
      "operations": [
        "write",
        "delete"
      ],
      "exempt_roles": [
        "lead"
      ],
      "when": {
        "status": {
          "ne": "done"
        }
      }
    }
  ],
  "workflow": {
    "field": "status",
    "initial": [
      "open"
    ],
    "transitions": [
      {
        "name": "start",
        "label": "Start",
        "from": [
          "open"
        ],
        "to": "doing"
      },
      {
        "name": "finish",
        "label": "Finish",
        "from": [
          "doing"
        ],
        "to": "done"
      },
      {
        "name": "reopen",
        "label": "Reopen",
        "from": [
          "done"
        ],
        "to": "open",
        "roles": [
          "lead"
        ]
      }
    ]
  }
}
```

- **`workflow`** says how the `status` field may change. A write that changes it is a *move*, and it is refused unless
  a transition leads from the task's current state to the new one. `start` goes from `open` to `doing`, `finish` from
  `doing` to `done`. `reopen` is for leads only. New tasks may only be created `open` (`initial`).
- **`restrict`** *narrows* what is granted: a task may only be changed or deleted while its status is not `done`
  (`"when": {"status": {"ne": "done"}}`), except for leads (`exempt_roles`). A finished task is final.

Grants combine with OR, restrictions with AND: you may do something if *a* grant says so *and every* restriction
agrees.

## 2. A status bar

The detail page can show the status as steps and let people click the next one. Add one line to the form in
`pages/task.xml`, above the fields:

<!-- file: ch06/tasks/pages/task.xml -->
```xml
<page route="/tasks/{id}" title="Task" model="task" roles="member">
  <view type="form" source="get_task" function="update_task" submit="Save" success="Saved">
    <statusbar field="status" function="update_task"/>
    <field name="title" label="Title" required="true"/>
    <field name="details" label="Details" fieldType="text"/>
    <field name="due" label="Due date" fieldType="date"/>
    <field name="assignee" label="Assignee"/>
  </view>
</page>
```

The steps come from the model's `select` options. Clicking one calls `update_task` with the new `status`, so the
workflow decides whether the move is allowed.

## 3. Try it as a member

Upgrade (`version` `0.6.0`, load, upgrade) and sign in as **bob**:

1. Create a task. Its status is *open*.
2. Click **done** in the status bar. Refused: *you may not move this task record to done from its state now*. There is
   no transition from `open` to `done`.
3. Click **doing**, then **done**. Both work.
4. Try to change the title of the finished task. Refused: *you may not change this task record*.
5. Try to move it back to **open**. Refused. Only a lead can reopen.

Sign in as **lee** (a lead): reopening and editing both work.

The messages come from the rule, and they reach the person who made the mistake. A script that wants to say something
different can catch the error with `try`/`catch` and call `fail("...")`; see [Rhai plugins](../rhai.md).

## What you learned

- `workflow` lists the allowed moves of a `select` field; `restrict` removes access based on a record's state.
- Rules apply to every path, so the page, a command and your own code all obey the same ones.
- A role exempts people from a restriction (`exempt_roles`), as `reopen` limits a transition (`roles`).

> **Stuck?**
> - *nothing is refused*: you are signed in as the administrator, who is exempt. Use bob.
> - *a new task cannot be created*: it was created in a state other than `initial`.
> - *"unknown state" at load*: a state in the workflow is not one of the field's options.

Previous: [Chapter 5](05-permissions.md). Next: [Chapter 7, Code](07-code.md).
