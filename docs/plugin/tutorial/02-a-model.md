---
title: "Chapter 2: A model"
description: "Describe a task as a model, let Aether give it ids, and add and list tasks from the command line."
section: plugin
status: stable
order: 14
---
# Chapter 2: A model

**Goal:** a `task` model, and tasks you can add and list.

A **model** says what one kind of record looks like: its fields, their types and rules. Aether keeps the records in the
database and checks every write against the model. See [Models](../../architecture/models.md) for everything a model
can say.

## 1. Write the model

Create `plugins/tasks/models/task.json`. Write it **without ids**:

<!-- json-without-ids: ch02/tasks/models/task.json -->
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
      ]
    },
    {
      "name": "due",
      "label": "Due date",
      "type": "date"
    },
    {
      "name": "assignee",
      "label": "Assignee",
      "type": "string"
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
  }
}
```

What it says:

- `title_field` is the field used as the record's name wherever it is shown.
- `status` is a `select`: one of three options. `default` is used when none is given.
- `title` is `required` and at most 200 characters; `index: plain` makes searching by it fast.
- `view` is the default look of a list: which columns and in what order.

## 2. Ids: let the tool write them

Every model, field and option needs a stable **id**. Names are for people: you can rename `title` later and no data
moves, because the database stores the value under the id. Ids must be unique across the whole installation, so
**never write one by hand and never copy one from another plugin or from this tutorial**. Let Aether assign them:

```sh
aether -c aether.toml --sync-models plugins/tasks
```

Open `models/task.json` again: every model, field and option now has an `id` like `fld_4awjvqyfx6pzzxzat5cj`. Running
it again changes nothing, and an id that exists is never changed.

## 3. Let the plugin use the model

Two things must be said in the manifest. The plugin needs the database capabilities, and it must name the models it may
read and write (`access_models`). Add commands to try it with:

<!-- file: ch02/tasks/plugin.toml -->
```toml
[plugin]
name = "tasks"
label = "Tasks"
version = "0.2.0"
description = "A small team task list, built step by step in the plugin author tutorial."
authors = [{ name = "Your Name", email = "your.email@example.com" }]
dependencies = []
capabilities = ["db::query", "db::mutate"]
access_models = [{ name = "task", permissions = ["read", "write"] }]
is_builtin = false
script = "main.rhai"

[[command]]
name = "hello"
function = "hello"
help = "Say hello, to check that the plugin runs."

[[command]]
name = "add"
function = "add_task"
help = "Add a task."
args = [
  { name = "title", required = true },
  { name = "due", help = "YYYY-MM-DD" },
  { name = "assignee" },
]

[[command]]
name = "list"
function = "list_tasks"
help = "List the tasks, earliest due date first."

[plugin.api]
version = "0.1"

[plugin.meta]
kind = "addon"

[communication]
channels = []
```

The new lines are `capabilities`, `access_models` and the two `[[command]]` blocks `add` and `list`. Nothing can touch
a model that is not in `access_models`.

## 4. The functions

<!-- file: ch02/tasks/main.rhai -->
```rhai
// Tasks: the plugin author tutorial's example. This file is the whole plugin; nothing to compile.

fn hello(input) {
    #{ message: "Hello from tasks", actor: context::get().actor }
}

fn add_task(input) {
    if input.title == () || input.title == "" { fail("a title is required"); }
    db::create("task", #{
        title: input.title,
        due: input.due,
        assignee: input.assignee
    })
}

fn list_tasks(input) {
    db::find("task", #{ order: "due", limit: 200 })
}
```

- `fail("...")` stops the call and shows the message to whoever called. Use it for anything you want a person to read.
- `db::create(model, data)` writes one record and returns it. The kernel checks it against the model: a missing
  required field or a status that is not an option is refused.
- `db::find(model, query)` reads records. `order` sorts them.

## 5. Try it

Load and upgrade the installed plugin to the new version, then add and list:

```sh
aether -c aether.toml --load-plugins plugins/tasks
aether -c aether.toml --upgrade-plugin tasks --org main
aether -c aether.toml --command tasks.add --org main --arg title="Write chapter 2" --arg due=2026-12-01 --arg assignee=ann
aether -c aether.toml --command tasks.list --org main
```

You should see your task, with an `id`, `status` `open` (the default) and the other fields.

Now fail on purpose. Leave the title out:

```sh
aether -c aether.toml --command tasks.add --org main --arg due=2026-12-01
```

The command line refuses first: `title` is `required` in the `[[command]]` block, so the call never reaches your
function. Now send an *empty* title, which the command line accepts:

```sh
aether -c aether.toml --command tasks.add --org main --arg title= --arg due=2026-12-01
```

This time it is your own check that stops it: `tasks.add failed: a title is required`. That is the `fail` in
`add_task`. A command line, a page and the model each check what they can; your function is the place for the rules
only you know.

The model refuses what does not fit its fields, too. Send a due date that is not a date:

```sh
aether -c aether.toml --command tasks.add --org main --arg title=x --arg due=notadate
```

The answer names the field and says what is wanted: `task.due: must be a date like 2026-10-03`. The model did that;
you wrote nothing for it.

## What you learned

- A model is a JSON file; ids come from `--sync-models`, never from you.
- A plugin names the models it may use in `access_models` and the database commands it may send in `capabilities`.
- `db::create` and `db::find` are how a script writes and reads records, and the model checks every write.

> **Stuck?**
> - *model has no ids* at load: run `--sync-models` first.
> - *model 'task' is not in this plugin's access_models*: add it to `access_models`.
> - *capability not granted*: add `db::query` (read) or `db::mutate` (write) to `capabilities`.

Previous: [Chapter 1](01-hello.md). Next: [Chapter 3, A list and a form](03-list-and-form.md).
