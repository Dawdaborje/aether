---
title: "Meetings: a built-in video server (SFU)"
description: "How Aether runs video meetings itself: a media server inside the kernel that forwards WebRTC audio and video, signaling over a WebSocket, rooms a plugin authorizes, and a call widget for pages."
section: architecture
status: proposal
order: 230
---
# Meetings: a built-in video server (SFU)

**Status: PROPOSAL; milestones 1 to 5 are built, and most of 6 and simulcast of 7 (`facets/sfu`: frames, rooms, TURN credentials, and the media engine, tested between real browsers; the signaling socket, `[[sfu]]`, the `sfu::` commands and the settings).  `atrium_meet` holds its meetings there by default, and a host and a guest were run through it in two Chromium browsers (`plugins/atrium/tests/meet_call_e2e.mjs`). Active speaker, screen share and the host's controls (mute, remove, end) are built; H.264/VP9 (enabling them), a TCP/TLS fallback and a load test are not. Quality follows the size of the tile through simulcast. Microphone, camera and speaker can be chosen before joining and in the call.** Nothing here is available to plugins yet; the build state of each part is in
[Milestones](#milestones). The `jitsi` bridge stays as it is: a plugin chooses between the two.

[Realtime](realtime.md) carries small signals. A video call needs the opposite: continuous audio and video from every
participant to every other, in real time. Browsers do this with WebRTC. With more than a handful of people they cannot send
their video to everyone else directly, so a server in the middle receives each person's stream once and forwards it to the
others. That server is an **SFU** (selective forwarding unit). It does not mix or decode the video; it copies packets, which is
why one server can carry many meetings.

## What is built where

| Part | Lives in | Does |
|---|---|---|
| Media server | new facet `facets/sfu`, on the [`str0m`](https://crates.io/crates/str0m) WebRTC library | Terminates DTLS/SRTP for each participant and forwards RTP between them |
| Signaling socket | `facets/core` (`GET /api/ui/sfu/{room}`) | Carries offers, answers, ICE candidates and room events; authenticates like the [realtime](realtime.md) socket |
| Room access | the plugin, `[[sfu]]` in `plugin.toml` | Decides who may join a room and as what (`host` or `member`) |
| Room control | `sfu::` commands | A plugin mutes, removes or ends from its own functions |
| Call page widget | `web/src/lib/components/widgets/CallWidget.svelte` | Camera and microphone, peer connections, tiles, controls |
| Meetings | `atrium_meet` | Plans meetings, picks the provider, opens the call page |

`str0m` is "sans-IO": it is a state machine that is given packets and the time and answers with packets to send, and it owns no
sockets or threads. That fits the kernel's tokio runtime: one task owns one UDP socket and runs every room's state machines.

## How a call is set up

```text
browser                         kernel (core)                       sfu
   | GET /api/ui/sfu/meeting.7 (upgrade) |                            |
   |------------------------------------>| session + organization     |
   |                                     | plugin: join (as the person)|
   |                                     |   -> { allow, role, name } |
   |<--------- { t: "joined", ice, you, participants } ---------------|
   | publisher offer (mic + camera)      |                            |
   |------------------------------------>|--------------------------->|
   |<------------------------------ answer ----------------------------|
   |<=========== DTLS + SRTP over the one UDP port ===================>|
   |<--------- subscriber offer (the others' tracks) ------------------|
   |-- answer ---------------------------->|------------------------->|
```

- **Two connections per person**, like LiveKit (the receiving one also carries a data channel, which keeps its bundle from ever being empty: a browser refuses an offer that stops every media line): one the browser *offers* to send its own media, one the server *offers* to
  deliver everyone else's. Adding or removing a remote participant only renegotiates the second, so the two sides never offer
  at once.
- **One UDP port** for all media (`sfu.udp_port`). Packets are matched to a participant by the ICE username in the first STUN
  packet and by source address afterwards. The firewall needs one port, not a range.
- **The server is ICE-lite**: it has a fixed public address and only answers checks; the browser does the work.
- **Identity**: the DTLS fingerprint in each side's SDP is exchanged over the authenticated socket, so a stream cannot be
  attached to someone else's seat.

### Signaling frames (JSON text)

| From the browser | Meaning |
|---|---|
| `{ "op": "offer", "sdp" }` | Publisher offer (first and on each change of what the person sends) |
| `{ "op": "answer", "sdp" }` | Answer to the server's subscriber offer |
| `{ "op": "candidate", "pc": "pub"\|"sub", "candidate" }` | Trickled ICE candidate |
| `{ "op": "mute", "kind": "audio"\|"video", "muted" }` | Tells the others; the media stops on its own |
| `{ "op": "screen", "sharing" }` | Sent *before* the offer that adds the screen's video line, so the server knows that line is a screen: it reaches the others as the stream `<sid>-screen`, and the roster's `screen` flag follows. `sharing: false` closes it for the others |
| `{ "op": "want", "user", "quality": "high"\|"low"\|"off" }` | What the person wants to receive from someone (tile size, off-screen) |
| `{ "op": "leave" }` | Leave |

| From the server | Meaning |
|---|---|
| `{ "t": "joined", "you", "role", "ice": [...], "participants": [...] }` | Joined; `ice` are STUN/TURN servers with short-lived credentials |
| `{ "t": "offer", "sdp" }` / `{ "t": "answer", "sdp" }` | Subscriber offer, publisher answer |
| `{ "t": "participant", "join"\|"leave"\|"update": {...} }` | Roster: `{ user, name, role, audio, video, screen }` |
| `{ "t": "muted", "kind" }` | A host muted this person: their browser stops sending that kind (they can switch it on again) |
| `{ "t": "speaking", "users": [...] }` | Active speakers, from the audio level RTP extension |
| `{ "t": "removed", "reason" }` | The host removed the person, or the room ended |

## Room access: `[[sfu]]`

Like `[[realtime]]`, a plugin declares which rooms exist and who may join. A room name is in the plugin's namespace.

```toml
[[sfu]]
room = "meeting.{meeting}"        # a pattern; {name} matches one segment
join = "can_join_meeting"         # called as the person; answers { allow, role?, name? }
max_participants = 50             # optional, bounded by sfu.max_participants
```

The kernel calls `join` **on every join**, as the person who asks, with `{ room, params, user }`; the answer is not cached, so
a person removed from the meeting cannot rejoin. `role` is `host` (may mute, remove and end) or `member`.

### Commands for plugins (capability group `sfu::`)

| Command | Effect |
|---|---|
| `sfu::room { room }` | `{ participants: [{ user, name, role, audio, video, screen, since }] }`, empty when nobody is in it |
| `sfu::mute { room, user, kind }` | Asks the server to stop forwarding that person's audio or video, and tells them |
| `sfu::remove { room, user }` | Ends that person's session at once; they hear `removed` |
| `sfu::end { room }` | Ends the room for everyone |
| `sfu::lock { room, locked }` | While locked, `join` is not even asked: nobody new gets in |

A plugin can only reach rooms it declared.

## Forwarding

- **Audio**: every audio track is forwarded to everyone. The server reads the audio level extension to tell who is speaking
  and sends `speaking`, so clients can highlight and order tiles without decoding anything.
- **Video**: forwarded per subscriber and per track, and only for tiles the subscriber wants (`want`). A person looking at
  nine tiles does not receive the fiftieth person's video.
- **Loss and rate**: the server answers NACKs from its send buffer, forwards PLI/FIR (key frame requests) to the sender, and
  paces each subscriber by the bandwidth estimate (transport-wide congestion control) so a slow viewer does not stall.
- **Simulcast**: the browser sends its camera as three layers (`rid` `h`, `m`, `l`: full size, half and a quarter of each side;
  a camera smaller than about 640x360 is sent as fewer layers, so the call asks for HD); the server gives each subscriber one
  layer per sender, without re-encoding. `want high` takes the biggest layer being sent and `want low` the smallest; the widget asks for
  `low` for the strip beside a shared screen and in a crowded grid (more than four tiles), and `high` for a pinned person. A
  subscriber moves to another layer only at that layer's key frame (VP8: the first bit of the frame), asking the sender for one
  and keeping the old layer until it arrives; a subscriber with nothing yet starts on a smaller layer's key frame and moves up.
  Screen shares are one stream. A layer counts as being sent while a packet of it arrived in the last 1.5 seconds: when the browser
  drops one (its own connection is short of room), viewers on it move to the biggest layer still sent, and back when it returns.
  Key frames are told per codec (VP8, VP9, H.264; the call enables VP8 only, so VP9 and H.264 are checked by unit tests on made-up
  frames, not on a real stream); for a codec it does not know, a viewer stays on the first layer they were given.
- **Bandwidth**: each viewer's connection estimates what it can carry (transport-wide congestion control, from the browser's
  feedback), and the server asks the estimator to probe for the big pictures of everyone sent to that person. A viewer whose
  estimate is under 1 Mbit/s for each camera they are sent is given the small layers only, whatever their tiles ask, and gets the big ones
  back only when the estimate is a third above that line, so a number near the line does not make the picture jump. Not done:
  a viewer is never taken below the small layers (their video is not paused to save the audio), and the numbers (1 Mbit/s a big
  camera, 150 kbit/s a small one) are set from the layer rates the browser is told, not from measurements of a real network.
- **Screen share**: one more video track with its own flag in the roster.
- **Codecs**: Opus for audio; VP8 first, H.264 and VP9 after, because every browser does VP8.
- **Not done by the server**: mixing, transcoding, layouts. A bad link to one person can only cost that person quality.

## Networks

- **Direct UDP** works for most people. The server needs a reachable address: `sfu.public_ips` (what goes into candidates)
  and the UDP port open. Behind Docker or a NAT, that is host networking or a mapped port.
- **Strict firewalls** (UDP blocked, common in offices) need a relay. Aether does not run TURN itself: set `sfu.turn_urls` and
  `sfu.turn_secret` for a [coturn](https://github.com/coturn/coturn) server using time-limited credentials. The kernel hands each person
  a username and password that expire in a few hours, in `joined.ice`. Later, a TCP/TLS listener on 443 in the SFU itself can
  replace this for sites that cannot run a relay.
- **Secure context**: browsers give camera and microphone to HTTPS pages (and `localhost`) only. The web app's
  `Permissions-Policy` must allow `camera`, `microphone` and `display-capture` for its own origin.

## Settings: `[sfu]` in `aether.toml`

The media server owns a UDP port of the machine, so it is configured by whoever runs the server, never by a plugin or an
organization.

```toml
[sfu]
enabled = true
bind = "0.0.0.0:7881"            # the one UDP media port
public_ips = ["203.0.113.10"]    # what browsers are told to send media to
turn_urls = ["turn:turn.example.com:3478"]
turn_secret = "…"                # shared with coturn (use-auth-secret)
```

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether this server runs the media server at all (it opens a UDP port on the first join) |
| `bind` | `0.0.0.0:7881` | The one media port |
| `public_ips` | none | Addresses put into ICE candidates; required for anyone not on this machine. When empty, `bind` is used if it names one address, else `127.0.0.1` (and a warning is logged) |
| `max_rooms` | `100` | Open rooms per server |
| `max_participants` | `50` | Per room, unless the plugin's `[[sfu]]` says less |
| `turn_urls`, `turn_secret` | none | A TURN relay and its shared secret |
| `credential_ttl_secs` | `14400` | How long TURN credentials last |

Per-server limits on bitrate come with milestone 6.

## The call widget

```xml
<page route="/meet/room/{meeting}" title="Meeting" shell="none">
  <call room="meeting.{meeting}" leave="/meet"/>
</page>
```

A plugin page cannot run script, so the browser side is a widget, like `<messages>`. It asks for the camera and microphone
(with a check-your-devices step first), opens the signaling socket, keeps the two peer connections, and draws the room: a grid
of tiles with the speaker highlighted, pin or spotlight, a strip for screen shares, and the controls (microphone, camera,
share, devices, leave; mute, remove and end for hosts). It asks for quality by tile size and pauses video that is off screen.
In-call chat is an ordinary [realtime](realtime.md) topic beside it.

## In `atrium_meet`

`atrium_meet` gets a setting `meet.provider`: `builtin` or `jitsi`. For `jitsi`, nothing changes (the bridge issues a signed
link). For `builtin`, "Join" opens `/meet/room/{meeting}`, the `[[sfu]]` join function checks that the person is the host or
an invited guest, and the room name is the meeting's random key. The meeting models, calendar event and guest list are shared.

## Safety

- Media is encrypted hop by hop (DTLS-SRTP): browser to server, server to browser. The server can see the packets it forwards;
  this is not end-to-end encrypted. Say so wherever it is offered.
- Joining needs a session, the organization, a matching `Origin`, and the plugin's `join` answer, every time.
- Frames are bounded in size and rate, like [realtime](realtime.md); an unauthenticated UDP packet that is not STUN for a known
  username is dropped without a reply.
- Room, participant and bitrate limits come from the settings above and are per server.
- Nothing is recorded. Recording would be a separate, visible feature with its own consent.

## Milestones

| # | What | Status |
|---|---|---|
| 1 | `facets/sfu`: signaling types, room registry, limits, TURN credentials (pure logic, 13 tests) | built |
| 2 | The media engine: UDP socket, `str0m` participants, audio and VP8 forwarding between browsers | built; `facets/sfu/dev/browser_test.mjs` runs three Chromium browsers through join, forwarding, and a person leaving |
| 3 | `GET /api/ui/sfu/{room}`, `[[sfu]]` in plugin.toml, `sfu::` commands, `[sfu]` in aether.toml, the Rust SDK module | built; the socket is checked against a running server by the `atrium_meet` test of milestone 5 |
| 4 | `<call>` widget: join, tiles, mute, camera, leave (the device picker comes later) | built; the browser client (`web/src/lib/call/client.ts`) is tested with fakes (19 tests) and in real browsers against the engine; the widget runs in a page in `meet_call_e2e.mjs`; it keeps Join now disabled until the camera has answered, or the call would start without media |
| 5 | `atrium_meet`: a meeting's `provider` (`builtin` by default, or `jitsi`), the call page `/meet/room/{meeting}`, who may join | built; `meet_call_e2e.mjs`: a host and an invited guest see each other, muting shows, an uninvited person is refused, leaving ends the call |
| 6 | Active speaker, `want`, bandwidth pacing, screen share, host controls | active speaker (audio-level extension, 700 ms hold), screen share (a stream `<sid>-screen`, one stage tile per sharer) and host controls (`moderate` on `<call>`: mute, remove, end for all) are built and run in `meet_call_e2e.mjs`; bandwidth is handled by the cap above (viewers over their limit get the small layers). The device picker (microphone, camera, speaker; `devicechange` keeps the lists current) is built: a new microphone or camera replaces the old one on the open connection with no new offer, and keeps the muted state. Stopping a share closes its video line, and a host's mute reaches the person as its own `muted` frame |
| 7 | Simulcast, H.264/VP9, TCP/TLS fallback listener | simulcast is built (three VP8 layers, per-subscriber choice by `want`, switching at key frames) and runs in `facets/sfu/dev/browser_test.mjs` (a viewer's received width goes from 960 to 240 and back) and in `meet_call_e2e.mjs` (small camera picture beside a shared screen); H.264/VP9 and the TCP/TLS listener are not started |
| 8 | A load test with real numbers, and the multi-server story (a room lives on one server; routing a room to one) | not started |

Capacity is not claimed here: forwarding cost grows with *participants × subscribed video*, and the number per core has to be
measured in milestone 8.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Use a library or write the WebRTC stack? | **`str0m`**: sans-IO, no runtime of its own, supports what an SFU needs (RTP forwarding, NACK, TWCC, simulcast). |
| 2 | One connection per person or two? | **Two** (publish and subscribe), so renegotiation never collides. |
| 3 | Signaling over the realtime socket or its own? | **Its own** (`/api/ui/sfu/{room}`): offers can be large, need ordering and replies, and must not share `client_rate` with typing. |
| 4 | Who is allowed in? | **The plugin, on every join**, as the person; the kernel never decides membership. |
| 5 | Run TURN inside Aether? | **No** for now: configure coturn. A TCP/TLS listener inside the SFU covers sites that cannot. |
| 6 | Several servers? | **A room lives on one server.** Routing is milestone 8. |
