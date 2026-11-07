---
title: "Organizations and the Apps launcher"
description: "An installation serves one organization: how it is created, who belongs to it, and the Apps launcher."
section: architecture
status: stable
order: 310
---
# Organizations and the Apps launcher

## One organization per installation

An Aether installation serves exactly one organization. (Several organizations in one
installation are part of Aether Enterprise.) `[tenancy]` in `aether.toml` names it:

```toml
[tenancy]
mode = "single"             # the only mode in this edition
single_org_name = "Main"    # display name, used by `aether --init`
single_org_db_name = "main" # the organization's database
```

`aether --init` creates the organization, owned by the developer account, so a fresh install
is ready to use. Then:

- Every request is for that organization. Nothing in a request or session names another, and
  nobody is ever asked to choose.
- Every account is a member from the start: users created from the desk or with
  `--create-user` are added to it automatically.
- There is no way to create, disable, delete, switch or leave organizations. Handing the
  organization to another member (ownership transfer) works.
- The desk's **Organization** entry opens the one organization (members, roles, schedules,
  plugins).
- The server refuses to start if the organization does not exist, and says to run
  `aether --init`. `mode = "multi"` is rejected with a message saying it is an Enterprise feature.
- `GET /api/auth/orgs` answers `mode: "single"`, the one organization as `current`, and
  `selection_required: false`.

Under the hood the kernel still keeps a `core` database and the organization's own database,
so the rest of this page (roles, apps, the desk) applies unchanged.

An account created by signing in through an outside provider (OAuth) is **not** added to the
organization by default: anyone with a Google or GitHub account could otherwise join. An
administrator adds them (Users → add existing, by email) or invites them; until then they are
treated as a visitor. A private provider can set `oauth_auto_join = true`, and a first-time
sign-in then joins at once; a member an administrator removed is not put back.

## The Apps launcher

The home screen for every member is the **Apps** page (`/apps`), a grid of tiles,
one per installed app. An app is an installed plugin that
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
# folder = "Messaging"      # optional: create/join a named folder (same workspace)
# nav = true                # a link beside Users / Settings, not a tile
# standalone = true         # keep a tile even with a single sidebar menu
# also_in_host = true       # keep the tile and also list its menu in the workspace host sidebar

[app.tile]                  # optional: a live number on the launcher tile
function = "count_open"     # a function of this plugin answering a number (or { "count": n }, or a list)
label = "Open requests"     # optional, shown under the number
unit = "%"                  # optional, shown after it

[[app.palette]]             # optional, up to 20: entries of the search palette (Ctrl/⌘+K)
label = "New message"
href = "/chat/new"          # an app path; or `function = "…"` (run it) or `search = "…"` (find records)
icon = "plus"               # optional
keywords = "write compose"  # optional: extra words that find it
shortcut = "alt+m"          # optional: `mod`, `alt`, `shift` joined by `+`, then a letter or digit
roles = ["chat_user"]       # optional; omit = whoever sees the app
```

`route` must be one of the plugin's own pages and cannot contain `{param}`
segments, so a tile always opens something; this is checked when the plugin is
loaded. `GET /api/ui/apps` returns the apps installed (and enabled) in the
caller's organization, for logged-in members only, **filtered by role**: pages
with `roles="…"` and `[app] roles = […]` are omitted unless the caller holds one
of those roles (or `org_admin` / developer). Each tile includes `menus`: the
plugin's main pages the caller may open (literal routes, not `/new`, `/hire`, or
`{param}` patterns; `menu="false"` hides a page). Pages may set `menu_group` and `menu_group_icon` for
a collapsible sidebar section.

A workspace with `launcher = "folder"` (default) groups leftover members under
the workspace label on Apps home; `launcher = "flat"` (Base) lists each member
as its own tile. Plugins that set the same `[app] folder` join a named folder
inside that workspace category (install/category stay on the workspace). In a
workspace folder, thin modules (at most one menu, or no `[app]` at all) fold
into the host tile (shortest plugin name — e.g. `hr`) unless
`[app] standalone = true`. `[app] also_in_host = true` keeps the tile and also adds its menu to the host sidebar. Plugins with no workspace stay standalone on the
home grid. A folder with only one leftover app opens that app directly. An
organization with no apps shows an empty state.

### Live tiles

`[app.tile]` makes the launcher tile show the answer of a function of the plugin. The web app
asks it through the ordinary plugin call API (so the function's own permission checks apply)
while the tile is on screen: when it appears, every minute, when the window regains focus, and
at once when the plugin emits an event or a notification arrives. Tiles in the compact home
menu do not show it. `GET /api/ui/apps` returns it as `tile: { function: "plugin.fn", label,
unit }`.

### Palette entries

`[[app.palette]]` adds entries to the search palette, listed under "Commands" before
screens and apps. Each entry has exactly one target:

* `href`: opens a page of the app.
* `function = "start_request"`: runs that function of the plugin (no arguments) when chosen. If
  it answers `{ "href": "/leave/7" }` the app opens that page; `{ "message": "…" }` (or a plain
  string) shows the text; a failure shows its error.
* `search = "find_requests"`: not listed itself. While someone types (two letters or more,
  after a short pause) the palette calls the function with `{ "q": "<typed>", "limit": 5 }` and
  lists what it answers under "Records": `[ { "label": "Ada Lovelace", "href": "/leave/7",
  "context": "Annual" } ]` (or `{ "rows": […] }`). Only app paths are kept, at most five per app.
  It cannot have a `shortcut`.

An entry with an `href` may also set `create = true`: the shell's `create` block lists it under "New".

`GET /api/ui/apps` returns them in `palette`, already filtered: an entry with `roles` needs one
of them, one without goes to whoever sees the app. Functions are called through the ordinary
plugin call API, so their own permission checks apply. A `shortcut` needs `mod` (⌘ on a Mac,
Ctrl elsewhere) or `alt`; it is stored lower case in the order `mod`, `alt`, `shift`, and works
anywhere in the app. `mod+k` opens the palette and cannot be used, and two entries of one app
cannot share a shortcut.

### Setup steps on the Apps page

A page can ask to be offered on the Apps page until something is set up:

```xml
<page route="/base/setup/country" title="Choose your country" roles="org_admin" menu="false"
      setup_when="needs_country_setup"
      setup_text="Tell us which country this organization operates in.">
```

`setup_when` names a function of the page's plugin. On every `GET /api/ui/apps` the kernel calls it as the
caller, for people allowed to open the page (`roles`). An answer of `true`, a record or a non-empty list shows
the page as a card above the tiles (`setup: [{ plugin, title, description, href }]`, `description` from
`setup_text`); `null`, `false` or an empty list hides it, as does a function that fails. The answer is never
cached, so the card goes away as soon as the setup is done. The page should redirect to `/apps` when saved.

With `setup_modal="true"` on the page, the card (`modal: true` in the answer) opens the page's form in a dialog on the Apps
page instead of leaving it, and the list of steps is asked again once the form has saved. Use it for one-field steps.

Icons are a fixed set bundled with the app (`web/src/lib/components/apps/icons.ts`);
an app naming another icon gets a coloured letter tile until it is added there.

## The desk is for developers

Developers (superusers) land on the **desk** after login (`/desk/…`): Organizations,
Users (`/desk/users` — every account on the instance), installing apps (`/desk/apps`),
Settings, Bridges and developer tools, in a sidebar layout. The member-facing app
launcher stays at `/apps` for everyone. Most desk routes are developer-only in the
web app (other users are sent to `/apps`). **Exception:** organization owners and
`org_admin` may open **Settings** (`/desk/settings`) for the current organization
(organization overrides only). A theme that names the `desk` layout is ignored for
users who are not developers.

Home → Navigate and the Apps left sidebar show **Users**, **Roles**, and **Settings**
only when the session has those org capabilities (`can_manage_users` /
`can_manage_org`). Org managers open **Users** at `/users` (current organization),
separate from the developer desk at `/desk/users`. Plugin roles such as
`hr.hr_manager` do not unlock them — grant `org_admin`, `user_manager`, or a custom role that holds
the permission instead.
Leave **kinds of leave** are configured in the HRMS **Configurations** app
(`hr_config`, `/hr/config/leave-types`, role `hr_leave.leave_admin`), not under
org Settings and not in the Leave app sidebar.

## User management

Who may list and manage members of an organization:

| Actor | Scope |
|---|---|
| Developer | Every platform user (`GET /api/ui/users`) and members of any org |
| Organization owner | That org’s members (no extra role needed) |
| `org_admin` | That org’s members (no extra role needed) |
| `user_manager` | That org’s members (granted with `--grant-role user_manager`) |

| Action | Who |
|---|---|
| List, invite, add existing, deactivate members | Developer, owner, or a role with `members.manage` |
| Create / edit / delete custom roles (`custom.*`), set their organization permissions, grant or revoke roles | Developer, owner, or a role with `roles.manage` |
| Set password / send reset magic link | Owner or developer |
| Kick (soft-remove) from org | Owner or developer |
| Grant/revoke `org_admin` | Owner, developer, or `org_admin` |

Org managers use `/users` and `/roles` (current organization). Developers use
Desk → Users and Desk → Organizations → Members / Roles. Opening a member shows
Overview, Roles, and Password. Opening a role shows holders and grant/revoke.

### Roles catalog

Roles live in each organization’s database, so one org never sees another’s roles.

| Kind | Name | Source |
|---|---|---|
| System | `org_admin`, `user_manager` | Kernel |
| App / plugin | `<plugin>.<name>` (e.g. `hr.hr_manager`) | Declared in `plugin.toml`, synced on install |
| Custom | `custom.<slug>` | Created in the org Roles UI / `POST …/roles` |

Custom roles are grantable like any other role string (pages, apps, record rules).
Plugin pages that use short names still qualify to that plugin (`approver` →
`hr.approver`); to gate on a custom role, name it fully (`custom.shift_lead`).
System and plugin roles cannot be edited or deleted from the UI; custom roles can.

### Organization permissions

What a role may do to the organization itself is a set of kernel permissions, separate from the record
rights plugins give it (see [Rules](rules.md)):

| Permission | Lets a person |
|---|---|
| `members.manage` | list, invite, edit and deactivate members (the Users page) |
| `roles.manage` | create, edit and delete custom roles, choose their permissions, and give or take roles away (the Roles page) |
| `org.settings` | change the organization's settings |

`org_admin` holds all three by definition and `user_manager` holds `members.manage` and `roles.manage`;
neither can be changed. A custom role holds whatever an administrator ticks on its page (**Organization
permissions**), stored in `role_permissions` in the organization's database. The sidebar, the pages and every
endpoint check the permission, never a role name, so a "Role manager" is a custom role with only
`roles.manage`. Nobody can give a role a permission they do not hold themselves, and only the owner,
developers and `org_admin` can grant or revoke `org_admin`. `GET /api/auth/me` returns `can_manage_users`,
`can_manage_roles`, `can_manage_org` and the person's `permissions`.

APIs: `GET/POST /api/ui/organizations/{db}/roles`, `PATCH/DELETE …/roles/{role}`, `GET/PUT …/roles/{role}/kernel-permissions`,
plus per-member `POST/DELETE …/members/{user_id}/roles…`. CLI: `--list-roles`,
`--grant-role`, `--revoke-role`.

**Invite** creates an account with a generated password, assigns them to the org, and
queues a multipart (plain + HTML) email when email is configured. The temporary
password is also returned once in the API response so the admin can copy it.

**Deactivate** sets `org_users.is_active = false` for that organization (not a
platform-wide ban). **Kick** soft-deletes membership (`deleted_at` on
`organization_users` and `org_users`); rows are never hard-deleted. Re-adding the
same person clears the soft-delete.

Password reset uses the generic [magic link](magic_links.md) purpose
`auth.password_reset`. Ownership transfer stays owner-only.

A plugin in the current organization can do a narrower version of invite when its
manifest lists the capability. `org::create_member` creates an account with a
generated password (or adds an existing one) and does not accept a password.
`org::invite` emails `auth.account_invite` so the person chooses a password, or
adds an existing account without changing it. Neither command grants roles.
