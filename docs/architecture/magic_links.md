# Magic links

Single-use tokens for auth and plugin workflows. Rows live in core `magic_links`
and are **never hard-deleted**: consume sets `used_at`, supersede/revoke sets
`revoked_at`.

## Kernel commands

| Command | Capability | Role |
|---|---|---|
| `magic_link::create` | `magic_link::create` | Issue a link; returns `{ id, url, token, expires_at }` once |
| `magic_link::revoke` | `magic_link::revoke` | Soft-revoke by id or purpose+user |
| `magic_link::inspect` | `magic_link::inspect` | Metadata without consuming |

Plugins may only create purposes under `<plugin>.…`. The host may create `auth.*`.

```json
{
  "purpose": "approvals.decide",
  "user_id": "users:…",
  "email": "optional@x.com",
  "ttl_secs": 3600,
  "payload": { "record": "…" },
  "path": "/approvals/magic",
  "supersede": true
}
```

## Built-in purposes

| Purpose | Default TTL | Consume |
|---|---|---|
| `auth.password_reset` | 60 minutes | `POST /api/auth/magic-link/consume` with `new_password` |
| `auth.login` | 15 minutes | Same endpoint; starts a session |
| `auth.account_invite` | 7 days | Same endpoint with `new_password`; creates the account and joins `org_db`. Issued only by `org::invite`, not by `magic_link::create` |

Other purposes queue jobs for subscribers of `<plugin>.magic_link_consumed` in the
organization and return the payload to the HTTP client.

## HTTP

- Consume: `POST /api/auth/magic-link/consume` `{ token, new_password? }`
- Password-reset and account-invite UI: `/login/magic?token=…`
- Admin send-reset: `POST /api/ui/organizations/{db}/members/{user_id}/password-reset`
  (owner or developer)
