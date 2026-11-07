---
title: "Sharing records"
description: "Rules decide who may see which records from roles and record state. They cannot express \"I give this one quote to Ana for a week\" or \"send the customer a link to this one invoice\"."
section: architecture
status: proposal
order: 410
---
# Sharing records (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

[Rules](rules.md) decide who may see which records from roles and record state. They cannot express "I give this one
quote to Ana for a week" or "send the customer a link to this one invoice". Sharing adds that, without a second
permission system.

## A share is a rule

A share is stored as data and evaluated by the same code as `access` rules (`facets/core/src/data_model/rules.rs`,
`kernel/host/guard.rs`). It only ever adds access, and only up to the level the sharer holds.

```
record_shares: model, record, subject (user | role | link_key), operations [read, write],
               expires_at, created_by, note, revoked_at
```

In the rule engine this is one extra permission term: *"or there is an unexpired, unrevoked share on this record for
the caller"*. Lists use the same term as a filter, so a shared record appears in lists and searches without a special
case. Field rules still apply: sharing a record does not reveal fields the recipient's roles hide (a share can carry an
explicit `fields` allow-list to narrow further).

## Who can share

A person who may `read` a record may share `read`; to share `write` they must hold `write`. A model opts in:

```json
{ "model": "quote", "sharing": { "enabled": true, "roles": ["sales"], "max_days": 90, "links": true } }
```

so nothing is shareable by default. Revoking is one call and takes effect on the next request. The record page lists its
shares (who, level, expiry), as does the recipient's "Shared with me" view.

## Link shares

A link share has a random key (stored hashed) and gives anonymous access through the existing visitor rules of
[Access](access.md): the visitor gets read access to that record only, through a page the plugin declares `public`, with
rate limits and audit rows naming the link. Options: expiry, a view limit, an optional passcode, "download allowed".
Opening a link is audited with the visitor identity; the owner sees opens on the record.

## Commands

| Command | Capability | Does |
|---|---|---|
| `sharing::share { model, id, subject, operations, expires_at }` | `sharing::manage` | create a share |
| `sharing::revoke { share }` | `sharing::manage` | end it now |
| `sharing::list { model, id }` | `sharing::manage` | shares on a record |
| `sharing::link { model, id, options }` | `sharing::manage` | create a link, returns the URL once |

Notifications: the recipient gets a [notification](notifications.md); the owner is told when a link expires.

## Interaction with other proposals

- [History](history.md): shares are not versioned data, but each create/revoke is audited.
- [Verification](verification.md): the access matrix lists sharing as a *possible* extra path for models where it is
  enabled, so a review sees that path.
- [Federation](federation.md): a share to someone outside the organization is a federation exchange, not a link.

## Settings

`sharing.max_days`, `sharing.links_enabled`, `sharing.link_requires_passcode`, `sharing.notify`.

## Tables

`record_shares`, `share_opens` (link key, time, visitor, ip hash).

## Not in scope

Folder or hierarchy sharing ("share everything under this project"; the `under` filter makes this possible later),
share-by-email invitations for people without an account (use a link or an [account invite](magic_links.md)).

## Build order

1. Table and the extra permission term, list filter included.
2. User and role shares, UI on the record page.
3. Link shares with expiry and audit.
4. Passcode, view limits.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Is sharing a separate permission system? | **No.** A share is a rule term, same engine. |
| 2 | Opt-in per model | **Yes.** |
| 3 | May a share exceed the sharer's own access? | **No.** |
| 4 | Do field rules still apply to a shared record? | **Yes**, plus an optional narrower field list. |
