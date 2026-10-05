# File watches: telling a plugin that a file arrived

Plugins often need to react to files: a bank statement dropped by an accountant, a CSV exported by
another system, a scan from a copier. Instead of polling, the kernel listens to the folder with the
operating system's own file notifications (**inotify** on Linux; the equivalent mechanism on macOS and
Windows) and runs one of the plugin's functions when something happens. Nothing is read from the
folder until something changes.

## A plugin's folder

Every plugin has a folder per organization on the server's local disk:

```
<app_dir>/orgs/<organization>/plugins/<plugin>/
```

It is where files are dropped and where the plugin works. Paths are always **relative to this folder**.
A plugin cannot name or reach anything outside it: `..`, absolute paths, backslashes, control
characters and symbolic links that lead out of the folder are all refused, by the watcher and by the
`fs::*` commands alike. Another plugin's folder, the organization's other files and the rest of the
machine are out of reach. (For files that belong to the organization and may live in S3, use
`storage::*`; this folder is always local, because inotify only exists on a local disk.)

## Declaring a watch

```toml
[[watch]]
name = "invoices"             # lower-case letters, digits, _ and -
path = "inbox"                # relative to the plugin's folder; "" is the folder itself
pattern = "*.csv"             # optional glob; `*` stays in one folder, `**/*.csv` goes into subfolders
events = ["created"]          # created, modified, deleted; default: created and modified
function = "import_invoice"   # the plugin function to run
debounce_ms = 500             # how long a file must stay quiet first (50 to 60000); default 500
recursive = false             # also watch subfolders
catch_up = true               # on start, report what arrived, changed or went away while nothing listened (default)
poll_secs = 10                # look every 10 s instead of listening: for network file systems
queue = "default"             # the queue its jobs go to
max_attempts = 3
```

The function receives:

```json
{ "watch": "invoices", "path": "inbox/2026/a.csv", "event": "created", "size": 1834, "modified_ns": "1790000000000000000" }
```

`path` is relative to the plugin's folder, ready to hand to `fs::read`. A deletion has no size or time.
A manifest with a bad watch (a path that leaves the folder, an unknown event, a broken glob, a debounce
out of range) is refused when the plugin is loaded.

```rhai
fn import_invoice(change) {
    let csv = fs::read(change.path);
    if csv == () { return #{}; }                       // already gone
    // ... parse and store the rows ...
    fs::rename(change.path, "done/" + change.path.sub_string(6));   // handled; move it away
    #{ imported: 12 }
}
```

## What the plugin is told, and when

* **Quiet first.** Copying a file produces a burst of events. Events for one path are merged and
  reported once the path has been quiet for `debounce_ms`, so a file that is still being copied is not
  reported half written. `created` then `modified` is one `created`; `created` then `deleted` is nothing;
  `deleted` then `created` is `modified`. A rename into the folder is a `created`; out of it, a `deleted`.
* **Not reported:** names starting with `.` or ending in `~`, `.part`, `.tmp`, `.swp`, `.crdownload` or
  `.partial` (what editors and downloaders write while working), directories, and anything that
  resolves outside the plugin's folder. The kernel's own `fs::write` writes under a dot-name and renames
  into place, so a file written by a plugin appears complete.
* **A job, not a call.** Each settled change becomes a background job (see
  [the scheduler](scheduler.md)): it is retried if the function fails, runs as the kernel with the
  plugin's own permissions, and is recorded in the plugin call audit. Its key is the watch, the event,
  the path, the size and the modification time, so the same change is never queued twice, even when
  several scheduler nodes watch the same folder.
* **At least once.** Write the function so that handling a file twice is harmless, and move processed
  files away (`done/`) so a restart does not find them again.

## Where it runs

The watches run **on the scheduler**: wherever jobs are run (inside `aether --serve`, or in
`aether --start-scheduler`), and only on the node that is actually running jobs. An embedded scheduler
that stands down because a standalone one is alive stops its watches, and takes them up again if the
standalone one stops. A paused scheduler stops listening too. So the folder must be on a disk the
scheduler can see: the same `app_dir` as the server.

Each scheduler sweep (every `poll_secs`, 5 s by default) compares the `watches` table with what is
listened to and starts, replaces or stops listeners. Installing, upgrading or disabling a plugin
therefore takes effect within a sweep, and an install made from the command line is found the same
way. The folder is created when the watch starts.

## Limits and warnings

* **Missed events (catch-up).** A listener only sees changes while it runs, so the kernel remembers what
  each watch has reported: path, size and modification time, in the organization's `watch_seen`. When a
  scheduler node starts listening it compares the folder with that memory and reports, as ordinary
  events, the files that are **new** (`created`), **changed** (`modified`) or **gone** (`deleted`) since,
  filtered by the watch's `events` and `pattern`. The first time a watch ever starts, every matching file
  already in the folder is new. A file that appeared and went again while nothing listened is never
  reported. The memory is updated only after a change is safely queued, so a database outage cannot make
  the kernel forget a file; and it is the kernel's, so nothing depends on the plugin moving files away
  (although doing so is still a good way to see what has been handled). `catch_up = false` turns it off:
  the watch then reacts only to live changes. Changes the watch did not ask about (a `modified` when it
  wants only `created`) are not sent to the plugin but are remembered, so a later catch-up is correct.
* **Network file systems** (NFS, SMB, some cloud mounts) do not deliver notifications. Use `poll_secs`.
* **inotify limits.** Linux limits the folders one user may watch (`fs.inotify.max_user_watches`).
  Watching a very large tree with `recursive = true` can reach it; the error is logged with the
  setting to raise.
* **Large files** are reported when they stop changing; a writer that pauses for longer than
  `debounce_ms` mid-copy can be reported early. Raise `debounce_ms`, or have the writer rename the
  finished file into place (names ending in `.part` and `.tmp` are ignored).

## The file commands

| Command | Capability | |
|---|---|---|
| `fs::read { path, encoding? }` | `fs::read` | `{ text }` or, with `encoding: "base64"`, `{ base64 }`; `null` if there is no such file; at most 10 MiB |
| `fs::write { path, text \| base64 }` | `fs::write` | creates folders, replaces the file, appears complete; at most 10 MiB |
| `fs::list { path? }` | `fs::list` | entries `{ name, path, is_dir, size, modified_ns }`, at most 1000, not looking into subfolders; links that leave the folder are not listed |
| `fs::stat { path }` | `fs::read` | `{ is_dir, size, modified_ns }` or `null` |
| `fs::rename { from, to, overwrite? }` | `fs::write` | moves inside the folder, creating the destination's folders; refuses to replace unless `overwrite` |
| `fs::delete { path }` | `fs::delete` | a file or an empty folder; deleting what is not there is not an error; never the whole folder |

In Rhai they are `fs::read`, `fs::read_base64`, `fs::write`, `fs::write_base64`, `fs::list`, `fs::stat`,
`fs::rename` and `fs::delete`; the Rust SDK has a `files` module with a `Changed` type for the
function's input.

## Tables

`watches` and `watch_seen` (what each watch has reported) in every organization database (migrations `019_watches` and `020_watch_seen`), `watches` filled from the catalog's
`plugins.watches` (core migration `025_plugin_watches`) when a plugin is installed or upgraded. Plugins
cannot name it as a model.
