# Chatter

The conversation and history of a record, in the style of Odoo's chatter and Frappe's timeline. It
is **off unless a model asks for it**: a model without it has no panel, no endpoints, no stored lines
and no cost.

## Turning it on

In the model's JSON (or the **Chatter** box in the model editor, Studio → Models):

```json
{
  "name": "ticket",
  "chatter": { "enabled": true, "messages": true, "notes": true, "followers": true,
               "track_changes": true, "visitors": "none" },
  "fields": [ { "name": "status", "type": "select", "track": true, "options": [] } ]
}
```

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Everything else is ignored unless this is true |
| `messages` | `true` | Messages, which notify followers |
| `notes` | `true` | Internal notes, which notify nobody except people mentioned |
| `followers` | `true` | Follow / unfollow / mute, and the creator follows automatically |
| `track_changes` | `true` | Write old and new values of fields marked `"track": true` |
| `visitors` | `none` | `none`, `read` or `read_write` for anonymous visitors |

Rules checked when the model is saved: visitors need chatter enabled; `read_write` needs messages;
a `json` field cannot be tracked; `track` needs chatter on with `track_changes`.

An organization can switch chatter off everywhere with the setting `chatter.enabled`.

## What is in a thread

| Kind | Written by | Notifies | Seen by |
|---|---|---|---|
| `message` | people | the record's followers, and anyone mentioned | members, and visitors when the model allows |
| `note` | members who can edit the record | only people mentioned | members |
| `change` | the kernel | nobody | members |
| `system` | the kernel (`created`) | nobody | members |

**Changes cannot be skipped or forged.** The kernel adds them to the same transaction as the write
(`db::create`, `db::update`, `db::delete`), so a write and its record succeed or fail together, and
writing the same value again is not a change. Values are stored under field ids, so a renamed
field still reads correctly, with today's label.

## Access

* Members need the model in the plugin's `access_models` (`read` to see and send messages, `write`
  for notes).
* Visitors need the model's `visitors` setting **and** read access to the model through the
  plugin's public pages or `public_access_models`. A visitor may only post messages, with a name
  (up to 60 characters, shown as written), and the usual visitor rate limit applies.
* Mentions must be active members of the organization; others are dropped.
* Edit: the author. Delete: the author or a developer. Restore from the trash: developers.

## Nothing is deleted

Deleting a message sets `deleted_at`; deleting a **record** sets `record_deleted_at` on its whole
thread. Both wait in the trash, where developers can see and restore a message. Cleanup (hourly)
removes trash older than the setting **`chatter.trash_retention_days`** (default 30; an
organization can override it). Settings live in Settings → Chatter.

## API

`/api/chatter/{plugin}/{model}/{key}`: `GET` (thread, followers, what the caller may do, tracked
field metadata; `{"enabled": false}` when off), `POST …/messages`, `PATCH …/messages/{id}`,
`DELETE …/messages/{id}`, `POST …/messages/{id}/restore`, `PUT …/follow`; `GET /api/chatter/people?q=`
for mentions. A new post publishes a live `chatter` event so open pages reload their thread.

## Storage

Organization database tables `chatter_messages` and `chatter_followers` (migration `016_chatter`).
Plugins cannot name them as models.

## Not yet

Attachments on messages (planned through the media backend), a `chatter::post` command so plugins
can write system lines, and editing-history display (the earlier texts are stored in `edits`).
