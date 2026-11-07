---
title: "Chapter 3: A list and a form"
description: "Give the plugin an app tile, a page that lists tasks and a form that adds one."
section: plugin
status: stable
order: 15
---
# Chapter 3: A list and a form

**Goal:** open Tasks in the web app, see the tasks, and add one with a form.

A **page** is an XML file in `pages/`. It says what to show, not how to draw it: the web app draws lists and forms the
same way for every plugin, in the current theme. See [Pages and data](../pages.md) for everything a page can hold.

## 1. The list

Create `plugins/tasks/pages/tasks.xml`:

<!-- file: ch03/tasks/pages/tasks.xml -->
```xml
<page route="/tasks" title="Tasks" model="task" roles="member" icon="check-square">
  <view type="list" model="task" source="list_tasks" create="/tasks/new" create_label="New task">
    <search/>
    <columns>
      <column field="title" label="Title"/>
      <column field="status" label="Status" fieldType="badge"/>
      <column field="due" label="Due"/>
      <column field="assignee" label="Assignee"/>
    </columns>
    <actions>
      <action name="remove" label="Delete" function="remove_task" confirm="Delete this task?" danger="true"/>
    </actions>
    <empty title="No tasks yet" description="Use New task to add one."/>
  </view>
</page>
```

- `route` is the address of the page inside the web app. `model` ties it to the `task` model.
- `roles="member"` says who may open the page: any signed-in member of the organization. Without `roles`, a page opens only
  for a member who holds some role, and a new member holds none, so they would be refused. Chapter 5 shows how to
  narrow access when you want to.
- `<view type="list">` shows records. `source` is the plugin function that returns them: our `list_tasks`.
- `create` is the page to open from the **New task** button.
- `<columns>` picks the columns; `fieldType="badge"` draws `status` as a coloured label (the colours are in the model).
- `<actions>` adds a **Delete** action to every row; `confirm` asks first. It calls `remove_task`, which we write below.
- `<empty>` is shown while there are no tasks.

## 2. The form

Create `plugins/tasks/pages/task_new.xml`:

<!-- file: ch03/tasks/pages/task_new.xml -->
```xml
<page route="/tasks/new" title="New task" model="task" roles="member">
  <view type="form" function="add_task" submit="Add task" success="Task added" redirect="/tasks">
    <field name="title" label="Title" required="true"/>
    <field name="details" label="Details" fieldType="text"/>
    <field name="due" label="Due date" fieldType="date"/>
    <field name="assignee" label="Assignee"/>
  </view>
</page>
```

- `type="form"` with `function="add_task"` sends the filled form to our function. `redirect` goes back to the list on
  success.
- Each `<field>` is one input. `fieldType` chooses the widget: `text` is a multi-line box, `date` a date picker. `required`
  stops an empty submission before it is sent.

## 3. The tile and the function

The plugin needs an `[app]` table so it appears on the Apps screen, and the list page needs `remove_task`:

<!-- file: ch03/tasks/plugin.toml#1-16 -->
```toml
[plugin]
name = "tasks"
label = "Tasks"
version = "0.3.0"
description = "A small team task list, built step by step in the plugin author tutorial."
authors = [{ name = "Your Name", email = "your.email@example.com" }]
dependencies = []
capabilities = ["db::query", "db::mutate"]
access_models = [{ name = "task", permissions = ["read", "write"] }]
is_builtin = false
script = "main.rhai"

[app]
icon = "check-square"
route = "/tasks"
description = "Team tasks"
```

<!-- file: ch03/tasks/main.rhai#21-23 -->
```rhai
fn remove_task(input) {
    db::delete("task", input.id)
}
```

The `[app]` table gives the tile its icon (a [Lucide](https://lucide.dev/icons) name), the route it opens and a short
description.

## 4. Upgrade and look

```sh
aether -c aether.toml --load-plugins plugins/tasks
aether -c aether.toml --upgrade-plugin tasks --org main
```

With `--serve --watch` running you can also just save the files. Reload the web app: a **Tasks** tile has appeared on
the Apps screen. Open it:

1. The list shows the task you added in chapter 2. Its status is a label, not plain text.
2. Select **New task**, fill in a title and a date, and submit. You are back on the list with two tasks.
3. Try submitting with the title empty. The form refuses before it sends anything.
4. Delete the second task from its row action.

You have a working app.

## What you learned

- A page is a few lines of XML: a list ties a model to a function; a form ties inputs to a function.
- The web app draws it, so the same page looks right in any theme and on a phone.
- The `[app]` table puts the plugin on the Apps screen.

> **Stuck?**
> - *no Tasks tile*: the plugin is not upgraded for the organization, or `[app]` is missing from `plugin.toml`.
> - *the list is empty but chapter 2 added a task*: check `source="list_tasks"` is spelled like the function.
> - *"you do not have permission to open this page"*: the page has no `roles="member"` and the person holds no role.
> - *a page is not found*: the page file must be in `pages/` and its `route` must match what you open.

Previous: [Chapter 2](02-a-model.md). Next: [Chapter 4, A detail page](04-detail-page.md).
