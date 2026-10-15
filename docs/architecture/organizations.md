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

The home screen for every member is the **Apps** page (`/apps`), a grid of tiles,
one per installed app (like Frappe's desk home). An app is an installed plugin that
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
# standalone = true         # keep a tile even with a single sidebar menu
```

`route` must be one of the plugin's own pages and cannot contain `{param}`
segments, so a tile always opens something; this is checked when the plugin is
loaded. `GET /api/ui/apps` returns the apps installed (and enabled) in the
caller's organization, for logged-in members only, **filtered by role**: pages
with `roles="…"` and `[app] roles = […]` are omitted unless the caller holds one
of those roles (or `org_admin` / developer). Each tile includes `menus`: the
plugin's main pages the caller may open (literal routes, not `/new`, `/hire`, or
`{param}` patterns; `menu="false"` hides a page). Pages may set `menu_group` for
a collapsible sidebar section.

A workspace with `launcher = "folder"` (default) groups leftover members under
the workspace label on Apps home; `launcher = "flat"` (Base) lists each member
as its own tile. Plugins that set the same `[app] folder` join a named folder
inside that workspace category (install/category stay on the workspace). In a
workspace folder, thin modules (at most one menu, or no `[app]` at all) fold
into the host tile (shortest plugin name — e.g. `hr`) unless
`[app] standalone = true`. Plugins with no workspace stay standalone on the
home grid. A folder with only one leftover app opens that app directly. An
organization with no apps shows an empty state.

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
`hr.hr_manager` do not unlock them — grant `org_admin` or `user_manager` instead.
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
| Invite, add existing, grant roles, deactivate in org | Developer, owner, `org_admin`, `user_manager` |
| Create / edit / delete custom roles (`custom.*`) | Developer, owner, `org_admin`, `user_manager` |
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

APIs: `GET/POST /api/ui/organizations/{db}/roles`, `PATCH/DELETE …/roles/{role}`,
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
