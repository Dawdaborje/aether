# Settings

Settings are part of the developer desk: only developers (superusers) can read or
change them, in the web app and through the API (`/api/settings/...` answers `403`
to anyone else).

The screen is a navigation of groups (General, Notifications, Security & Privacy,
Appearance) beside the selected group's settings, shown as one card of rows. A row
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

## Secret settings

An API key or a password is a **secret** setting. It is encrypted before it is stored, and the API
and the screen never show it again: the field says whether a value is set, and entering a new one
replaces it. An organization that saves a blank secret goes back to using the global one. See
[Communication](communication.md#secret-settings) for how the encryption key is found and what must
be shared between processes.

## Settings added by the kernel

Besides the groups above, *Email*, *SMS* and a group for each provider (SMTP, Resend, Twilio, Termii,
Africa's Talking) configure [communication](communication.md), and *Scheduler* configures
[background jobs](scheduler.md). They are seeded by `aether --seed` like the rest. Every setting's label
and key must be unique across the whole catalog.
