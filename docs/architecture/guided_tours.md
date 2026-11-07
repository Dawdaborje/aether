---
title: "Guided tours"
description: "Plugins ship short, declarative walkthroughs, with optional video steps, that the kernel shows on a page the first time a person opens it, highlighting real controls and remembering who has seen what."
section: architecture
status: proposal
order: 430
---
# Guided tours

**Status: proposal. Nothing here is built.** Choices are marked *Decide* in the last section.

A new person opening a page for the first time sees a list, a form or a board and has to guess what to do. A plugin
author knows what matters, but today can only write it in a help page nobody opens. A **tour** is a short sequence of
steps the plugin declares next to its pages. The kernel decides when to show it, draws it over the real page, and
remembers who has seen it. The plugin ships data, not browser code, the same rule as [Pages](../plugin/pages.md).

## What a tour looks like

A tour is a file in the plugin: `tours/<name>.xml`.

```xml
<tour name="first_contact" page="/contacts" trigger="first-visit" revision="1">
  <step title="tour.contacts.welcome.title" body="tour.contacts.welcome.body"/>

  <step target="action:create" placement="bottom"
        title="tour.contacts.create.title" body="tour.contacts.create.body"
        advance="click"/>

  <step target="field:email" placement="right" when="form"
        body="tour.contacts.email.body"/>

  <step target="view:switcher" placement="bottom"
        body="tour.contacts.views.body" optional="true"/>
</tour>
```

`title` and `body` are keys in the plugin's `i18n/<locale>.json` ([Localization](localization.md)); a missing key falls
back to the default locale, like everything else a plugin shows.

### Attributes of `<tour>`

| Attribute | Meaning |
|---|---|
| `name` | Unique inside the plugin. The kernel namespaces it, so two plugins can both have `welcome`. |
| `page` | The route the tour belongs to (same syntax as `<page route>`, `{name}` allowed). |
| `trigger` | `first-visit` (default): the first time the person opens `page`. `manual`: only when asked for (help menu, a button). `event`: when a named event fires for the person (see below). |
| `revision` | An integer. Raising it shows the tour again to people who finished an older one. Leave it alone for typo fixes. |
| `requires` | A permission or role the person must hold, for example `requires="contact.create"`. Without it the tour is never offered. |
| `priority` | Breaks ties when several tours are eligible on one page. Higher first. |

### Attributes of `<step>`

| Attribute | Meaning |
|---|---|
| `target` | What to highlight. Omitted: a centred card with no highlight (good for the first and last step). |
| `placement` | `top`, `bottom`, `left`, `right`, `auto` (default). |
| `title`, `body` | i18n keys. `body` may use `{me.name}` and route values. |
| `advance` | `next` (default, a button), `click` (the person must use the highlighted control, then the tour moves on), `event:<topic>` (moves on when that [Realtime](realtime.md) topic delivers). |
| `when` | A page state the step needs, for example `form` (the create or edit form is open). The tour waits for it instead of failing. |
| `optional` | If the target is absent, skip the step silently. Without it, an absent target ends the tour and logs a warning for the plugin author. |

### Targets are names, not selectors

A plugin must not write CSS selectors against the kernel's markup; they would break whenever a theme or widget changes.
Targets are a small vocabulary the web app resolves itself, using the `data-part` and `data-*` attributes that stock
widgets already carry:

| Target | Resolves to |
|---|---|
| `action:<name>` | An `<action>` or row action by name |
| `field:<name>` | A form field (`data-field`) |
| `column:<name>` | A list column header |
| `view:switcher`, `filter:bar`, `search:box` | Fixed parts of a list page |
| `pane:<name>` | A `<pane>` of a custom page |
| `nav:<plugin>` | The plugin's entry in the navigation |
| `tour:<id>` | Any widget carrying `tour="<id>"`, the escape hatch for custom layouts |

Any widget in a page may carry `tour="<id>"`; it adds a stable hook and nothing else.

## Video steps

A step can show a short video or animation instead of, or beside, its text. This is for things a sentence explains
badly: a drag, a multi-click gesture, a before/after.

```xml
<step title="tour.board.drag.title" body="tour.board.drag.body"
      media="tours/drag-card.mp4" poster="tours/drag-card.jpg"
      captions="tours/drag-card.{locale}.vtt" display="modal"/>
```

| Attribute | Meaning |
|---|---|
| `media` | A video (`mp4`, `webm`) or animated image (`gif`, `webp`, `svg`) inside the plugin's `assets/` folder. No external URLs, so a tour works offline and never leaks who watched it. |
| `poster` | Still frame shown before play and when motion is reduced. Required for video. |
| `captions` | WebVTT track; `{locale}` resolves like `i18n`. Required for video that has speech or on-screen text. |
| `display` | `modal` (default for a step with `media` and no `target`): a centred dialog with the player, the title and the body under it. `inline`: the player sits inside the step popover, next to the highlighted control. |

- **Player.** The stock `<player>` widget that pages already have ([Pages](../plugin/pages.md)); the modal is the
  stock dialog, so themes style it through `tour.media` and `tour.popover`.
- **Playback.** Muted, looping and click-to-play by default. It never autoplays with sound, and when the person
  prefers reduced motion it shows the `poster` with a Play button instead of starting.
- **Advance.** `advance="next"` is the default; the Next button is enabled immediately. A video never blocks the tour,
  and *Skip* works while it plays. An `advance="ended"` option is deliberately not offered.
- **Size.** Validation warns above 5 MB per file and fails above 20 MB, since assets ship inside the plugin package.
- **Validation.** `media`, `poster` and `captions` must exist in `assets/`, and a video without `poster` fails.
- **Drop-off counts** include whether the video was played, so an author can tell if anyone watches.

## When a tour appears

1. On navigation the web app asks the kernel: *which tour, if any, should this person see on this route?*
   (`GET /api/tours/next?route=…`). The answer is at most one tour, with its steps resolved to the person's locale.
2. The kernel picks the highest-priority tour that is installed, enabled, permitted for this person (`requires`), not
   yet finished at this `revision`, and not snoozed.
3. The web app waits for the page to settle (data loaded, target present), then starts it. It never starts over a
   modal, a video call, or while the person is typing in a composer.
4. A global cooldown (one tour automatically per session by default) stops a person who installs five plugins from
   being walked through five tours in a row. The rest wait for the next visit to their own page.

`trigger="manual"` tours appear in a **Take a tour** entry in the help menu of the page they belong to, and any widget
can start one: `<button label="Show me around" tour-start="first_contact"/>`.

`trigger="event"` is for tours that make sense after something happens, not on arrival: `on="record.created"` with
`model="contact"` starts the tour after the person's first contact is saved. It uses the ordinary [Events](events.md)
bus, filtered to the person who caused the event.

## What the kernel remembers

A new core table, kept per organization and **per person**, not in the browser:

```text
tour_progress
  user, plugin, tour, revision,
  status      = seen | completed | dismissed | snoozed
  step        (last step reached, for resume)
  updated_at, snoozed_until
```

Server-side so a person who finished a tour on a laptop is not shown it again on a phone, and so an administrator can
reset it. The state is not plugin data: a plugin cannot read or write it directly.

| Action by the person | Result |
|---|---|
| Finishes the last step | `completed` at this revision; never automatic again |
| "Skip tour" or Escape | `dismissed`; never automatic again, still available manually |
| "Remind me later" | `snoozed` for a day; offered again after |
| Closes the tab halfway | `seen` with the step; next visit offers to **resume**, once |

Raising `revision` makes `completed` and `dismissed` count as unseen for that tour only.

## Staying out of the way

- **Off switches.** A person setting **Show guided tours** (default on) and an organization setting
  `ui.tours.enabled` (default on). Either off means no automatic tour; manual ones still work.
- **Permissions decide the steps.** A step whose target needs a permission the person lacks is dropped on the server
  before the tour is sent. A tour is never a hint that a feature exists for someone who cannot use it.
- **Demo and seeded data.** Tours are not shown to API-only sessions or to impersonation sessions (an administrator
  looking at a person's screen must not consume that person's tour).
- **Accessibility.** The popover is a focus-trapped dialog with `aria-labelledby`, steps are announced through a live
  region, arrow keys and Escape work, and the highlight respects `prefers-reduced-motion`. The page underneath is not
  made inert for steps that ask the person to click it.
- **Themes.** The overlay, spotlight and popover use `data-part` names (`tour.overlay`, `tour.popover`, `tour.step`) so
  a theme can restyle them like any stock widget ([Themes](themes.md)).

## Events and the plugin side

The kernel emits `tour.started`, `tour.completed`, `tour.dismissed` (with the step reached) into the normal event bus.
A [rule](rules.md) can react, for instance to tick an onboarding checklist or to send a welcome message after
completion. Plugin code does not need a capability to *have* tours; it needs one only to drive them:

| Command | Capability | Effect |
|---|---|---|
| `tour::start(name)` | `ui::tour` | Queue the tour for the current person on their next page load |
| `tour::reset(name, user?)` | `ui::tour` | Forget progress for the caller (or, with `org_admin`, another person) |
| `tour::status(name)` | `ui::tour` | `none`, `seen`, `completed`, `dismissed` |

Per-step drop-off (how many people reached each step, and where they dismissed) is aggregated per tour and shown to
the plugin's administrators on the desk, so an author can tell which step loses people. Only counts are kept, never
who.

## Validation

`aether --load-plugins --dry-run` (and the `validate_plugin` MCP tool) checks a tour statically:

- every `title` and `body` key exists in the default locale;
- every `target` is in the vocabulary, and `action:`, `field:` and `column:` names exist in a page of this plugin whose
  route matches `page`;
- `page` matches a declared route; `advance="event:…"` names a topic the plugin declares;
- `requires` names a permission the plugin knows.

A tour that points at something that does not exist fails validation instead of silently never showing.

## Uninstall and upgrade

Uninstalling a plugin removes its `tour_progress` rows (see [Uninstall](uninstall.md)). Reinstalling starts fresh.
Upgrading keeps progress; only `revision` decides whether a tour comes back.

## Build order

| Step | Deliverable |
|---|---|
| 1 | `tours/*.xml` parsed and validated; `tour_progress` table; `GET /api/tours/next`, `POST /api/tours/{id}/progress` |
| 2 | `TourHost` in the app shell: spotlight, popover, step advance, target vocabulary for lists and forms |
| 3 | Manual start (help menu, `tour-start`), resume, snooze, the two off switches |
| 4 | `trigger="event"`, `advance="click"` and `event:`, `when="form"`, video steps (`media`, `display="modal"`) |
| 5 | `ui::tour` commands, `tour.*` events, drop-off counts on the desk |
| 6 | Docs page in `docs/plugin/`, a worked example, e2e in a real browser |

## Known gaps

Not covered by this design. Each is marked so it is decided on purpose rather than discovered later.

| # | Gap | Note |
|---|---|---|
| G1 | **Tutorials for plugin authors** (planned in [Plugin author tutorial](plugin_tutorial.md)). Tours teach the *end user* of a plugin. Nothing teaches a person how to *write* a plugin: there is no step-by-step tutorial, and `get_started.md` and `docs/plugin/` are reference pages. | Separate from tours. Needs its own plan: a worked, build-it-yourself path in `docs/plugin/` (manifest, a model, a page, a rule), checked against `aether --load-plugins --dry-run` so it cannot rot. |
| G2 | **Tour authoring docs.** Build step 6 promises one page in `docs/plugin/` and one example, but no author-facing guide for writing good tours (length, wording, when to use `manual`). | Part of step 6; widen it from a reference page to a short guide. |
| G3 | **Scaffolding.** `scaffold_plugin` and the CLI generators do not create a `tours/` folder or a starter tour. | Add a `tours/` stub to the scaffold once step 1 lands. |
| G4 | **One route per tour.** `page` binds a tour to a single route, so a flow that crosses pages (list, then form, then detail) needs several tours chained by hand. | Revisit after step 4; a `next-tour` hand-off is the likely answer. |
| G5 | **Surfaces beyond pages.** The target vocabulary covers list, form and custom pages. Chat panes, the desk, Mantle and the meeting view have no targets. | Extend the vocabulary per surface once those widgets carry stable `data-part` names. |
| G6 | **Kernel's own screens.** Only plugin pages are described; whether core pages (settings, org admin, plugin manager) get tours, and who writes them, is open. | Decide with the core UI owners. |
| G7 | **Checklists.** Already deferred as *Decide* 5. | Built on `tour.completed` events. |

## Decide

| # | Question | Recommendation |
|---|---|---|
| 1 | Tours as XML files, or a `[[tour]]` table in the manifest? | **XML files.** They are page-shaped, long, and translated; the manifest stays small. |
| 2 | Progress in the browser or on the server? | **Server.** It follows the person across devices and can be reset. |
| 3 | Free CSS selectors as targets? | **No.** A fixed vocabulary plus `tour="id"` keeps tours working across theme changes. |
| 4 | More than one automatic tour per session? | **No**, one per session with a cooldown; the rest wait for their own page. |
| 5 | Checklists ("Get set up: 3 of 5 done") on the desk? | **Later**, built on `tour.completed` events, not part of this design. |
| 6 | Should a tour be able to run code when a step is reached? | **No.** Reacting is the job of rules on the `tour.*` events. |
| 7 | Video and animation steps (`media`, shown in a modal or inline)? | **Yes**, from `assets/` only, muted and click-to-play, with poster and captions required; see [Video steps](#video-steps). |
