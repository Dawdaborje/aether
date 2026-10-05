# Running Aether

## Starting and stopping

`aether --serve` needs an initialized database (`aether --init` once; it
reports clearly if you forgot).

Ctrl+C or SIGTERM shuts it down gracefully: it stops accepting connections,
lets in-flight requests finish (for up to 10 seconds, since open event streams (SSE)
would otherwise hold it open), stops background tasks, signs the
database session out and exits with status 0. All requests share one websocket
to SurrealDB, which closes when the process exits.

## The developer account

The account `--init` creates is the **developer**: it bypasses permission
checks, lands on the desk (the Organizations page) after login, and is the only
account that sees the desk and the developer tools (`/studio`, more to come); see
[organizations.md](organizations.md). It is stored as a superuser
(`is_super_user`); the API reports it as `is_developer` too.
