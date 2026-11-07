---
title: "Chapter 9: Translate and theme"
description: "Move the plugin's text into catalogs so it can be translated, and see how the plugin follows the theme."
section: plugin
status: stable
order: 21
---
# Chapter 9: Translate and theme

**Goal:** no text is written into the pages; it all lives in catalogs, in two languages.

## 1. A catalog per language

Create `plugins/tasks/i18n/en.json`, the language the plugin is written in:

<!-- file: ch09/tasks/i18n/en.json -->
```json
{
  "app.description": "Team tasks",
  "page.tasks": "Tasks",
  "page.new": "New task",
  "page.task": "Task",
  "page.overdue": "Overdue tasks",
  "field.title": "Title",
  "field.details": "Details",
  "field.due": "Due date",
  "field.assignee": "Assignee",
  "field.status": "Status",
  "field.late": "Late",
  "col.due": "Due",
  "action.finish": "Finish",
  "action.delete": "Delete",
  "confirm.delete": "Delete this task?",
  "btn.new": "New task",
  "btn.add": "Add task",
  "btn.save": "Save",
  "msg.added": "Task added",
  "msg.saved": "Saved",
  "empty.title": "No tasks yet",
  "empty.text": "Use New task to add one.",
  "overdue.empty.title": "Nothing is overdue",
  "overdue.empty.text": "Every open task is within its due date.",
  "task.need_title": "a title is required",
  "task.finish_needs_doing": "only a task that is being worked on can be finished"
}
```

and `plugins/tasks/i18n/fr.json`, the same keys in French:

<!-- file: ch09/tasks/i18n/fr.json -->
```json
{
  "app.description": "Tâches de l'équipe",
  "page.tasks": "Tâches",
  "page.new": "Nouvelle tâche",
  "page.task": "Tâche",
  "page.overdue": "Tâches en retard",
  "field.title": "Titre",
  "field.details": "Détails",
  "field.due": "Échéance",
  "field.assignee": "Responsable",
  "field.status": "Statut",
  "field.late": "En retard",
  "col.due": "Échéance",
  "action.finish": "Terminer",
  "action.delete": "Supprimer",
  "confirm.delete": "Supprimer cette tâche ?",
  "btn.new": "Nouvelle tâche",
  "btn.add": "Ajouter la tâche",
  "btn.save": "Enregistrer",
  "msg.added": "Tâche ajoutée",
  "msg.saved": "Enregistré",
  "empty.title": "Aucune tâche",
  "empty.text": "Utilisez Nouvelle tâche pour en ajouter une.",
  "overdue.empty.title": "Rien n'est en retard",
  "overdue.empty.text": "Toutes les tâches ouvertes sont dans les délais.",
  "task.need_title": "un titre est obligatoire",
  "task.finish_needs_doing": "seule une tâche en cours peut être terminée"
}
```

Keys are plain names. A language may translate only some of them: a missing key falls back to the plugin's default
language, and last of all to the key itself, so a half-finished translation never breaks a page.

## 2. Use the keys in the pages

Any text attribute in a page takes a key between percent signs. The list page becomes:

<!-- file: ch09/tasks/pages/tasks.xml -->
```xml
<page route="/tasks" title="%page.tasks%" model="task" icon="check-square" roles="member">
  <view type="list" model="task" source="list_tasks" open="/tasks/{id}" create="/tasks/new" create_label="%btn.new%">
    <search/>
    <filter field="status" options="open, doing, done" label="%field.status%"/>
    <columns>
      <column field="title" label="%field.title%"/>
      <column field="status" label="%field.status%" fieldType="badge"/>
      <column field="due" label="%col.due%"/>
      <column field="assignee" label="%field.assignee%"/>
      <column field="overdue" label="%field.late%" fieldType="boolean"/>
    </columns>
    <actions>
      <action name="finish" label="%action.finish%" function="finish_task"/>
      <action name="remove" label="%action.delete%" function="remove_task" confirm="%confirm.delete%" danger="true"/>
    </actions>
    <empty title="%empty.title%" description="%empty.text%"/>
  </view>
</page>
```

Text without `%` is shown as written, so you can convert one page at a time. Do the same in `task_new.xml`, `task.xml`
and `task_overdue.xml` (the finished plugin in `tasks/pages` has all four).

## 3. Say which language is the base

In `plugin.toml`, the app description now uses a key, and the plugin names its base language:

<!-- file: ch09/tasks/plugin.toml#13-16 -->
```toml
[app]
icon = "check-square"
route = "/tasks"
description = "%app.description%"
```

<!-- file: ch09/tasks/plugin.toml#67-68 -->
```toml
[plugin.i18n]
default = "en"
```

## 4. Messages built in code

The words in `fail("...")` are text too. `i18n::t("key")` fetches a key from the catalogs in the language of the request,
and gives back the key itself when nobody translated it:

<!-- file: ch09/tasks/main.rhai#51-57 -->
```rhai
fn finish_task(input) {
    let task = db::get("task", input.id);
    if task.status != "doing" { fail(i18n::t("task.finish_needs_doing")); }
    let task = db::update("task", input.id, #{ status: "done" });
    announce_done(task);
    task
}
```

The catalogs hold `task.need_title` and `task.finish_needs_doing` (see the files above). Give `i18n::t` a map for
`{name}` parameters, and a third map with `locale` to ask for a particular language:
`i18n::t("greeting", #{ name: "Ada" }, #{ locale: "fr" })`.

Dates and numbers need nothing from you: the browser formats them for each person's language.

## 5. Try it

Bump `version` to `0.9.0`, load and upgrade. Everything looks exactly as before: the English words now come from
`en.json`, and a refused action (try **Finish** on an open task) shows its message from the catalog.

Now see the French. The language Aether shows is the server's, set in `aether.toml`:

```toml
[i18n]
default_locale = "fr"
```

Restart the server and reload the web app: the tile, the list headings, the buttons and the form labels are in French. Set
it back to `"en"` when you are done. (A person's own language and an organization default are planned but not read yet; a
visitor who is not signed in gets the language their browser asks for. See
[Localization](../../architecture/localization.md).)

## The theme

You wrote no colours, fonts or spacing, and that is the point. Lists, forms, badges and status bars are drawn by the
web app with whatever **theme** is active, so Tasks looks right in the default theme and in any other theme that is
installed. A developer switches theme in **Settings → Appearance**. See [Themes](../../architecture/themes.md) for
writing one. If you find yourself wanting to style one of your pages, check first whether a stock widget already does it.

## What you learned

- Text lives in `i18n/<language>.json`; pages refer to it with `%key%`.
- A translation can be partial; the fallbacks keep the page working.
- A plugin that uses stock widgets follows the theme for free.

> **Stuck?**
> - *a page shows `field.title` or similar*: the key is missing from every catalog, or misspelled in the page.
> - *the French is not used*: `[i18n] default_locale` is missing from `aether.toml`, or the server was not restarted.

Previous: [Chapter 8](08-events-and-schedule.md). Next: [Chapter 10, Check and ship](10-ship.md).
