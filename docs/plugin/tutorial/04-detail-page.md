---
title: "Chapter 4: A detail page"
description: "Open a task, edit it, and see who changed what: a detail page and a change history."
section: plugin
status: stable
order: 16
---
# Chapter 4: A detail page

**Goal:** select a task in the list, edit it, and read its history.

## 1. The functions

A detail page loads one record and saves it back. Two functions:

<!-- file: ch04/tasks/main.rhai#21-23 -->
```rhai
fn get_task(input) {
    db::get("task", input.id)
}
```

<!-- file: ch04/tasks/main.rhai#25-30 -->
```rhai
fn update_task(input) {
    let id = input.id;
    let data = input;
    data.remove("id");
    db::update("task", id, data)
}
```

`update_task` receives the whole form: the record's `id` plus every field. The `id` says *which* record, so it is taken
out of the data before the update. Rhai passes maps by value, so `data.remove("id")` changes only the copy.

## 2. The page

Create `plugins/tasks/pages/task.xml`:

<!-- file: ch04/tasks/pages/task.xml -->
```xml
<page route="/tasks/{id}" title="Task" model="task" roles="member">
  <view type="form" source="get_task" function="update_task" submit="Save" success="Saved">
    <field name="title" label="Title" required="true"/>
    <field name="details" label="Details" fieldType="text"/>
    <field name="due" label="Due date" fieldType="date"/>
    <field name="assignee" label="Assignee"/>
  </view>
</page>
```

- `route="/tasks/{id}"` has a **route value**: whatever is in the address after `/tasks/` becomes `id`, and is sent to
  `get_task` as `{ id: ... }`.
- `source` loads the record, `function` saves it. The form is the same kind of view as chapter 3's, filled in.

Now make the rows of the list open it. In `pages/tasks.xml` add `open` to the list view, so it reads:

<!-- file: ch04/tasks/pages/tasks.xml#2-2 -->
```xml
  <view type="list" model="task" source="list_tasks" open="/tasks/{id}" create="/tasks/new" create_label="New task">
```

## 3. History

Every change to a task can be written down: who, when, which field, from what to what. That is a model setting called
**chatter**. Add this next to `fields` in `models/task.json`, and mark the fields worth tracking with `"track": true`:

<!-- json-without-ids: ch04/tasks/models/task.json -->
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

The two new things are the `chatter` block and `track` on `status`, `due` and `assignee`. (Do not touch the ids you
already have.) `messages`, `notes` and `followers` also give each task a conversation: people can leave notes and follow
it. See [Chatter](../../architecture/chatter.md).

## 4. Try it

Bump `version` in `plugin.toml` to `0.4.0` so Aether sees a new version, then:

```sh
aether -c aether.toml --load-plugins plugins/tasks
aether -c aether.toml --upgrade-plugin tasks --org main
```

Reload the web app, open Tasks and select a row. Change the assignee and save. Below the form the history now shows a
line like *Assignee: ann → bob*, with your name and the time. A task you never changed shows just *created*.

You added no code for the history. The kernel writes each line in the same transaction as the change, so a change
cannot happen without its line.

## What you learned

- A route value (`{id}`) in a page's address reaches the plugin function as input.
- A form with `source` and `function` is an edit page.
- `chatter` plus `track` gives every record a history and a conversation, from the model alone.

> **Stuck?**
> - *the row does not open anything*: `open="/tasks/{id}"` is missing from the list view.
> - *no history under the form*: `chatter.enabled` is not `true`, or the fields are not marked `track`.
> - *Saving a form with an empty title fails*: the `title` field is `required`; this is the form working.

Previous: [Chapter 3](03-list-and-form.md). Next: [Chapter 5, Permissions](05-permissions.md).
