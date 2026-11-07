---
title: "Chapter 7: Code"
description: "Write functions that find late tasks, a row button that finishes a task, and a page of what is overdue."
section: plugin
status: stable
order: 19
---
# Chapter 7: Code

**Goal:** a page listing overdue tasks, a **Finish** button on each row, and a command that counts what is late.

Rules say what is *allowed*. Code does what a rule cannot: look things up, combine them, give a person a friendly
answer. Your functions still go through the rules, so they cannot do more than the person calling them.

## 1. Find the overdue tasks

Add these two functions to `main.rhai`:

<!-- file: ch07/tasks/main.rhai#36-43 -->
```rhai
fn overdue_tasks(input) {
    let today = context::get().now.sub_string(0, 10);
    db::find("task", #{
        filter: #{ due: #{ lt: today }, status: #{ ne: "done" } },
        order: "due",
        limit: 200
    })
}
```

<!-- file: ch07/tasks/main.rhai#45-47 -->
```rhai
fn count_overdue(input) {
    #{ overdue: overdue_tasks(input).len() }
}
```

- `context::get().now` is the current time as text; its first ten characters are today's date (`2026-11-01`). Dates in this
  format compare correctly as text.
- A `filter` says which records to read: `{ lt: today }` is "less than", `{ ne: "done" }` is "not equal". The
  complete list is in [Queries](../../architecture/queries.md).
- `count_overdue` calls `overdue_tasks`: functions can call each other like in any language.

## 2. A button that finishes a task

<!-- file: ch07/tasks/main.rhai#49-53 -->
```rhai
fn finish_task(input) {
    let task = db::get("task", input.id);
    if task.status != "doing" { fail("only a task that is being worked on can be finished"); }
    db::update("task", input.id, #{ status: "done" })
}
```

The function does a check of its own before it asks for the move. `fail` stops the call and sends the message to the
person, who sees it next to the button. The workflow would also refuse the move, but its message talks about states;
yours can talk about tasks.

Put the button on each row of the list in `pages/tasks.xml`, in `<actions>`, above Delete:

<!-- file: ch07/tasks/pages/tasks.xml#11-14 -->
```xml
    <actions>
      <action name="finish" label="Finish" function="finish_task"/>
      <action name="remove" label="Delete" function="remove_task" confirm="Delete this task?" danger="true"/>
    </actions>
```

A row action calls the function with `{ id: <the row's id> }`, and the list reloads afterwards.

## 3. A page for what is late

Create `pages/task_overdue.xml`:

<!-- file: ch07/tasks/pages/task_overdue.xml -->
```xml
<page route="/tasks/overdue" title="Overdue tasks" model="task" roles="member">
  <view type="list" model="task" source="overdue_tasks" open="/tasks/{id}">
    <columns>
      <column field="title" label="Title"/>
      <column field="due" label="Due"/>
      <column field="assignee" label="Assignee"/>
    </columns>
    <empty title="Nothing is overdue" description="Every open task is within its due date."/>
  </view>
</page>
```

It is the same kind of list as the first page, with a different `source`.

## 4. A command

Add a command so a script or a scheduled job can count too. In `plugin.toml`:

<!-- file: ch07/tasks/plugin.toml#43-46 -->
```toml
[[command]]
name = "overdue"
function = "count_overdue"
help = "Count the tasks that are past their due date and not done."
```

## 5. Try it

Upgrade (`version` `0.7.0`, load, upgrade). Add a task with a due date in the past:

```sh
aether -c aether.toml --command tasks.add --org main --arg title="Old task" --arg due=2020-01-01
aether -c aether.toml --command tasks.overdue --org main
```

You should see `"overdue": 1` (or more). In the web app open `/tasks/overdue`: the old task is there. Back in the list,
select **Finish** on an *open* task: the message *only a task that is being worked on can be finished* appears. Start it
from its detail page, then **Finish** works.

## What you learned

- Functions read with `db::find` and filters, and are free to call each other.
- `fail` gives the person a message about *their* action.
- A row `<action>` runs a function on one record; a `[[command]]` runs one from the command line.
- Your code still obeys the rules: a member cannot use a function to do what a rule forbids.

> **Stuck?**
> - *`/tasks/overdue` is empty*: no task has a past `due` date that is not done.
> - *Finish does nothing*: look at the message on the page; the task is probably not `doing`.
> - *`unknown function`*: the plugin was not upgraded after you edited `main.rhai`.

Previous: [Chapter 6](06-rule.md). Next: [Chapter 8, Reacting](08-events-and-schedule.md).
