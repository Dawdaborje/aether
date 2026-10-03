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
