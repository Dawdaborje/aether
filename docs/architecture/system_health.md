---
title: "System health"
description: "The developer desk page and API that show whether the database, scheduler, plugin runtime and cache are working, with a status and a sentence for each."
section: architecture
status: stable
order: 265
---
# System health

Developers open **`/desk/health`** (desk nav, developers only) to see whether the server is
working, without reading logs. The page refreshes every 10 seconds while its tab is visible.

## What it checks

Each area gets a status, `ok`, `warn` or `error`, and a sentence saying why. The page headline
is the worst of them.

| Area | `error` when | `warn` when |
| --- | --- | --- |
| Database | the core database does not answer a trivial query | it takes 250 ms or more |
| Scheduler | no scheduler node has reported in during the last 30 seconds | the scheduler is paused |
| Plugin runtime | – | plugins have timed out waiting to compile, been turned away because the queue was full, or been refused, since start-up |
| Cache (Mantle) | – | 90% or more of the cache budget is in use |
| Mode | – | development mode is on (session cookies are sent without the `Secure` flag) |

The counters for the plugin runtime and the cache are since the process started, so a warning
that stays after a one-off burst clears when the server restarts. The cache and plugin
runtime details also appear on the [Mantle](mantle.md) page and in the runtime stats.

## API

`GET /api/desk/health` (a signed-in developer only) returns `status`, the list of `checks`
(`id`, `label`, `level`, `detail`), and the numbers behind them: `server` (version, uptime,
development mode), `database` (answering, response time, organization count), `scheduler`
(paused, the nodes that reported in), `plugins` (the runtime stats) and `cache`.

The call runs one trivial query and reads counters held in memory; it does not count jobs
across organizations, so it stays cheap on a platform with many of them.

## Not covered yet

The page does not show per-organization job backlogs, rate-limit blocks, media storage or
the audit trail. The scheduler and the limiters keep their counters per process, so a
multi-node installation would need each node to report its own.
