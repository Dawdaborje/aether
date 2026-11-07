---
title: "Auth hardening: second factor, passkeys, SAML"
description: "Sign-in today is a local password, a magic link, or an identity provider through an auth bridge (LDAP, Keycloak, Auth0, Authentik, Microsoft)."
section: architecture
status: proposal
order: 60
---
# Auth hardening: second factor, passkeys, SAML (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

Sign-in today is a local password, a [magic link](magic_links.md), or an identity provider through an auth bridge
(LDAP, Keycloak, Auth0, Authentik, Microsoft). All three paths end in `create_session`
(`facets/orm/src/services/sessions.rs`). That single choke point is where a second factor belongs, so it covers
every way in at once.

## Step-up inside the session

`create_session` gains a state. A session starts as **`pending`** when the user (or the organization's policy)
requires a second factor and none has been given; a pending session can reach only `/api/auth/mfa/*` and `logout`.
When the factor passes, the session becomes **`full`**. Nothing else in the kernel changes: every extractor
(`AuthSession` in `facets/authentication/src/authz.rs`) already refuses what is not `full`.

```
password / magic link / OAuth  ->  create_session(pending?)  ->  /api/auth/mfa/verify  ->  full
```

## Factors

| Factor | Notes |
|---|---|
| **Passkey (WebAuthn)** | Phishing-resistant; first choice. Resident keys allow passwordless sign-in. |
| **TOTP** | Authenticator app; the secret is encrypted with `secrets.rs`, a 30 s step, one-use window to stop replay. |
| **Recovery codes** | Ten single-use codes shown once, stored hashed. |
| **Email/SMS code** | Via [Communication](communication.md); weakest, off by default. |

A user enrolls factors on their profile page; removing the last factor needs a fresh sign-in with a factor.
Enrollment, use, failure and removal each write an audit row.

## Policy

Per organization (and a global default), settings the organization owner can set:

- `auth.mfa.policy`: `off`, `optional`, `required_for_roles` (e.g. `org_admin`, a role list), `required`.
- `auth.mfa.grace_days`: how long a new member may postpone.
- `auth.mfa.remember_days`: trusted-device cookie (hashed, bound to user agent), `0` disables.
- **Step-up for sensitive actions**: a plugin or the kernel can ask for a *recent* factor (within N minutes) before
  an action: changing a role, exporting data, restoring history, revoking keys. `ctx.require_recent_mfa(300)`.

Brute force: attempts per user and per IP are limited (reusing the visitor rate limits of
[Access](access.md#rate-limits)); repeated failure locks the factor, not the account, and notifies the owner.

## Identity providers

- **SAML 2.0** is added as an auth bridge under `bridges/auth_and_identity/saml`, implementing the `AuthBridge`
  trait like the existing ones. It supports SP-initiated sign-in, signed assertions, and attribute-to-role mapping.
- **Per-organization provider config.** `resolve_bridge` in `facets/authentication/src/bridges.rs` currently ignores its
  database argument, so providers are installation-wide. It will read the organization's `bridge.<name>.*` settings
  (the all-or-nothing rule of [Bridges](../bridges.md) applies), so each company can bring its own identity provider.
- **Group and role mapping**: provider groups map to organization roles by a rule table the owner edits; unmapped
  users get a default role or are refused (a setting).
- **Just-in-time accounts** and **SCIM provisioning** (create, disable, delete members from the provider) are
  supported by the same mapping; disabling at the provider ends sessions at the next check.
- When the provider already did a second factor (`amr` claim), the organization may accept it instead of asking again.

## API tokens and MFA

[Tokens](api_and_webhooks.md) are the answer for machines: they never go through a second factor, and creating or
raising one requires a recent factor.

## Commands and endpoints

Browser endpoints (`/api/auth/mfa/enroll`, `/verify`, `/recovery`, `/devices`). Plugin-visible: `auth::require_recent_mfa`
(capability `auth::step_up`), `auth::mfa_state { user }` (capability `auth::read`).

## Tables

`user_factors` (type, encrypted secret or credential id and public key, label, last used), `recovery_codes`
(hash, used_at), `trusted_devices`, session column `mfa_level` and `mfa_at`.

## Not in scope

Hardware token management for administrators, risk-based "impossible travel" scoring, passwordless-only mode in the first
version (passkeys are an additional way in).

## Build order

1. Session state and `/mfa/verify`, with TOTP and recovery codes.
2. Policy settings and step-up call.
3. WebAuthn.
4. Per-organization provider config, SAML, role mapping.
5. SCIM.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Where is the second factor enforced? | **In `create_session`** (one place for all sign-in paths). |
| 2 | Default policy for new organizations | **`optional`**, with `required_for_roles: [org_admin]` pre-filled. |
| 3 | Recommend SMS codes? | **No**, off by default. |
| 4 | Provider config scope | **Per organization**, falling back to the global provider. |
| 5 | SCIM in the first release? | **No**, after SAML. |
