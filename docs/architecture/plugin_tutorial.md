---
title: "Plugin author tutorial"
description: "A guided, build-it-yourself path that takes a new author from an empty folder to a working plugin with a model, pages, a rule and a test, in one sitting."
section: architecture
status: proposal
order: 435
---
# Plugin author tutorial

**Status: proposal, decisions accepted. Nothing here is written.** The choices in the last section are settled. This closes gap G1 of
[Guided tours](guided_tours.md).

Today an author learns by reading reference pages ([Plugin documentation](../plugin/README.md), [Pages](../plugin/pages.md),
[Models](models.md), [Rules](rules.md)) and the sample plugins under `plugins/test`. [Get started](../get_started.md) is
an empty stub. Reference answers "what does this attribute do"; it does not answer "what do I do first, and in what
order". A tutorial is the missing path: one small plugin, built in short chapters, where each chapter ends with
something that runs.

## Who it is for

A developer who can read a config file and a little code, has never seen Aether, and has one to two hours. They do not
need Rust or WebAssembly: the tutorial uses a script language so there is nothing to compile. Authors who want a
compiled language get a closing chapter that points at the SDKs.

## The example: a small team task list

One plugin, `tasks`, grown chapter by chapter. It is small enough to finish and uses every common building block:
a model, a list and a form, a detail page, a state change, a rule, a permission and a scheduled job. It names no real
plugin and depends on none, so it makes sense with a bare kernel.

## Chapters

Each chapter has: a goal in one sentence, the files to create, the command that proves it works, and "what you just
learned" with links to the reference page. Every chapter starts from the previous chapter's result, so a reader can
also start at any chapter from a provided snapshot.

| # | Chapter | The reader ends with | Reference it links |
|---|---|---|---|
| 0 | **Set up** | `aether` installed, a database running, a workspace folder, the dev server open in a browser | Prerequisite, Running |
| 1 | **Hello, plugin** | `aether --gen plugin` output read file by file; `plugin.toml` explained; the plugin installed and visible in Apps | Plugin documentation |
| 2 | **A model** | A `task` model (title, status, due date, assignee) with ids from the tool, not invented | Models |
| 3 | **A list and a form** | A page that lists tasks and a form that creates one | Pages, Components |
| 4 | **A detail page** | Open a task, edit it, see its history | Pages, History |
| 5 | **Permissions** | `access_models`, a role that can read but not write, and what a person without it sees | Access |
| 6 | **A rule** | A workflow `open → doing → done`, and a field that locks when done | Rules |
| 7 | **Code** | A command in the script that counts overdue tasks and a button that calls it | Rhai or Lua, Commands |
| 8 | **Reacting** | An event on `task.done` that notifies the assignee; a scheduled job for overdue tasks | Events, Scheduler, Notifications |
| 9 | **Translate and theme** | `i18n/` in two locales; the plugin looking right under another theme | Localization, Themes |
| 10 | **Check and ship** | `aether --load-plugins --dry-run` clean, a test, a package and an install into a second organization | Verification, Plugin loading |
| 11 | **Where next** | Compiled languages, the SDKs, the MCP server, workspaces, a guided tour for the plugin | SDK, MCP, Guided tours |

Chapters 0 to 3 are the "five minute win": a reader who stops there still has a working list and form. Chapters 4 to 9
are independent of each other after chapter 3, so a reader can skip to the one they need.

## Rules for writing it

- **Every snippet is real.** Code blocks are included from the files of the tutorial plugin, not typed into the prose.
  A snippet that does not exist in the plugin cannot appear in the page.
- **It is tested.** The tutorial plugin has one snapshot per chapter. CI runs `aether --load-plugins --dry-run` on each, and the end
  state runs in the e2e suite. When a kernel change breaks a chapter, the build fails, so the tutorial cannot rot.
- **Ids come from the tool.** The text tells the reader to ask `aether` (or the `new_ids` MCP tool) for model and field
  ids, never to invent them, matching [Models](models.md).
- **Show the failure too.** Each chapter includes one thing that goes wrong on purpose (a validate error, a denied
  write) and how to read the message. Authors spend most of their time there.
- **One concept per chapter,** no forward references. If chapter 6 needs something from chapter 8, split it.
- **Plain pages, no marketing.** Short steps, expected output shown, a "stuck?" box with the top three causes.
- **No other plugin named,** except as a short example, per the documentation rule in the repository.

## Where it lives

- **Pages:** `docs/plugin/tutorial/` with `README.md` (the map above) and `01-hello.md` ... `11-next.md`. It joins the
  docs site and the MCP index automatically, so an LLM client reading `search_docs` finds it too.
- **Code:** the tutorial plugin and its chapter snapshots live with the sample plugins in `plugins/test/` (for example
  `plugins/test/tutorial/ch01 ... ch10`), because the kernel repository holds no plugin.
- **Get started** becomes a half-page: install, run, then "Next: the tutorial".
- **Guided tours link-up:** when [Guided tours](guided_tours.md) are built, chapter 11 adds a tour to the tutorial
  plugin, which doubles as the worked example that tours' build step 6 asks for.

## Build order

| Step | Deliverable |
|---|---|
| 1 | Prove the path: build the final `tasks` plugin once, in the chosen script language, noting every point of friction. Friction found here is a kernel or docs bug to fix first, not to paper over |
| 2 | **Done (see Build step 2).** Chapters 0 to 3 with snapshots and the check; Get started rewritten |
| 3 | **Done (see Build step 3).** Chapters 4 to 7 |
| 4 | **Done (see Build step 4).** Chapters 8 to 11 |
| 5 | **Done (see Build step 5).** A read-through as a newcomer; every stall fixed |
| 6 | **Done (see Build step 6).** The same plugin in Rust as an appendix |

## Decisions (accepted)

| # | Question | Decision |
|---|---|---|
| 1 | Which language does the tutorial use? | **Rhai.** Nothing to compile or install, and it is the shortest route to a running plugin. Lua is the same shape; a chapter note shows the difference. Checked: Rhai exposes `events::subscribe`, `events::emit`, `scheduler::enqueue` and `scheduler::register` ([Rhai](../plugin/rhai.md)), so chapters 7 and 8 are covered. Any gap found while building step 1 is recorded, not hidden |
| 2 | One long page or many chapters? | **Many short chapters** with a map page; each ends with something that runs |
| 3 | Where do the tutorial plugin's files live? | **`plugins/test/tutorial/`**, next to the other samples, with one snapshot folder per chapter |
| 4 | Is the tutorial tested in CI? | **Yes.** `aether --load-plugins --dry-run` on every snapshot, e2e on the end state. An untested tutorial goes stale within a release |
| 5 | Video in the tutorial? | **Not at first.** Short clips for the two or three hardest gestures (the form designer, the workflow editor) once the text is stable |
| 6 | Interactive in-browser sandbox? | **No.** Out of scope; the tutorial runs on the reader's own machine |
| 7 | Translate the tutorial? | **Later.** English first; the docs site decides its own localisation |

## Build step 1: findings

The finished `tasks` plugin is in `plugins/test/tutorial/tasks` (model, workflow and lock rule, three pages, a command,
an event with a listener, a daily schedule). `validate_plugin` reports it VALID, and it was loaded, installed and
called over HTTP against a throwaway in-memory database (debug build of 2026-11-01), as the administrator, as a plain
member and as a member holding the `lead` role. **Not checked:** how the three pages render in the browser, and where the
notification from `notify::send` shows up.

| # | Finding | Result | Effect on the tutorial |
|---|---|---|---|
| F1 | `"track": true` on a field is refused unless the model's chatter has `track_changes` | Confirmed by the validator | Chapter 6 leaves tracking out; a later box can add chatter |
| F2 | A plugin listening to its **own** event (`tasks.task_done`) | **Works.** The handler ran as a background job each time a task was finished | Chapter 8 can emit and listen inside one plugin; fix the events reference, which says the emitter must be a dependency |
| F3 | Role spelling: `tasks.lead` in the lock rule, `lead` in the workflow | **Both work.** A member granted `tasks.lead` could reopen and edit a done task | Teach the bare name inside the plugin's own files, the full name in `--grant-role` |
| F4 | `context::get().now` with `sub_string(0, 10)`, `map.remove("id")`, `db::find` filters with `lt` and `ne`, `fail(...)`, `notify::send` fields | **All ran without error.** `count_overdue` and `mark_overdue` found and flagged the overdue task. Where the notification appears was not looked at | Safe to use in chapters 7 and 8 |
| F5 | The `overdue` flag could be a computed filter | Kept, as it shows a job writing data | Mention the alternative in the chapter |
| F6 | **Setup.** A fresh install is in single-organization mode: `aether --init` already creates the organization `main` and its administrator, | Hit while running | Chapter 0 must use `--org main` and the `--init` administrator, not `--create-org`. The sample setup scripts in the repository still assume several organizations |
| F7 | **Refusals looked like crashes.** A workflow or rule refusal inside a script reached the caller as `502 plugin invocation failed`; the readable text was only in the server log | **Fixed in the kernel** (`plugin_manager/script/mod.rs`): a refusal by a record, field or workflow rule now reaches the caller as a 422 with the message, like `fail`. Covered by unit tests; not yet re-run end to end, because that needs a rebuilt `aether` binary | Chapter 6 shows the message the member sees. `try`/`catch` is still the way to change it. Other failures (bad payload, missing capability) stay internal |
| F8 | **The administrator skips every rule.** As `admin`, `open → done` and editing a done task both succeeded | Matches the rules reference (`org_admin` is exempt) | Chapters 5 and 6 must be tried as a plain member, and say why the administrator sees no effect |
| F9 | Declaring `[[roles]]` closes role-less pages to members without one of the plugin's roles (from earlier project notes) | Not tested here | Check in step 2 with `bob`; likely needs a sentence in chapter 5 |

The setup used: `surreal start memory` on its own port, `aether --init`, `--load-plugins` for `base` and for the
tutorial plugin, `--install-plugin base_ui` and `tasks --org main`, `--create-user bob`, then `aether --serve`. Step 2
turns this into a script the CI check can run.

## Build step 2: what was written

- **Pages:** `docs/plugin/tutorial/` holds the map (`README.md`) and chapters 0 to 3. They are in the docs bundle (the
  docs crate builds and its 29 tests pass, which also checks links and forbidden text). [Get started](../get_started.md)
  is now a half-page that points at them.
- **Snapshots:** `plugins/test/tutorial/ch01`, `ch02`, `ch03` (each a full `tasks` plugin) beside the finished `tasks`.
- **The check:** `plugins/test/tutorial/check.py` asks `aether --mcp` to validate every snapshot and the finished
  plugin, and compares every marked code block in the pages with its file (a model is compared without its ids, since
  readers must not copy ids). All four plugins are VALID and the 9 marked blocks match.
- **Replayed:** chapters 0 to 3 were followed command by command in an empty folder against a throwaway database.
  **Not checked:** anything in a browser (signing in, the Apps tile, the list, the form, delete) and `--serve --watch`.
  Those steps are written from the page definitions and need one pass by a person.

What the replay changed in the text:

| # | Finding | Result |
|---|---|---|
| F10 | `aether --version` does not exist | Chapter 0 uses `aether --help` |
| F11 | `--init` fails to load plugins when `plugin_paths` already names a folder that does not exist | The `plugin_paths` step moved from chapter 0 to chapter 1, after the folder exists |
| F12 | A command's `required` argument is refused by the command line before the function runs, so `fail` was never reached | Chapter 2 sends an empty title to show `fail`, and explains the order of checks |
| F13 | The generated plugin also has a `LICENSE` | Chapter 1 lists it |
| F14 | **Kernel gap, not fixed.** A value the model refuses (for example `due=notadate`: "task.due: must be a date like 2026-10-03") still reaches the caller as `plugin invocation failed`; the reason is only in the log. F7 covered rule refusals only | Left out of the chapters. Decide whether model value errors should reach the caller too; they are `invalid payload`, the same class a script bug can produce, so it needs care |

## Build step 3: what was written

- **Pages:** chapters 4 (a detail page and history), 5 (permissions), 6 (a rule) and 7 (code) in `docs/plugin/tutorial/`,
  and the map updated. The docs bundle builds and its tests pass.
- **Snapshots:** `ch04` to `ch07`. The finished `tasks` plugin was rebuilt from `ch07` plus the event, the listener and
  the daily schedule; it no longer depends on `base_ui` (the built-in `<statusbar>` does the same job). 24 marked code
  blocks match their files, and all snapshots are VALID.
- **Replayed** against a throwaway database as the administrator, a plain member (`bob`) and a lead (`lee`), through
  the plugin's functions and the page, chatter and role endpoints: the detail round trip, the history lines (a
  `created` line and a `change` line), the leads-only delete, the workflow moves, the lock on a finished task, the
  reopen by a lead, the overdue list and count, the `tasks.overdue` command, and `finish_task` refusing an open task.
  **Not checked:** anything drawn in a browser (the history panel, the status bar, the row buttons, the page of overdue
  tasks), and the finished plugin's event and schedule after its rebuild.

| # | Finding | Result |
|---|---|---|
| F15 | A page without `roles` returns 403 to a member who holds no role at all (`you do not have permission to open this page`), so a fresh member could not open the pages of chapter 3 | Every page says `roles="member"` from chapter 3; chapter 5 explains why. Consider whether a plugin that declares no roles should open its pages to every member |
| F16 | The replay binary predates the F7 fix, so refusals showed as `502` there and their text only in the log | Chapters 5 and 6 describe the message the member sees, as the fixed kernel gives it. Re-test them on a rebuilt binary |
| F17 | `aether --upgrade-plugin` printed `database error: The table 'plugins' does not exist` twice while a server was running, yet the upgrade took effect and a repeat said "already on 0.7.0" | Not explained, not reproduced on its own. Worth a look |
| F18 | `finish_task` returns a `422` with the function's own text, as `fail` should | Confirmed |

## Build step 4: what was written

- **Pages:** chapters 8 (events and a schedule), 9 (translate and theme), 10 (check and ship) and 11 (where next), and the
  map. The docs bundle builds and its tests pass.
- **Snapshots:** `ch08`, `ch09`, `ch10`. The finished `tasks` plugin is now identical to `ch10` (version 1.0.0, with
  translations, its own documentation and `tests/smoke.sh`). 41 marked code blocks match their files; every snapshot is VALID.
- **Replayed** against a throwaway database: both ways of finishing a task emit the event and the listener runs for each;
  the notification reaches a member; `mark_overdue` flags a late open task; `--load-plugins --dry-run` passes; the smoke
  test prints `ok`; uninstalling keeps the 5 records and installing again picks them up.
  **Not checked in a browser:** the bell, the **Late** column, the Desk schedule page, the plugin documentation page, and
  **Settings → Appearance**. **Not run:** `Upload zip`, and `aether --serve-docs --plugin-docs`.

| # | Finding | Result |
|---|---|---|
| F19 | `i18n::t` was documented for scripts but missing (`Module not found: i18n`) | **Built.** A `translate` hook on the plugin caller, the host command `i18n::t` (no capability), Rhai `i18n::t(key[, params[, options]])` and Lua `i18n.t`. A key nobody translated comes back as the key. Unit tests cover the command's fallback; the translation itself is not yet seen end to end (needs a rebuilt binary). The Rust SDK got it afterwards (F25); the other WebAssembly SDKs are not built. [Localization](localization.md) and the Rhai and Lua pages say so. Chapter 9 uses it for the `fail` messages |
| F20 | `[i18n] default_locale = "fr"` plus a restart did not change the language | **Fixed.** `load_config` never read the `[i18n]` table, so the setting was always `en`. It does now, with a unit test. Not yet seen end to end: needs a rebuilt binary. Also corrected [Localization](localization.md): the person's and the organization's language are not read yet (only the server's, and the browser's for visitors). Chapter 9 now shows the French by changing that setting |
| F21 | The `table 'plugins' does not exist` error from `--upgrade-plugin` (F17) | **Open, not reproduced.** Upgrading through chapters 3 to 10 with a server running, in a clean database, gave no error in any of several runs. It appeared only after test sessions had made HTTP calls |
| F22 | [Notifications](notifications.md) lists `?unread=1`; the server wants `unread=true` | **Fixed** (documentation) |
| F23 | `--uninstall-plugin` keeps the data and says how many records are left, and a reinstall picks them up | Matches chapter 10 |


Also corrected [Events](events.md): a plugin may listen to its own events without a dependency (F2, seen working).

Also fixed after step 4:

| # | Finding | Result |
|---|---|---|
| F14 | A value the model refuses (`due=notadate`) reached the caller as `plugin invocation failed` | **Fixed.** `SchemaError` value and required failures become a new `HostError::Refused`, shown to a script's caller as `task.due: must be a date like 2026-10-03`; a field the model does not have stays internal. The record API maps it like an invalid payload (422). The database tests that expected `InvalidPayload` now accept `Refused`; they were not run (they need a database). Chapter 2 shows it |
| F15 | A page without `roles` returns 403 to a member with no role | **Not a bug.** [Pages](../plugin/pages.md) documents it: no `roles` means a member who holds some role. Changing it would open pages by default, which is a security decision, so it stays. The tutorial uses `roles="member"` and explains why |

## Build step 5: the read-through

Replayed the whole tutorial from an empty folder against the rebuilt binary and a throwaway database, as a newcomer
would: chapters 0 to 3 by their own commands, chapters 4 to 10 by upgrading through the snapshots.

| # | Where a newcomer stalls | Result |
|---|---|---|
| S1 | Chapter 0 and Get started said "open http://localhost:7890 and sign in", but the server serves no screens (`/` answers a JSON 404). The screens come from the separate `web/` project | **Fixed.** Chapter 0 has a new section "The web app" (`npm install`, `npm run dev`, <http://localhost:5173>, `PUBLIC_API_BASE_URL` when the server is not on 7890) and a troubleshooting line; the server check is `/docs`. Get started says the same. **Not run:** `npm install` and the web app itself |
| S2 | The generated `aether.toml` already has `[configuration] is_development_mode = false`; "add a `[configuration]` table" would make a second one | **Fixed.** The text says to change `false` to `true` |
| S3 | A reader whose SurrealDB is already running would start a second one on the same port | **Fixed.** A note says to skip that step |
| S4 | Re-checked with the rebuilt binary: rule refusals answer 422 with the message, the lock on a finished task, lead-only delete, French pages and `i18n::t` messages (F7, F14, F19, F20) | Seen working |

Still not verified: anything drawn in a browser, `Upload zip`, `aether --serve-docs --plugin-docs`, `i18n::t` inside
background jobs, the `facets/core/tests` database suite (F14 widened its matches), and F21.

## Build step 6: the Rust appendix

- **Plugin:** `plugins/test/tutorial/rust/tasks`, the finished plugin in Rust (about 140 lines), with the same name, models,
  rules, pages, translations and commands; only `script = "main.rhai"` became `wasm_file = "out/plugin.wasm"`. It builds
  with `make` to a 204 KB file. It is a second, alternative copy of the model ids, so it lives in its own `rust/` folder.
- **Page:** [Appendix: the same plugin in Rust](../plugin/tutorial/12-in-rust.md) (every code block is taken from the files and
  checked), a differences table, and links from the tutorial map and chapter 11.
- **Checker:** `check.py` validates `rust/tasks` after `make` has built it, and prints `skip` when it has not.
- **Replayed** against a throwaway database: the dry run passes, install works, `hello`, `add`, `list`, `overdue` and
  `mark_overdue` answer as the Rhai plugin, and over HTTP a member gets the same refusals (finish needs `doing`, a finished
  task is locked, only a lead deletes), the `task_done` event reaches the listener and the member gets the notification.

| # | Finding | Result |
|---|---|---|
| F24 | A value the model refuses (`due=notadate`) reached the caller of a Rust plugin as `plugin invocation failed`, while a Rhai caller saw the message (F14) | **Fixed.** The host's error envelope gives `Refused` its own kind, `refused`, with the person's wording and no `invalid payload:` prefix; the Rust SDK turns `refused` (like `denied`) into `Error::Message`, which is shown as a 422. A field the model does not have stays internal. Unit tests in the kernel and in the SDK; seen end to end (`task.due: must be a date like 2026-10-03`). Go, TypeScript and Java SDKs do not read kinds yet, so they see a generic error until they are built |
| F25 | `i18n::t` did not exist in the Rust SDK | **Built.** `aether_sdk::i18n` has `t(key)`, `t_with(key, &params)` and `t_in(key, locale, &params)`, over the same host command as Rhai and Lua (no capability). Seen end to end: the tutorial's Rust plugin answers `a title is required` and, with the server in French, `un titre est obligatoire`. The appendix uses it. The other WebAssembly SDKs are not built |
| F26 | `--watch` reloads on `out/plugin.wasm`, so `make` alone is the development loop | Checked in the watcher's tests, **not** run live |

Not checked: the plugin under `--watch`, and a Go or TypeScript version. The database tests in `facets/core/tests` were not run after F24 (the host error text for a refused value is now the bare reason).
