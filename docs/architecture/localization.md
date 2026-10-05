# Localization (i18n)

Aether shows every person the interface in their own language. Two things are translated, and
they work differently:

| What | Where the text lives | Resolved |
|---|---|---|
| The core interface (shell, login, settings, Apps launcher) | `web/messages/<locale>.json`, compiled by Paraglide | in the browser, at build time |
| Everything a plugin shows (pages, settings, commands, messages, emails, SMS) | the plugin's `i18n/<locale>.json` | in the kernel, per request |

Plugins are installed at run time, per organization and per version, so their text cannot be in
the web build. The kernel resolves it and the browser receives finished text.

## Which locale

The first of these that is set wins:

1. the person's `locale` (their profile),
2. the organization's default locale (a setting),
3. the server's default locale (`aether.toml`),
4. `en`.

On a public page with no signed-in person, the `Accept-Language` header comes before step 2.
Notifications use the **recipient's** locale, not the caller's (see below).

## For plugin authors

### Catalogs

```text
my_plugin/
├── plugin.toml
└── i18n/
    ├── en.json
    └── fr.json
```

```json
{
  "note.title": "Title",
  "note.empty": "No notes yet",
  "note.count": "{count, plural, one {# note} other {# notes}}",
  "note.greeting": "Hello, {name}"
}
```

Keys are flat strings. The syntax is the ICU MessageFormat subset used by the core catalogs:
`{name}` parameters and `plural` / `select`. A plugin declares its base locale, which is the
fallback of last resort:

```toml
[plugin.i18n]
default = "en"
```

A catalog may cover a language only in part. Each key falls back separately:
person's locale → the same language without region (`fr-CA` → `fr`) → organization default →
plugin default → the key itself.

### Pages and settings

Any text attribute takes a key written `%key%`. Text without `%` is a literal and is shown as is,
so existing plugins keep working.

```xml
<page route="/notes" title="%note.page%" model="note">
  <view type="list" source="list_notes">
    <columns><column field="title" label="%note.title%"/></columns>
    <empty title="%note.empty%"/>
  </view>
</page>
```

Settings labels and descriptions, `[[command]]` descriptions and `[app]` names take `%key%` the same way.

### Messages returned by functions

A function that returns or reports a message can return a **message** instead of a string:

```json
{ "message": { "key": "note.saved", "params": { "name": "Ada" } } }
```

The kernel turns it into text in the caller's locale before it reaches the page, so the form's
`success`, a toast, or an error shown under a form is translated. `Error::msg("plain text")`
keeps working; the SDK adds `Error::key("note.not_found").with("id", id)` and `Message::key(..)`.
A message may also carry `default` text, used when no catalog has the key.

### Messages built in code: `i18n::t`

```rhai
let text = i18n::t("note.count", #{ count: 3 });
let fr   = i18n::t("note.count", #{ count: 3 }, #{ locale: "fr" });
```

Without `locale`, it uses the caller's. A job run by the scheduler has no caller, so it names the
locale or the recipient.

### Email and SMS

`communication::send` takes a message instead of finished text, and the worker renders it when it
sends, in the **recipient's** locale:

```rhai
communication::send(#{
  to: user_id,
  subject: #{ key: "invoice.subject", params: #{ number: n } },
  body:    #{ key: "invoice.body",    params: #{ number: n } },
});
```

Plain strings are sent as written. The job stores the key and parameters, so a retry or a delayed
send still uses the locale the recipient has at that moment.

### Dates, numbers, currency

Plugins send values, not formatted text. The browser formats them with `Intl` in the resolved
locale. Use `fieldType` `date`, `datetime`, `currency` or `integer` and the rest is done for you.

### Right to left

The page root gets `dir="rtl"` for right-to-left locales (`ar`, `he`, `fa`, `ur`). Themes follow it
(see [Themes](themes.md)).

## Overriding another plugin's text

A plugin can replace the text of a plugin it depends on, to change a word or adapt it to a
business. It needs the capability and the dependency, and writes the keys as `<plugin>:<key>`
in its own catalogs:

```toml
# plugin.toml of the overriding plugin
dependencies = ["company"]

[plugin]
capabilities = ["i18n::override"]
```

```json
// i18n/en.json of the overriding plugin
{ "company:note.title": "Subject" }
```

Rules:

- Without `i18n::override` **and** a dependency on the target, loading the plugin fails with
  a message saying which is missing. Nothing is ignored silently.
- It applies only in organizations where both plugins are installed.
- The target does not need to allow it. Keys are not private, and only text is replaced, never behaviour.
- When two plugins override the same key, the one installed last wins.
- In each language of the fallback chain an override is tried before the target's own text, but a
  better language beats it: a French reader sees the target's French text before an English override.
- Keys with a `:` are not the overriding plugin's own text; they are not reachable as its keys.

## Core pieces

- `facets/localization`: catalog loading from plugin revisions, locale negotiation, message
  formatting, overrides. Today it is an empty stub.
- `pages_api.rs` resolves `%key%` in pages before they are served; function results carrying
  a `message` are resolved on the way out of the plugin call.
- The host dispatch gets `i18n::t`, and `capabilities/i18n.json` describes it (every plugin has
  `i18n::t`; `i18n.override` is the only one that must be asked for).
- The scheduler's communication job stores keys and parameters and renders at send time.
- Server default: `[i18n] default_locale = "en"` in `aether.toml`.
- Settings: a user `locale`, an organization `locale` and the server default, with the usual
  source badge (see [Settings](settings.md)).
