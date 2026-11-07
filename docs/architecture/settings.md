---
title: "Settings"
description: "Settings live under the desk (/desk/settings). Developers (superusers) can read and change global defaults and any organization's overrides."
section: architecture
status: stable
order: 400
---
# Settings

Settings live under the desk (`/desk/settings`). **Developers** (superusers) can
read and change global defaults and any organization's overrides. The
**organization owner** and members with `org_admin` can open Settings for the
current organization and save **organization overrides** only (the API refuses
them without an organization context). Other members get `403`.

The screen is a navigation of groups (General, Notifications, Security & Privacy,
User Interface, Appearance) beside the selected group's settings, shown as one card of rows. A row
shows:

- the label, a **source badge** (`Default`, or `Organization` when this
  organization overrides the global value), the description, and the setting's key;
- the control: a switch for on/off settings (saved the moment it is toggled, and
  put back if the save fails), or a field for text, numbers, lists and JSON. Edits
  show **Save changes** and **Discard** only while the value differs from what is
  saved; invalid values (non-numbers, malformed JSON) are explained and cannot be
  saved. Lists show their parsed values as chips.

Groups with more than a few settings get a filter box. **Appearance** is
special: it manages the organization's theme (see [themes.md](themes.md)).

## Settings a plugin adds

A plugin declares its own groups and settings in `plugin.toml`; they are added to the catalog when the plugin
is loaded and show in Settings like any other.

```toml
[[setting_group]]
id = "country"
label = "Country"
icon = "globe"

[[setting]]
group = "country"
key = "country.organization"   # starts with the plugin's name and a dot
label = "Organization country"
value = ""                     # the global default; text, a number or true/false
description = "The country this organization operates in."
```

- A `group` names a group declared here or one that already exists (a kernel group such as `general`).
- A setting that already exists keeps its value when the plugin is loaded again; only its label and text refresh.
- A plugin reads settings with `settings::get` (capability `settings::get`; any setting that is not a secret) and
  changes its own with `settings::set` (capability `settings::set`; only keys under its own name, and only when an
  organization administrator or the kernel caused the call). In Rust: `aether_sdk::settings`; in Rhai:
  `settings::get("key")` returns `{ key: value }`, `settings::get_many([...])`, `settings::set("key", value)`.
- Plugin settings are not secret and not encrypted.

## Secret settings

An API key or a password is a **secret** setting. It is encrypted before it is stored, and the API
and the screen never show it again: the field says whether a value is set, and entering a new one
replaces it. An organization that saves a blank secret goes back to using the global one. See
[Communication](communication.md#secret-settings) for how the encryption key is found and what must
be shared between processes.

## Settings added by the kernel

Besides the groups above, *Communication* holds email, SMS and every provider (SMTP, Resend, Twilio,
Termii, Africa's Talking), and *Scheduler* configures [background jobs](scheduler.md). They are seeded
by `aether --seed` like the rest. Every setting's label and key must be unique across the whole catalog.
