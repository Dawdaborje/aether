---
title: "Chapter 8: Reacting"
description: "Announce that a task is done with an event, react to it in the background, and run a job every morning."
section: plugin
status: stable
order: 20
---
# Chapter 8: Reacting

**Goal:** finishing a task tells everyone, and every morning the late tasks are flagged, without anyone clicking.

Two ways for work to happen without a person waiting for it:

- An **event** says "this happened" and lets other code react later, in the background.
- A **schedule** runs a function at set times.

Both run through the same background queue ([Events](../../architecture/events.md), [Scheduler](../../architecture/scheduler.md)).
A job runs again if it fails, so write it to be safe to repeat.

## 1. A new field

The morning job will flag late tasks. Add an `overdue` field to the model, and ask for ids only for what is new:

<!-- json-without-ids: ch08/tasks/models/task.json -->
```json
{
  "name": "task",
  "label": "Task",
  "plural_label": "Tasks",
  "icon": "check-square",
  "title_field": "title",
  "fields": [
    {
      "name": "title",
      "label": "Title",
      "type": "string",
      "required": true,
      "max_length": 200,
      "index": "plain"
    },
    {
      "name": "details",
      "label": "Details",
      "type": "text"
    },
    {
      "name": "status",
      "label": "Status",
      "type": "select",
      "default": "open",
      "options": [
        {
          "value": "open",
          "label": "Open",
          "color": "gray"
        },
        {
          "value": "doing",
          "label": "Doing",
          "color": "blue"
        },
        {
          "value": "done",
          "label": "Done",
          "color": "green"
        }
      ],
      "track": true
    },
    {
      "name": "due",
      "label": "Due date",
      "type": "date",
      "track": true
    },
    {
      "name": "assignee",
      "label": "Assignee",
      "type": "string",
      "track": true
    },
    {
      "name": "overdue",
      "label": "Overdue",
      "type": "bool",
      "default": false
    }
  ],
  "view": {
    "list": [
      "title",
      "status",
      "due",
      "assignee"
    ],
    "sort": [
      {
        "field": "due"
      }
    ]
  },
  "chatter": {
    "enabled": true,
    "messages": true,
    "notes": true,
    "followers": true,
    "track_changes": true
  }
}
```

```sh
aether -c aether.toml --sync-models plugins/tasks
```

It writes ids for the new field only; the ones you have stay as they were. Also show the flag in the list, by adding a
column to `pages/tasks.xml`, after the assignee column:

<!-- file: ch08/tasks/pages/tasks.xml#9-10 -->
```xml
      <column field="assignee" label="Assignee"/>
      <column field="overdue" label="Late" fieldType="boolean"/>
```

## 2. Announce, listen, schedule

Add these to `main.rhai`:

<!-- file: ch08/tasks/main.rhai#60-62 -->
```rhai
private fn announce_done(task) {
    events::emit(#{ event: "task_done", payload: #{ id: task.id, title: task.title, assignee: task.assignee } });
}
```

<!-- file: ch08/tasks/main.rhai#65-68 -->
```rhai
fn on_task_done(received) {
    notify::send(#{ title: "Task done", body: received.payload.title, level: "info" });
    #{}
}
```

<!-- file: ch08/tasks/main.rhai#71-77 -->
```rhai
fn mark_overdue(input) {
    let late = overdue_tasks(input);
    for t in late {
        if t.overdue != true { db::update("task", t.id, #{ overdue: true }); }
    }
    #{ marked: late.len() }
}
```

- `private fn` is a helper that cannot be called from outside. `announce_done` emits the event `task_done` with a small
  payload.
- `on_task_done` is the *listener*. It receives the event and sends a notification to the organization's members.
  It runs in the background, **as the plugin**, not as the person who finished the task, so it can do things that
  person may not.
- `mark_overdue` reuses `overdue_tasks` from chapter 7 and flags each late task once.

Now call `announce_done` from the two places a task becomes done. In `update_task`, after the update, and in
`finish_task`:

<!-- file: ch08/tasks/main.rhai#25-32 -->
```rhai
fn update_task(input) {
    let id = input.id;
    let data = input;
    data.remove("id");
    let task = db::update("task", id, data);
    if task.status == "done" { announce_done(task); }
    task
}
```

<!-- file: ch08/tasks/main.rhai#51-57 -->
```rhai
fn finish_task(input) {
    let task = db::get("task", input.id);
    if task.status != "doing" { fail("only a task that is being worked on can be finished"); }
    let task = db::update("task", input.id, #{ status: "done" });
    announce_done(task);
    task
}
```

## 3. Declare them in the manifest

Capabilities for what the code now does, the event, who listens, and when the job runs:

<!-- file: ch08/tasks/plugin.toml#8-8 -->
```toml
capabilities = ["db::query", "db::mutate", "events::emit", "events::subscribe", "notify::send", "scheduler::register"]
```

<!-- file: ch08/tasks/plugin.toml#23-35 -->
```toml
[[events]]
name = "task_done"

[[events]]
name = "tasks.task_done"
direction = "listen"
handler = "on_task_done"

[[schedule]]
name = "mark-overdue"
title = "Mark overdue tasks"
function = "mark_overdue"
cron = "0 6 * * *"
```

- `[[events]]` with just a `name` declares an event the plugin **emits**. Its full name is `tasks.task_done`.
- The second `[[events]]` is a **listener**: `direction = "listen"` names the event and `handler` the function. A plugin
  can listen to its own events or, if it lists the other plugin in `dependencies`, to another's.
- `[[schedule]]` runs `mark_overdue` at 06:00 every day. `cron` has five fields (minute, hour, day, month, weekday);
  `every = "15m"` is the alternative for "every so often".

## 4. Try it

Bump `version` to `0.8.0`, load and upgrade. Then, as a member, create a task, start it and finish it. Open the
notifications (the bell): *Task done* with the task's title. You did not write any code to show it; `notify::send` put it there.

Finish a task from the **Finish** button as well as from the status bar: both announce, because both call `announce_done`.

For the schedule you do not need to wait until 06:00. Run the same function by hand:

Add a late task, then run it:

```sh
aether -c aether.toml --command tasks.add --org main --arg title="Late and open" --arg due=2019-05-05
aether -c aether.toml --command tasks.mark_overdue --org main
```

You should see `"marked": 1`, and the task now shows **Late** in the list. Developers can also see the schedule, run it
now and look at its history under **Desk → Schedules**.

## What you learned

- `events::emit` announces; a `[[events]]` listener reacts in the background, as the plugin.
- `[[schedule]]` runs a function on a cron timetable or every so often.
- `notify::send` puts a notification in the bell for the organization's members.
- Background functions may run twice: flag a task once, test before you change.

> **Stuck?**
> - *no notification*: the plugin needs `notify::send`, `events::emit` and `events::subscribe` in `capabilities`.
> - *the listener never runs*: the `[[events]]` listener name is `tasks.task_done`, with the plugin's name in front.
> - *`--command tasks.mark_overdue` not found*: add the `[[command]]` for it (it is in the file above) and upgrade.

Previous: [Chapter 7](07-code.md). Next: [Chapter 9, Translate and theme](09-translate.md).
