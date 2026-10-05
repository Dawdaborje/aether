# The scheduler: background jobs and recurring tasks

Anything that should not make a person wait (sending an email, building a report, calling a slow
API) or should happen on a timetable (a nightly export, a sweep every 15 minutes) is the
scheduler's job. It does what Celery does in a Django project: a plugin puts work on a queue and
returns at once, a worker picks it up, and failures are retried. It also runs cron.

## What it runs

A **job** is one of:

| Kind | What it is | Made by |
|---|---|---|
| `plugin` | a call to one of the plugin's own functions, with a JSON payload | `scheduler::enqueue`, or a recurring task when it is due |
| `communication` | one email or SMS to one recipient, delivered through the configured bridge | `communication::send` (see [Communication](communication.md)) |

Jobs run as the kernel (`system:scheduler`), not as whoever started them, with the plugin's own
capabilities and model grants. They are recorded in the plugin call audit under the request id
`job-<id>`. A **recurring task** does nothing itself: when it is due it enqueues a job, so
scheduled work gets the same retries, queues and limits as everything else.

## For plugin authors

```toml
# plugin.toml: tasks that always exist
[[schedule]]
name = "nightly-report"
function = "send_report"
cron = "0 2 * * *"              # five fields: minute hour day month weekday
timezone = "Africa/Lagos"        # optional; UTC otherwise
payload = { kind = "daily" }     # optional input for the function
# every = "15m"                  # instead of cron: 30s, 15m, 6h, 1d (at least 10 s)
# queue = "slow"                 # default: "default"
# max_attempts = 3
# catch_up = "once" | "skip"     # after downtime: run once for what was missed (default), or wait
```

```rhai
// from a function: run `export` in the background, in an hour, at most once per invoice
scheduler::enqueue("export", #{ invoice: id }, #{ delay_secs: 3600, unique_key: "export-" + id });
scheduler::register(#{ name: "sweep", function: "sweep_stale", every: "30m" });
```

| Command | Capability | What it does |
|---|---|---|
| `scheduler::enqueue { function, payload?, delay_secs?, queue?, max_attempts?, unique_key? }` | `scheduler::enqueue` | queue a job; answers `{ id, created }` |
| `scheduler::job { id }` | `scheduler::enqueue` | the job's state (`queued`, `running`, `succeeded`, `failed`, `cancelled`), attempts and last error |
| `scheduler::cancel_job { id }` | `scheduler::enqueue` | cancel a job that has not started |
| `scheduler::register { name, function, cron \| every, timezone?, payload?, queue?, max_attempts?, catch_up? }` | `scheduler::register` | add or update a recurring task (up to 100 per plugin) |
| `scheduler::cancel { name }` | `scheduler::cancel` | remove a task registered while running; manifest tasks cannot be removed this way |

A job calls one of **the plugin's own** functions. To involve another plugin, that function calls
it with `plugins::call`. A payload is a JSON object of at most 64 KiB. A plugin may have 10 000 jobs
waiting at once.

## Delivery guarantee: at least once

A job that finishes is recorded as finished. A job whose worker dies (the process is killed, the
machine goes away) is not lost: the worker only *leases* a job (120 s by default), and when the lease
runs out another worker takes the job and runs it again. The price is that a job can run **more than
once**, for example if the worker dies just after sending an email but before recording it.

So write background functions to be safe to repeat, and use `unique_key` to stop the same work being
queued twice (`"welcome-email-user-42"`). The kernel's own email and SMS jobs are one recipient each,
so a retry only repeats the one message that did not complete.

A worker that comes back after its lease ran out and reports success or failure is ignored: the job
belongs to whoever holds it now.

## Retries

A failing job is tried again after a delay that doubles each time, from `scheduler.retry_backoff_secs`
(30 s) up to six hours, for `scheduler.max_attempts` attempts (3), then it is marked `failed` and
kept for `scheduler.failed_job_retention_days` (30). A failure that cannot be fixed by trying again
(the plugin or function does not exist; a provider rejected the message or the credentials) fails at
once. Successful jobs are deleted after `scheduler.job_retention_days` (7).

## Where it runs

The scheduler is one engine that can run in two places.

**Inside the server (the default).** `aether --serve` starts a scheduler in the same process. Nothing
else is needed. `aether --serve --no-scheduling`, or `embedded = false` under `[scheduler]`, turns it off.

**On its own, for scale.** `aether --start-scheduler` runs only the scheduler, with the same code. You
can run several, on several machines, and give each its own queues (`queues = ["email"]`). They share
jobs safely. It needs:

* the same database;
* the same `app_dir` (the plugin files live there; mount it, or share it as a volume);
* the same secret key (below);
* the **Redis cache backend** if plugins use `cache::*` from background jobs and from requests: the
  default in-process cache is private to each process, so a value a job caches is not seen by the
  HTTP server.

**How the two find each other.** Every scheduler keeps a row in the core table `scheduler_nodes` and
refreshes it every sweep. The HTTP server reads that table and learns where a standalone scheduler's
control API is, so nothing about it is written in `aether.toml`.

* The server sends it small hints: `wake` right after a job is enqueued, `reload` when plugins change,
  so work starts in milliseconds instead of at the next look.
* Hints are an optimization. A scheduler that never receives one finds everything on its next sweep
  (`poll_secs`, 5 s by default), because **the database is the source of truth**: an install made with
  the command line while the scheduler was down is picked up the same way.
* An embedded scheduler **stands down** while a standalone one is alive and **takes over** within one
  sweep after it stops (a standalone scheduler that stops cleanly removes its row at once; one that
  crashes is considered gone 30 s after its last heartbeat). Even a brief overlap cannot run a job
  twice at the same time, because claiming is atomic.

### The control API

A standalone scheduler listens on `[scheduler] bind` (default `127.0.0.1:7895`, or the address given
to `--start-scheduler`). Every request needs `Authorization: Bearer <token>`; the token is the
secret setting `scheduler.control_token`, created automatically, so every process that shares the
database and the secret key knows it. If the API must be reachable from other machines, bind it to a
private network address; the server is told the address through the database.

| Request | Effect |
|---|---|
| `GET /control/status` | the live scheduler nodes and job counts per organization |
| `POST /control/wake {"org": "…"}` | look at this organization's queue now |
| `POST /control/reload` | look at everything now (plugins or schedules changed) |
| `POST /control/pause`, `POST /control/resume` | stop or resume claiming work in this process |
| `POST /control/run {"org","plugin","task"}` | run a recurring task now, in addition to its schedule |

## How claiming works

Each organization's database has a `jobs` table. A worker claims jobs in a single transaction that
selects the due ones (`queued` and past `run_at`, or `running` with an expired lease and attempts
left) and updates them to `running` with its own name and a lease, re-checking each job's state in the
update. Two workers racing for the same job cannot both win; the loser's transaction is refused and it
asks again. A test starts two workers against 30 jobs and checks that none is taken twice.

A recurring task is moved to its next slot only if it is still at the slot that was fired, and the job
it enqueues has the key `task:<name>:<slot>`, so even if two schedulers fire the same slot, one job
exists.

After downtime, `catch_up = "once"` runs a task one time for everything it missed and then resumes
from the present; `"skip"` runs nothing for slots that are more than a few minutes late.

## Settings and configuration

Behaviour that an administrator tunes is in the **settings** (global, with an organization override),
group *Scheduler*:

| Setting | Default | |
|---|---|---|
| `scheduler.enabled` | on | an organization can turn it off to pause its jobs and tasks; they wait |
| `scheduler.max_attempts` | 3 | attempts before a job fails |
| `scheduler.retry_backoff_secs` | 30 | first retry delay; doubles each time |
| `scheduler.job_retention_days` | 7 | keep successful jobs |
| `scheduler.failed_job_retention_days` | 30 | keep failed and cancelled jobs |

How a *process* runs is in `aether.toml`:

```toml
[scheduler]
embedded = true            # run the scheduler inside `aether --serve`
concurrency = 8            # jobs this process runs at once
poll_secs = 5              # how often it looks for due work without being told
lease_secs = 120           # how long a worker holds a job before another may take it
queues = []                # queues to serve; empty means all
bind = "127.0.0.1:7895"    # where a standalone scheduler's control API listens
```

A job is stopped if it runs longer than `lease_secs` minus a few seconds; plugin calls are also bound
by the plugin runtime's own limits (10 s and 2 million operations for Rhai scripts).

## Tables

In every organization database (migration `018_scheduler`): `scheduled_tasks` and `jobs`. In the core
database (migration `023_scheduler_nodes`): `scheduler_nodes`. The plugin catalog stores each
plugin's `[[schedule]]` entries in `plugins.schedules`, and installing or upgrading a plugin in an
organization copies them into that organization's `scheduled_tasks` (removing ones the new version
dropped; tasks a plugin registered while running are left alone). Plugins cannot name these tables as
models.
