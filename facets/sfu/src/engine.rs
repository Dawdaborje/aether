//! The media engine: one UDP socket and every participant's WebRTC connections, driven by one task.
//!
//! `str0m` is sans-IO: it is given packets and the time and answers with packets to send and events. This task owns the
//! socket, feeds it, and moves media between the connections of a room. Each participant has two connections: a
//! *publisher* the browser offers (it sends its microphone, camera and screen on it) and a *subscriber* this side
//! offers (it carries everyone else's tracks), so renegotiation never has both sides offering at once.

use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

use str0m::{
    Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig,
    change::{SdpAnswer, SdpOffer, SdpPendingOffer},
    bwe::{Bitrate, BweKind},
    media::{Direction, KeyframeRequest, KeyframeRequestKind, MediaData, MediaKind as RtcKind, Mid},
    net::{Protocol, Receive},
};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, oneshot},
};

use crate::layers::{self, Cap};
use crate::protocol::{
    Change, ClientFrame, IceServer, Leg, MediaKind, Participant, Quality, Role, ServerFrame,
};
use crate::room::{JoinError, Limits, Rooms};
use crate::speaker::Speakers;

pub type ConnId = u64;

/// Where the engine listens and what it tells browsers to connect to.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// The addresses put into ICE candidates: the server's public (or LAN) address with the media port. The first is
    /// also taken as the destination of every datagram, because one socket cannot tell which address was dialled.
    pub advertise: Vec<SocketAddr>,
    pub limits: Limits,
}

pub struct JoinRequest {
    pub room: String,
    pub user: String,
    pub name: String,
    pub role: Role,
    /// The room's own `max_participants`, when its plugin declared one.
    pub cap: Option<usize>,
    pub ice: Vec<IceServer>,
    /// Where this person's frames go: the signaling socket sends them on.
    pub out: mpsc::UnboundedSender<ServerFrame>,
}

enum Command {
    Join(Box<JoinRequest>, oneshot::Sender<Result<ConnId, JoinError>>),
    Frame(ConnId, ClientFrame),
    Leave(ConnId),
    Remove { room: String, user: String, reason: String },
    End { room: String, reason: String },
    Mute { room: String, user: String, kind: MediaKind },
    Lock { room: String, locked: bool },
    Estimate { room: String, user: String, bps: Option<u64> },
    Roster(String, oneshot::Sender<Vec<Participant>>),
    Configure { advertise: Vec<SocketAddr>, limits: Limits },
}

/// How the rest of the kernel talks to the engine.
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Command>,
}

impl Handle {
    pub async fn join(&self, request: JoinRequest) -> Result<ConnId, JoinError> {
        let (reply, answer) = oneshot::channel();
        // A stopped engine looks like a server with no room for anyone.
        self.tx.send(Command::Join(Box::new(request), reply)).await.map_err(|_| JoinError::TooManyRooms)?;
        answer.await.unwrap_or(Err(JoinError::TooManyRooms))
    }

    pub async fn frame(&self, conn: ConnId, frame: ClientFrame) {
        let _ = self.tx.send(Command::Frame(conn, frame)).await;
    }

    pub async fn leave(&self, conn: ConnId) {
        let _ = self.tx.send(Command::Leave(conn)).await;
    }

    pub async fn remove(&self, room: &str, user: &str, reason: &str) {
        let _ = self.tx.send(Command::Remove { room: room.into(), user: user.into(), reason: reason.into() }).await;
    }

    pub async fn end(&self, room: &str, reason: &str) {
        let _ = self.tx.send(Command::End { room: room.into(), reason: reason.into() }).await;
    }

    pub async fn mute(&self, room: &str, user: &str, kind: MediaKind) {
        let _ = self.tx.send(Command::Mute { room: room.into(), user: user.into(), kind }).await;
    }

    /// Fix what the server believes a viewer's connection can carry (bits per second), or give it back to the estimate with
    /// `None`. For tests, and for an operator who wants a ceiling on one person.
    pub async fn set_bandwidth(&self, room: &str, user: &str, bps: Option<u64>) {
        let _ = self.tx.send(Command::Estimate { room: room.into(), user: user.into(), bps }).await;
    }

    pub async fn lock(&self, room: &str, locked: bool) {
        let _ = self.tx.send(Command::Lock { room: room.into(), locked }).await;
    }

    /// Change what can change while the server runs: the addresses put into ICE candidates (for connections made from
    /// now on) and the room and participant limits (for joins from now on). The bound port cannot change.
    pub async fn configure(&self, advertise: Vec<SocketAddr>, limits: Limits) {
        let _ = self.tx.send(Command::Configure { advertise, limits }).await;
    }

    pub async fn roster(&self, room: &str) -> Vec<Participant> {
        let (reply, answer) = oneshot::channel();
        if self.tx.send(Command::Roster(room.into(), reply)).await.is_err() {
            return Vec::new();
        }
        answer.await.unwrap_or_default()
    }
}

/// Install the crypto provider the WebRTC library uses. Call once per process, before the first [`spawn`].
pub fn install_crypto() {
    str0m::crypto::from_feature_flags().install_process_default();
}

/// Bind the socket and start the engine task.
pub async fn spawn(config: Config) -> std::io::Result<Handle> {
    let socket = UdpSocket::bind(config.bind).await?;
    let (tx, rx) = mpsc::channel(1024);
    tokio::spawn(Engine::new(socket, config).run(rx));
    Ok(Handle { tx })
}

/// A track a participant sends, named by who sends it and its `mid` on their publisher connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackKey {
    conn: ConnId,
    mid: Mid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutState {
    ToOpen,
    Negotiating(Mid),
    Open(Mid),
    ToStop(Mid),
    NegotiatingStop(Mid),
}

/// One track delivered to a participant on their subscriber connection.
#[derive(Debug)]
struct TrackOut {
    key: TrackKey,
    kind: RtcKind,
    /// Whose track it is: the sender's `sid`, which is the stream id the browser sees.
    owner: String,
    state: OutState,
    /// Which simulcast layer this viewer is given now (none until its first key frame).
    layer: Option<String>,
    /// The sender left: close the track as soon as it is open.
    gone: bool,
}

impl TrackOut {
    fn mid(&self) -> Option<Mid> {
        match self.state {
            OutState::ToOpen => None,
            OutState::Negotiating(m) | OutState::Open(m) | OutState::ToStop(m) | OutState::NegotiatingStop(m) => Some(m),
        }
    }
}

struct TrackIn {
    mid: Mid,
    kind: RtcKind,
    /// A shared screen, not a camera.
    screen: bool,
    /// The simulcast layers (`rid`s) seen so far; empty when the sender sends one stream.
    layers: Vec<layers::Layer>,
    last_keyframe_request: Option<Instant>,
}

struct Conn {
    room: String,
    user: String,
    sid: String,
    out: mpsc::UnboundedSender<ServerFrame>,
    publisher: Option<Rtc>,
    subscriber: Rtc,
    pending: Option<SdpPendingOffer>,
    /// Whether the subscriber offer carries its data channel yet. The channel keeps the connection's bundle from
    /// ever being empty (a browser refuses an offer that stops every media line) and is where room events can travel.
    channel: bool,
    tracks_in: Vec<TrackIn>,
    tracks_out: Vec<TrackOut>,
    /// What this person asked to receive, per sender; absent means everything.
    wants: HashMap<String, Quality>,
    /// What the connection can carry, from the bandwidth estimate: viewers over the limit get small pictures only.
    cap: Cap,
    /// A ceiling set from outside for this viewer (tests; an operator who wants one), in place of the estimate.
    pinned_estimate: Option<u64>,
    /// The last estimate the connection itself gave.
    estimate: Option<u64>,
    /// The person said they are about to share their screen: the next video line they open is it.
    screen_next: bool,
}

/// What polling a connection found, applied once the poll is over (the polled connection is borrowed until then).
#[allow(clippy::large_enum_variant)]
enum Effect {
    Opened { conn: ConnId, mid: Mid, kind: RtcKind },
    Media { conn: ConnId, data: MediaData },
    Keyframe { conn: ConnId, request: KeyframeRequest },
    Dead { conn: ConnId },
    /// The estimate of what a viewer's connection can carry (bits per second).
    Bandwidth { conn: ConnId, bps: u64 },
}

struct Engine {
    socket: UdpSocket,
    config: Config,
    rooms: Rooms,
    conns: HashMap<ConnId, Conn>,
    next: ConnId,
    speakers: Speakers,
    /// What each room was last told about who is speaking (the speakers' stream ids).
    spoken: HashMap<String, Vec<String>>,
}

impl Engine {
    fn new(socket: UdpSocket, config: Config) -> Self {
        let rooms = Rooms::new(config.limits);
        Engine { socket, config, rooms, conns: HashMap::new(), next: 1, speakers: Speakers::default(), spoken: HashMap::new() }
    }

    async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let mut buf = vec![0u8; 2048];
        loop {
            let timeout = self.poll_all();
            let wait = timeout.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
            tokio::select! {
                received = self.socket.recv_from(&mut buf) => {
                    if let Ok((n, source)) = received {
                        self.receive(&buf[..n], source);
                    }
                }
                command = commands.recv() => match command {
                    Some(command) => self.command(command),
                    None => return,
                },
                _ = tokio::time::sleep(wait) => {}
            }
            let now = Instant::now();
            self.announce_speakers(now);
            for conn in self.conns.values_mut() {
                for rtc in conn.publisher.iter_mut().chain(std::iter::once(&mut conn.subscriber)) {
                    if rtc.is_alive() {
                        let _ = rtc.handle_input(Input::Timeout(now));
                    }
                }
            }
        }
    }

    /// Tell each room who is speaking when that changed since it was last told.
    fn announce_speakers(&mut self, now: Instant) {
        let active = self.speakers.active(now);
        let mut by_room: HashMap<String, Vec<String>> = HashMap::new();
        for id in active {
            if let Some(conn) = self.conns.get(&id) {
                by_room.entry(conn.room.clone()).or_default().push(conn.sid.clone());
            }
        }
        for sids in by_room.values_mut() {
            sids.sort();
        }
        let rooms: Vec<String> = self.spoken.keys().chain(by_room.keys()).cloned().collect();
        for room in rooms {
            let now_speaking = by_room.remove(&room).unwrap_or_default();
            if self.spoken.get(&room).map(Vec::as_slice).unwrap_or(&[]) == now_speaking.as_slice() {
                continue;
            }
            for conn in self.conns.values().filter(|c| c.room == room) {
                let _ = conn.out.send(ServerFrame::Speaking { users: now_speaking.clone() });
            }
            if now_speaking.is_empty() {
                self.spoken.remove(&room);
            } else {
                self.spoken.insert(room, now_speaking);
            }
        }
    }

    /// The address a datagram was sent to, as far as the connections are concerned.
    fn destination(&self) -> SocketAddr {
        self.config.advertise.first().copied().unwrap_or(self.config.bind)
    }

    fn receive(&mut self, bytes: &[u8], source: SocketAddr) {
        let Ok(contents) = bytes.try_into() else { return };
        let input = Input::Receive(
            Instant::now(),
            Receive { proto: Protocol::Udp, source, destination: self.destination(), contents },
        );
        for conn in self.conns.values_mut() {
            for rtc in conn.publisher.iter_mut().chain(std::iter::once(&mut conn.subscriber)) {
                if rtc.is_alive() && rtc.accepts(&input) {
                    if rtc.handle_input(input).is_err() {
                        rtc.disconnect();
                    }
                    return;
                }
            }
        }
        // Not for anyone we know (often a browser's first check arriving before its offer was answered): dropped silently.
    }

    /// `viewer`: this connection carries the others' media to a person, so it estimates its own bandwidth.
    fn new_rtc(&self, viewer: bool) -> Rtc {
        let mut config = RtcConfig::new();
        if viewer {
            config = config.enable_bwe(Some(Bitrate::kbps(1500)));
        }
        let mut rtc = config
            .set_ice_lite(true)
            .clear_codecs()
            .enable_opus(true, false)
            .enable_vp8(true)
            .build(Instant::now());
        for addr in &self.config.advertise {
            if let Ok(candidate) = Candidate::host(*addr, "udp") {
                let _ = rtc.add_local_candidate(candidate);
            }
        }
        rtc
    }

    // ---- commands ---------------------------------------------------------------------------------------------

    fn command(&mut self, command: Command) {
        match command {
            Command::Join(request, reply) => {
                let _ = reply.send(self.join(*request));
            }
            Command::Frame(conn, frame) => self.frame(conn, frame),
            Command::Leave(conn) => self.leave(conn),
            Command::Remove { room, user, reason } => {
                let found = self.conns.iter().find(|(_, c)| c.room == room && c.user == user).map(|(id, _)| *id);
                if let Some(id) = found {
                    self.send(id, ServerFrame::Removed { reason });
                    self.leave(id);
                }
            }
            Command::End { room, reason } => {
                let ids: Vec<ConnId> = self.conns.iter().filter(|(_, c)| c.room == room).map(|(id, _)| *id).collect();
                for id in ids {
                    self.send(id, ServerFrame::Removed { reason: reason.clone() });
                    self.leave(id);
                }
                self.rooms.end(&room);
            }
            Command::Mute { room, user, kind } => {
                if let Some(change) = self.rooms.set_muted(&room, &user, kind, true) {
                    self.announce(&room, None, change);
                    let ids: Vec<ConnId> = self.conns.iter().filter(|(_, c)| c.room == room && c.user == user).map(|(id, _)| *id).collect();
                    for id in ids {
                        self.send(id, ServerFrame::Muted { kind });
                    }
                }
            }
            Command::Estimate { room, user, bps } => {
                let ids: Vec<ConnId> = self.conns.iter().filter(|(_, c)| c.room == room && c.user == user).map(|(id, _)| *id).collect();
                for id in ids {
                    if let Some(conn) = self.conns.get_mut(&id) {
                        conn.pinned_estimate = bps;
                    }
                    self.recap(id);
                }
            }
            Command::Lock { room, locked } => {
                self.rooms.set_locked(&room, locked);
            }
            Command::Configure { advertise, limits } => {
                if !advertise.is_empty() {
                    self.config.advertise = advertise;
                }
                self.config.limits = limits;
                self.rooms.set_limits(limits);
            }
            Command::Roster(room, reply) => {
                let _ = reply.send(self.rooms.people(&room));
            }
        }
    }

    fn join(&mut self, request: JoinRequest) -> Result<ConnId, JoinError> {
        let now = unix_now();
        let id = self.next;
        let who = Participant {
            sid: format!("s{id}"),
            user: request.user.clone(),
            name: request.name,
            role: request.role,
            audio: true,
            video: true,
            screen: false,
            since: now,
        };
        // A second tab of the same person replaces the first: the seat is theirs.
        let older: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| c.room == request.room && c.user == request.user)
            .map(|(id, _)| *id)
            .collect();
        let change = self.rooms.join(&request.room, who, request.cap, now)?;
        for id in older {
            self.send(id, ServerFrame::Removed { reason: "you joined from another place".into() });
            self.drop_conn(id);
        }
        self.next += 1;
        tracing::info!(conn = id, room = %request.room, user = %request.user, "joined");
        let subscriber = self.new_rtc(true);
        let mut conn = Conn {
            room: request.room.clone(),
            user: request.user.clone(),
            sid: format!("s{id}"),
            out: request.out,
            publisher: None,
            subscriber,
            pending: None,
            channel: false,
            tracks_in: Vec::new(),
            tracks_out: Vec::new(),
            wants: HashMap::new(),
            screen_next: false,
            cap: Cap::Full,
            pinned_estimate: None,
            estimate: None,
        };
        // What the others are already sending.
        for (other_id, other) in self.conns.iter().filter(|(_, c)| c.room == request.room) {
            for track in &other.tracks_in {
                conn.tracks_out.push(TrackOut {
                    key: TrackKey { conn: *other_id, mid: track.mid },
                    kind: track.kind,
                    owner: if track.screen { format!("{}-screen", other.sid) } else { other.sid.clone() },
                    state: OutState::ToOpen,
                    layer: None,
                    gone: false,
                });
            }
        }
        let role = self.rooms.role_of(&request.room, &request.user).unwrap_or(Role::Member);
        let participants = self.rooms.people(&request.room);
        let _ = conn.out.send(ServerFrame::Joined { you: request.user, role, ice: request.ice, participants });
        self.conns.insert(id, conn);
        self.announce(&request.room, Some(id), change);
        Ok(id)
    }

    fn frame(&mut self, id: ConnId, frame: ClientFrame) {
        match frame {
            ClientFrame::Offer { sdp } => self.publisher_offer(id, &sdp),
            ClientFrame::Answer { sdp } => self.subscriber_answer(id, &sdp),
            ClientFrame::Candidate { pc, candidate } => {
                let Ok(candidate) = Candidate::from_sdp_string(&candidate) else { return };
                let Some(conn) = self.conns.get_mut(&id) else { return };
                match pc {
                    Leg::Pub => {
                        if let Some(rtc) = conn.publisher.as_mut() {
                            rtc.add_remote_candidate(candidate);
                        }
                    }
                    Leg::Sub => conn.subscriber.add_remote_candidate(candidate),
                }
            }
            ClientFrame::Mute { kind, muted } => {
                let Some(conn) = self.conns.get(&id) else { return };
                let (room, user) = (conn.room.clone(), conn.user.clone());
                if let Some(change) = self.rooms.set_muted(&room, &user, kind, muted) {
                    self.announce(&room, None, change);
                }
            }
            ClientFrame::Screen { sharing } => {
                let Some(conn) = self.conns.get_mut(&id) else { return };
                let (room, user) = (conn.room.clone(), conn.user.clone());
                conn.screen_next = sharing;
                if !sharing {
                    let ended: Vec<Mid> = conn.tracks_in.iter().filter(|t| t.screen).map(|t| t.mid).collect();
                    conn.tracks_in.retain(|t| !t.screen);
                    for mid in ended {
                        self.close_track(&room, TrackKey { conn: id, mid });
                    }
                }
                if let Some(change) = self.rooms.set_screen(&room, &user, sharing) {
                    self.announce(&room, None, change);
                }
            }
            ClientFrame::Want { user, quality } => {
                if let Some(conn) = self.conns.get_mut(&id) {
                    conn.wants.insert(user, quality);
                }
            }
            ClientFrame::Leave => self.leave(id),
        }
    }

    fn publisher_offer(&mut self, id: ConnId, sdp: &str) {
        let Ok(offer) = SdpOffer::from_sdp_string(sdp) else {
            return self.send(id, ServerFrame::Error { message: "that offer could not be read".into() });
        };
        if self.conns.get(&id).is_some_and(|c| c.publisher.is_none()) {
            let rtc = self.new_rtc(false);
            if let Some(conn) = self.conns.get_mut(&id) {
                conn.publisher = Some(rtc);
            }
        }
        let Some(rtc) = self.conns.get_mut(&id).and_then(|c| c.publisher.as_mut()) else { return };
        match rtc.sdp_api().accept_offer(offer) {
            Ok(answer) => {
                let sdp = answer.to_sdp_string();
                self.send(id, ServerFrame::Answer { sdp });
            }
            Err(error) => {
                tracing::warn!(%error, "publisher offer refused");
                self.send(id, ServerFrame::Error { message: "that offer was refused".into() });
            }
        }
    }

    fn subscriber_answer(&mut self, id: ConnId, sdp: &str) {
        let Ok(answer) = SdpAnswer::from_sdp_string(sdp) else {
            return self.send(id, ServerFrame::Error { message: "that answer could not be read".into() });
        };
        let Some(conn) = self.conns.get_mut(&id) else { return };
        let Some(pending) = conn.pending.take() else { return };
        if let Err(error) = conn.subscriber.sdp_api().accept_answer(pending, answer) {
            tracing::warn!(%error, "subscriber answer refused");
            return;
        }
        let mut opened = Vec::new();
        for track in &mut conn.tracks_out {
            if let OutState::Negotiating(m) = track.state {
                track.state = OutState::Open(m);
                if track.kind == RtcKind::Video {
                    opened.push(track.key);
                }
            }
        }
        conn.tracks_out.retain(|t| !matches!(t.state, OutState::NegotiatingStop(_)));
        // A new viewer of a video needs a key frame to start from.
        for key in opened {
            self.request_keyframe(key, None, KeyframeRequestKind::Pli);
        }
    }

    fn send(&self, id: ConnId, frame: ServerFrame) {
        if let Some(conn) = self.conns.get(&id) {
            let _ = conn.out.send(frame);
        }
    }

    /// Tell everyone in the room (except `skip`) about a change of the roster.
    fn announce(&self, room: &str, skip: Option<ConnId>, change: Change) {
        for (id, conn) in &self.conns {
            if conn.room == room && Some(*id) != skip {
                let _ = conn.out.send(ServerFrame::Participant { change: change.clone() });
            }
        }
    }

    fn leave(&mut self, id: ConnId) {
        let Some(conn) = self.conns.get(&id) else { return };
        let (room, user) = (conn.room.clone(), conn.user.clone());
        tracing::info!(conn = id, %room, %user, "left");
        self.drop_conn(id);
        if let Some(change) = self.rooms.leave(&room, &user) {
            self.announce(&room, None, change);
        }
    }

    /// Forget a connection and close its tracks for everyone who was receiving them.
    fn drop_conn(&mut self, id: ConnId) {
        self.speakers.forget(id);
        let Some(mut gone) = self.conns.remove(&id) else { return };
        gone.subscriber.disconnect();
        if let Some(p) = gone.publisher.as_mut() {
            p.disconnect();
        }
        let room = gone.room.clone();
        let keys: Vec<TrackKey> = self
            .conns
            .values()
            .filter(|c| c.room == room)
            .flat_map(|c| c.tracks_out.iter().map(|t| t.key))
            .filter(|k| k.conn == id)
            .collect();
        for key in keys {
            self.close_track(&room, key);
        }
    }

    /// Close one sender's track for everyone in the room who was receiving it.
    fn close_track(&mut self, room: &str, key: TrackKey) {
        for conn in self.conns.values_mut().filter(|c| c.room == room) {
            conn.tracks_out.retain_mut(|t| {
                if t.key != key {
                    return true;
                }
                match t.state {
                    OutState::ToOpen => false,
                    OutState::Open(m) => {
                        t.state = OutState::ToStop(m);
                        true
                    }
                    _ => {
                        t.gone = true;
                        true
                    }
                }
            });
        }
    }

    // ---- driving the connections --------------------------------------------------------------------------------

    /// Poll every connection until each waits for something; returns the soonest time any of them needs attention.
    fn poll_all(&mut self) -> Instant {
        let mut soonest = Instant::now() + Duration::from_millis(100);
        loop {
            let mut effects = Vec::new();
            let ids: Vec<ConnId> = self.conns.keys().copied().collect();
            for id in ids {
                self.negotiate(id);
                let Some(conn) = self.conns.get_mut(&id) else { continue };
                for rtc in conn.publisher.iter_mut().chain(std::iter::once(&mut conn.subscriber)) {
                    let t = poll(rtc, id, &self.socket, &mut effects);
                    soonest = soonest.min(t);
                }
            }
            if effects.is_empty() {
                return soonest;
            }
            for effect in effects {
                self.apply(effect);
            }
        }
    }

    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::Opened { conn, mid, kind } => {
                let Some(owner) = self.conns.get_mut(&conn) else { return };
                let screen = kind == RtcKind::Video && std::mem::take(&mut owner.screen_next);
                owner.tracks_in.push(TrackIn { mid, kind, screen, layers: Vec::new(), last_keyframe_request: None });
                // A shared screen travels as a stream of its own, so the other side can tell it from the camera.
                let (room, user) = (owner.room.clone(), if screen { format!("{}-screen", owner.sid) } else { owner.sid.clone() });
                for (id, other) in self.conns.iter_mut().filter(|(id, c)| **id != conn && c.room == room) {
                    let _ = id;
                    other.tracks_out.push(TrackOut {
                        key: TrackKey { conn, mid },
                        kind,
                        owner: user.clone(),
                        state: OutState::ToOpen,
                        layer: None,
                        gone: false,
                    });
                }
            }
            Effect::Media { conn, data } => self.forward(conn, data),
            Effect::Keyframe { conn, request } => {
                let found = self
                    .conns
                    .get(&conn)
                    .and_then(|c| c.tracks_out.iter().find(|t| t.mid() == Some(request.mid)))
                    .map(|t| (t.key, t.layer.clone()));
                if let Some((key, layer)) = found {
                    // A simulcast viewer needs the key frame of the layer it is given, not of the sender's default.
                    let rid = layer.as_deref().map(str0m::media::Rid::from).or(request.rid);
                    self.request_keyframe(key, rid, request.kind);
                }
            }
            Effect::Bandwidth { conn, bps } => self.estimated(conn, bps),
            Effect::Dead { conn } => self.leave(conn),
        }
    }

    /// Ask the sender of `key` for a key frame, at most once a second per track.
    fn request_keyframe(&mut self, key: TrackKey, rid: Option<str0m::media::Rid>, kind: KeyframeRequestKind) {
        let Some(owner) = self.conns.get_mut(&key.conn) else { return };
        let Some(track) = owner.tracks_in.iter_mut().find(|t| t.mid == key.mid) else { return };
        if track.last_keyframe_request.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        track.last_keyframe_request = Some(Instant::now());
        if let Some(mut writer) = owner.publisher.as_mut().and_then(|p| p.writer(key.mid)) {
            let _ = writer.request_keyframe(rid, kind);
        }
    }

    /// A new bandwidth estimate for a viewer: it decides whether they are given big pictures.
    fn estimated(&mut self, id: ConnId, bps: u64) {
        tracing::debug!(conn = id, bps, "bandwidth estimate");
        if let Some(conn) = self.conns.get_mut(&id) {
            conn.estimate = Some(bps);
        }
        self.recap(id);
    }

    /// Work out a viewer's cap again from the estimate in force (a pinned one wins).
    fn recap(&mut self, id: ConnId) {
        let Some(conn) = self.conns.get_mut(&id) else { return };
        let Some(bps) = conn.pinned_estimate.or(conn.estimate) else {
            conn.cap = Cap::Full;
            return;
        };
        let cameras = conn.tracks_out.iter().filter(|t| t.kind == RtcKind::Video && !t.gone && !t.owner.ends_with("-screen")).count();
        let cap = layers::next_cap(conn.cap, bps, cameras);
        if cap != conn.cap {
            tracing::info!(conn = id, bps, cameras, ?cap, "bandwidth cap changed");
            conn.cap = cap;
        }
    }

    /// Send a packet of media to everyone in the room who receives that track.
    fn forward(&mut self, from: ConnId, data: MediaData) {
        let Some(origin) = self.conns.get(&from) else { return };
        let room = origin.room.clone();
        let owner = origin.user.clone();
        if let (Some(level), true) = (
            data.ext_vals.audio_level,
            origin.tracks_in.iter().any(|t| t.mid == data.mid && t.kind == RtcKind::Audio),
        ) {
            self.speakers.hear(from, level, Instant::now());
        }
        if !data.contiguous && origin.tracks_in.iter().any(|t| t.mid == data.mid && t.kind == RtcKind::Video) {
            self.request_keyframe(TrackKey { conn: from, mid: data.mid }, data.rid, KeyframeRequestKind::Fir);
        }
        let key = TrackKey { conn: from, mid: data.mid };
        // Simulcast: remember which layers the sender really sends now, so a viewer is never pointed at one that went quiet.
        let layer = data.rid.map(|r| r.to_string());
        let now = Instant::now();
        let mut seen: Vec<String> = Vec::new();
        let sending = self.conns.get_mut(&from).and_then(|c| c.tracks_in.iter_mut().find(|t| t.mid == data.mid));
        if let (Some(name), Some(track)) = (&layer, sending) {
            layers::note(&mut track.layers, name, now);
            seen = layers::alive(&track.layers, now);
        }
        let keyframe = layers::is_keyframe(data.params.spec().codec, &data.data);
        let mut wanted_keyframe = false;
        for (id, conn) in self.conns.iter_mut().filter(|(id, c)| **id != from && c.room == room) {
            let _ = id;
            let quality = layers::effective(conn.wants.get(&owner).copied().unwrap_or(Quality::High), conn.cap);
            let Some(track) = conn.tracks_out.iter_mut().find(|t| t.key == key) else { continue };
            let OutState::Open(mid) = track.state else { continue };
            if track.kind == RtcKind::Video && quality == Quality::Off {
                continue;
            }
            if let (Some(name), RtcKind::Video) = (&layer, track.kind) {
                let Some(target) = layers::target(&seen, quality) else { continue };
                match keyframe {
                    // A codec this does not know: the viewer is given the first layer wanted and stays on it.
                    None => {
                        if track.layer.is_none() && name == target {
                            track.layer = Some(name.clone());
                        }
                    }
                    Some(keyframe) => {
                        // A viewer with nothing yet starts on a smaller layer's key frame rather than wait for the big one.
                        if track.layer.is_none() && name != target && keyframe && layers::rank(name) < layers::rank(target) {
                            track.layer = Some(name.clone());
                        }
                        if name == target && track.layer.as_deref() != Some(target) {
                            // Moving to the layer this viewer should have: only at its key frame; until then the old one goes on.
                            if keyframe {
                                track.layer = Some(target.to_string());
                            } else {
                                wanted_keyframe = true;
                            }
                        }
                    }
                }
                if track.layer.as_deref() != Some(name.as_str()) {
                    continue;
                }
            }
            let Some(writer) = conn.subscriber.writer(mid) else { continue };
            let Some(pt) = writer.match_params(data.params) else { continue };
            if writer.write(pt, data.network_time, data.time, data.data.clone()).is_err() {
                conn.subscriber.disconnect();
            }
        }
        if wanted_keyframe {
            self.request_keyframe(key, data.rid, KeyframeRequestKind::Pli);
        }
    }

    /// Offer a participant the tracks that opened or closed since the last offer, when none is waiting for an answer.
    fn negotiate(&mut self, id: ConnId) {
        let Some(conn) = self.conns.get_mut(&id) else { return };
        if conn.pending.is_some() {
            return;
        }
        for track in &mut conn.tracks_out {
            if let (OutState::Open(m), true) = (track.state, track.gone) {
                track.state = OutState::ToStop(m);
            }
        }
        conn.tracks_out.retain(|t| !(t.gone && t.state == OutState::ToOpen));
        let mut change = conn.subscriber.sdp_api();
        if !conn.channel {
            change.add_channel("sfu".into());
            conn.channel = true;
        }
        for track in &mut conn.tracks_out {
            match track.state {
                OutState::ToOpen => {
                    let mid = change.add_media(track.kind, Direction::SendOnly, Some(track.owner.clone()), None, None);
                    track.state = OutState::Negotiating(mid);
                }
                OutState::ToStop(mid) => {
                    change.stop_media(mid);
                    track.state = OutState::NegotiatingStop(mid);
                }
                _ => {}
            }
        }
        if !change.has_changes() {
            return;
        }
        let Some((offer, pending)) = change.apply() else { return };
        conn.pending = Some(pending);
        // Ask the estimator to probe for what the big pictures of everyone sent to this person would take.
        let cameras = conn.tracks_out.iter().filter(|t| t.kind == RtcKind::Video && !t.gone && !t.owner.ends_with("-screen")).count();
        conn.subscriber.bwe().set_desired_bitrate(Bitrate::bps(layers::BIG_BPS * cameras.max(1) as u64 * 3 / 2));
        let _ = conn.out.send(ServerFrame::Offer { sdp: offer.to_sdp_string() });
    }
}

/// Drain one connection: send what it wants sent, collect what it found, and say when it next needs attention.
fn poll(rtc: &mut Rtc, conn: ConnId, socket: &UdpSocket, effects: &mut Vec<Effect>) -> Instant {
    loop {
        if !rtc.is_alive() {
            tracing::info!(conn, "connection ended");
            effects.push(Effect::Dead { conn });
            return Instant::now() + Duration::from_secs(1);
        }
        match rtc.poll_output() {
            Ok(Output::Timeout(t)) => return t,
            Ok(Output::Transmit(t)) => {
                // A full send buffer drops a packet, which media tolerates; nothing here may block the engine.
                let _ = socket.try_send_to(&t.contents, t.destination);
            }
            Ok(Output::Event(event)) => match event {
                Event::IceConnectionStateChange(state) => {
                    tracing::info!(conn, ?state, "ice state");
                    if state == IceConnectionState::Disconnected {
                        rtc.disconnect();
                    }
                }
                Event::MediaAdded(added) => effects.push(Effect::Opened { conn, mid: added.mid, kind: added.kind }),
                Event::MediaData(data) => effects.push(Effect::Media { conn, data }),
                Event::KeyframeRequest(request) => effects.push(Effect::Keyframe { conn, request }),
                Event::EgressBitrateEstimate(BweKind::Twcc { estimate, .. } | BweKind::Remb { estimate, .. }) => {
                    effects.push(Effect::Bandwidth { conn, bps: estimate.as_u64() });
                }
                _ => {}
            },
            Err(error) => {
                tracing::warn!(%error, "poll failed");
                rtc.disconnect();
            }
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
