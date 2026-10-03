# Organizations, switching, and the Apps launcher

## A user in several organizations

A user can belong to more than one organization. Which one a request is for
depends on `[tenancy] org_resolution`:

| Mode | Who decides | Does the app ask? |
|---|---|---|
| `subdomain`, `path` | the address (`acme.example.com`, `/o/acme`) | **no** |
| `header` | the browser sends `X-Org-Slug` on every request | yes, when the user has several |
| `session_only` | the session remembers the choice | yes, when the user has several |

Whoever belongs to exactly one organization is never asked: that one is used.
A user with **two or more** memberships must choose, **developers included**. A
developer with fewer than two memberships is not asked: they go to the
Organizations page, and from there can enter any organization (including ones
they do not belong to).

### The modal

When the user must choose and has not, the web app shows a **mandatory modal**
("Choose an organization": no close button, Escape and clicking outside are
ignored) listing their organizations. Afterwards the same dialog is the
**switcher**, opened from the "Acme ▾" button in the navigation (and the desk
header), where it can be dismissed. The wording says how the choice is kept:

- **header:** this browser remembers it (localStorage `aether.org`) and sends it
  as `X-Org-Slug` on every API request. The choice is also saved to the session.
- **session:** it is saved to the session.

After a choice the app reloads the theme and the current page.

### API

- `GET /api/auth/orgs` returns `{ mode, organizations: [{ db_name, name, member }],
  current, selection_required }`. `mode` is `address`, `header` or `session`.
  Developers get every organization (`member: false` for those they do not belong
  to); `selection_required` counts memberships only. When the user must choose,
  the modal offers only the organizations they belong to; the later switcher also
  lists the rest under "Other organizations".
- `POST /api/auth/org` with `{ "org": "<db_name>" }` saves the choice in the
  session. It is refused with `403` for an organization the user is not in, and
  with `409` where the address decides.
- Any request that needs an organization answers `409 organization selection
  required` when the user has several and nothing selects one, and the web app
  responds by opening the modal.

## The Apps launcher

The home screen of everyone who is not a developer is the **Apps** page (`/apps`),
a grid of tiles, one per installed app. An app is an installed plugin that
declares `[app]`:

```toml
[plugin]
name = "chat"
label = "Team Chat"
version = "0.1.0"

[app]
label = "Chat"              # optional; defaults to the plugin's label
icon = "message-square"     # a lucide icon name (see below); the tile shows a letter otherwise
route = "/chat"             # a page of this plugin; what the tile opens
description = "Team messaging"
```

`route` must be one of the plugin's own pages and cannot contain `{param}`
segments, so a tile always opens something; this is checked when the plugin is
loaded. `GET /api/ui/apps` returns the apps installed (and enabled) in the
caller's organization, for logged-in members only. An organization with no apps
shows an empty state: administrators are told how to install one.

Icons are a fixed set bundled with the app (`web/src/lib/components/apps/icons.ts`);
an app naming another icon gets a coloured letter tile until it is added there.

## The desk is for developers

Developers (superusers) land on the **desk** after login: the Organizations
page, Settings and the developer tools, in a sidebar layout. Everything on the
desk except the Apps launcher is developer-only, in the web app (other users are
sent to `/apps`) and on the server (the settings API answers `403`). A theme that
names the `desk` layout is ignored for users who are not developers.
