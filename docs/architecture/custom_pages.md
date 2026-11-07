---
title: "Custom pages: panes, live data and plugin layouts"
description: "Pages that are not a list or a form: a channel rail, a message area and a thread pane side by side, fed live from topics, with a shell the plugin controls."
section: architecture
status: stable
order: 221
---
# Custom pages: panes, live data and plugin layouts

**Status: partly built.** Built: `<page shell="none">`, `<panes>` and `<pane>` (resizable, collapsible, one pane at a time on a narrow screen), optional last route segment `{name?}`, `query="…"` with `{query.x}`, the `<viewer>` widget, `topic=` on data widgets, `<messages>` (virtual list, display names, attached files, reactions, edit and delete, highlighted mentions) `<composer>` (send, typing, attachments, @mentions) and `<rail>` (groups, unread badges, online dots, live refresh, search, folding). Their reference is in [Pages](../plugin/pages.md). `<presence>` and live online dots in `<rail>` are built too. `<heading>`, `{me}` and the `<files>` browser are built too. **Not built:** drag-to-reorder in `<rail>`, plugin-shipped layouts, and `<panes direction="column">` nesting beyond one level has not been exercised. These pages are exercised in a real browser by `tests/e2e/realtime/ui_e2e.mjs`. Choices are marked *Decide* in the last section.

A page today is one scrolling sheet with a title bar, holding a list, a form, a kanban or a dashboard, refreshed
when a model changes. That fits records. A chat, a mail client or a board with a detail pane is a different shape:
several regions side by side, a long list that grows at the bottom, a route that picks the channel and the thread,
and data that arrives by itself. This page adds what a plugin needs for such an app, with no new kind of plugin code
in the browser. It builds on [Realtime](realtime.md) and the page language in [Pages](../plugin/pages.md).

## Take over the page: `shell`

`<page shell="none">` drops the title bar and the sheet around the content; the page fills the area under the app
chrome and each region scrolls by itself. `shell="chrome"` (default) is today's behaviour. With the page `layout`
set to `bare` as well, the page fills the window.

## Regions: `<panes>`

```xml
<page route="/chat/{channel}" shell="none">
  <panes>
    <pane name="rail" width="260" min="200" max="400" collapsible="true">
      <rail source="my_channels" topic="inbox" active="{channel}" open="/chat/{id}"/>
    </pane>
    <pane name="main" grow="true">
      <messages source="channel_messages" topic="channel.{channel}" send="post_message" typing="true"/>
    </pane>
    <pane name="thread" width="360" show="{query.thread}" collapsible="true">
      <messages source="thread_messages" topic="thread.{query.thread}" send="post_reply"/>
    </pane>
  </panes>
</page>
```

- Panes sit left to right on a wide screen and become one pane at a time on a narrow one (a back button returns to the
  rail). A pane can be resized within `min` and `max`, and the width is remembered per person.
- `show` is a condition on route values and query values: `{query.thread}` is true when `?thread=…` is present. Opening
  a thread pane is a link to `?thread=…`; it needs no code.
- Panes may nest (`<panes direction="column">`) for a header over a body.

## Route and query values

A route may have an optional last segment, `/chat/{team}/{channel?}`, and the query string is available as
`{query.name}` anywhere a route value is: in `source` calls, `topic`, `open` and `show`. Changing the query does not
reload the page; widgets that depend on it refresh themselves.

## Live data: `topic` on a widget

Any data widget may carry `topic="…"` (the plugin's own topic; the kernel adds the namespace). The widget subscribes
while it is on screen and, on a message, either appends the payload as a row (when the plugin sends a whole row) or calls
its `source` again. This replaces "refresh when the model changes" with "refresh when something relevant happens".

## New widgets

| Widget | What it does |
|---|---|
| `<panes>`, `<pane>` | The regions above |
| `<rail source topic active open>` | A navigation list from a function: groups (teams), items (channels) with a badge for unread and a dot for online; collapsible groups; drag order is a later step. Replaces the static sidebar for apps whose entries come from data |
| `<messages source topic send typing>` | A virtual-scrolling list that starts at the bottom, loads older rows upward (`source` is called with `{ before, limit }`), appends live rows, groups by author and day, shows "new messages" and who is typing, and keeps scroll position when rows arrive above |
| `<composer send typing attachments>` | A message box: Enter sends, Shift+Enter breaks the line, mentions and a file button that uploads through [Media](media.md); sends `typing` while the person types |
| `<presence topic>` | Avatars of who is connected to a topic |
| `<call room leave moderate>` | A video meeting: a check-your-devices step, then a tile for each person with microphone, camera and leave. `room` is a room the plugin declared with `[[sfu]]` ([Meetings](meet_sfu.md)); `leave` is where to go afterwards; `moderate` names a plugin function the host's buttons (mute someone, remove someone, end for all) call with the page's values, `action` and `user` (the plugin checks that the caller is the host) |
| `<viewer>` | Already available: shows an attached file |

`<messages>` and `<composer>` are separate so a thread pane or a comment box on a record can reuse the composer.
Reactions, replies and edits are actions on a row (`<action>` inside `<messages>`, the same contract as a list row), so
the plugin keeps the behaviour and the kernel only draws it.

## Layouts a plugin can ship

Only a theme plugin can ship a shell layout today (`layouts/<name>.json`). An ordinary plugin may ship
`layouts/<name>.json` too, **namespaced to itself** (`layout="chat.workspace"`) and valid only on its own pages. It uses
the same regions and blocks as theme layouts and adds the block `route-outlet`, which is where the page goes. A theme
keeps control of the app's top bar and colours; a plugin layout controls what is around its own page (for example a
slim left strip of teams with the sidebar hidden).

## Performance and safety

- Message lists are virtual: only the rows near the screen are in the document, with spacers for the rest, and at most
  2000 rows are kept in memory; older rows are loaded as the person scrolls up.
- Rendering stays data-driven: rows are drawn by built-in widgets from JSON, never from HTML supplied by a plugin.
  Rich text arrives through the sanitizer planned for the rich-text field type.
- `topic` subscriptions are released when the widget leaves the screen.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Add widgets for chat, or let plugins ship JavaScript components? | **Built-in widgets** driven by functions and topics. Script in the app's origin is not allowed; the sandboxed frame used by [viewers](document_viewer.md) is the escape hatch for the odd custom region. |
| 2 | Are panes a new view type or plain layout widgets? | **Layout widgets** usable anywhere in a page, not only in a `view`. |
| 3 | Optional route segments and query values? | **Yes**, both; the query does not reload the page. |
| 4 | Can an ordinary plugin ship a layout? | **Yes, namespaced and limited to its own pages.** |
| 5 | Does `<rail>` replace the sidebar menu? | **No.** It fills a pane; the sidebar menu is unchanged. |
