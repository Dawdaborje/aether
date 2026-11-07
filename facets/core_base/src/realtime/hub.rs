//! The topic hub behind the WebSocket (see `docs/architecture/realtime.md`).
//!
//! Connections join topics (`<plugin>:<name>`); a published message gets the next sequence number of its topic, is
//! kept in a short ring so a reconnecting browser can resume, and is queued to every member. Nothing here waits on a
//! slow receiver: each connection has a bounded queue, and one that cannot keep up is disconnected (the browser
//! reconnects and resumes from its last sequence number).
//!
//! [`RealtimeHub::publish`] is the one way messages enter, so a pub/sub backend for several servers can call it for
//! messages from elsewhere, like [`crate::notifications::NotificationHub`].

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio::sync::mpsc;

/// Frames a connection may have waiting before it is considered too slow.
pub const QUEUE: usize = 256;
/// Topics kept in memory before idle ones are dropped.
const MAX_TOPICS: usize = 10_000;
const IDLE_TOPIC: Duration = Duration::from_secs(3600);

type TopicKey = (String, String);

struct Conn {
    org: String,
    user: String,
    tx: mpsc::Sender<Value>,
    topics: HashSet<String>,
}

struct Topic {
    seq: u64,
    ring: VecDeque<(u64, Value)>,
    cap: usize,
    members: HashSet<u64>,
    /// Connections per person, for presence.
    users: HashMap<String, u32>,
    /// Connections that are on the topic but not looking at it (another page or app, a hidden tab).
    idle_conns: HashSet<u64>,
    /// Idle connections per person; a person is idle when all of their connections are.
    idle_users: HashMap<String, u32>,
    presence: bool,
    touched: Instant,
}

impl Topic {
    /// Whether the person is connected and every connection of theirs is idle.
    fn user_idle(&self, user: &str) -> bool {
        self.users.get(user).is_some_and(|all| *all > 0 && self.idle_users.get(user).copied().unwrap_or(0) >= *all)
    }

    fn idle_list(&self) -> Vec<String> {
        let mut idle: Vec<String> = self.users.keys().filter(|u| self.user_idle(u)).cloned().collect();
        idle.sort();
        idle
    }

    fn new(cap: usize) -> Self {
        Self {
            seq: 0,
            ring: VecDeque::new(),
            cap,
            members: HashSet::new(),
            users: HashMap::new(),
            idle_conns: HashSet::new(),
            idle_users: HashMap::new(),
            presence: false,
            touched: Instant::now(),
        }
    }
}

#[derive(Default)]
struct State {
    conns: HashMap<u64, Conn>,
    topics: HashMap<TopicKey, Topic>,
    per_user: HashMap<String, u32>,
}

#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    next: AtomicU64,
}

#[derive(Clone, Default)]
pub struct RealtimeHub {
    inner: Arc<Inner>,
}

fn locked(inner: &Inner) -> MutexGuard<'_, State> {
    // A panic elsewhere must not take realtime down with it; the maps stay valid.
    inner.state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn user_key(org: &str, user: &str) -> String {
    format!("{org}\u{0}{user}")
}

/// Queue `frame` for connection `id`; false when it cannot take more (gone, or too slow).
fn queue(conn: &Conn, frame: Value) -> bool {
    conn.tx.try_send(frame).is_ok()
}

impl State {
    /// Send `frame` to the members of a topic except those of `skip_user`; slow ones are disconnected.
    fn deliver(&mut self, key: &TopicKey, frame: &Value, skip_user: Option<&str>) {
        let Some(topic) = self.topics.get(key) else { return };
        let mut failed = Vec::new();
        for id in &topic.members {
            let Some(conn) = self.conns.get(id) else { continue };
            if skip_user.is_some_and(|user| user == conn.user) {
                continue;
            }
            if !queue(conn, frame.clone()) {
                failed.push(*id);
            }
        }
        for id in failed {
            self.remove_conn(id);
        }
    }

    /// Take a connection out of one topic; announces the leave when it was the person's last connection, and a change
    /// between idle and active when the others of theirs decide it.
    fn leave_topic(&mut self, key: &TopicKey, id: u64, user: &str) {
        let Some(topic) = self.topics.get_mut(key) else { return };
        if !topic.members.contains(&id) {
            return;
        }
        let was_idle = topic.user_idle(user);
        topic.members.remove(&id);
        if topic.idle_conns.remove(&id)
            && let Some(idle) = topic.idle_users.get_mut(user)
        {
            *idle = idle.saturating_sub(1);
            if *idle == 0 {
                topic.idle_users.remove(user);
            }
        }
        let mut gone = false;
        if let Some(count) = topic.users.get_mut(user) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                topic.users.remove(user);
                gone = true;
            }
        }
        let now_idle = topic.user_idle(user);
        let presence = topic.presence;
        if !presence {
            return;
        }
        if gone {
            let frame = json!({ "t": "presence", "topic": format!("{}", key.1), "leave": user });
            self.deliver(key, &frame, None);
        } else if was_idle != now_idle {
            let frame = if now_idle {
                json!({ "t": "presence", "topic": key.1, "idle": user })
            } else {
                json!({ "t": "presence", "topic": key.1, "active": user })
            };
            self.deliver(key, &frame, None);
        }
    }

    fn remove_conn(&mut self, id: u64) {
        let Some(conn) = self.conns.remove(&id) else { return };
        let key = user_key(&conn.org, &conn.user);
        if let Some(open) = self.per_user.get_mut(&key) {
            *open = open.saturating_sub(1);
            if *open == 0 {
                self.per_user.remove(&key);
            }
        }
        for name in &conn.topics {
            self.leave_topic(&(conn.org.clone(), name.clone()), id, &conn.user);
        }
        // Dropping `conn` drops its sender, which ends the connection's loop.
    }

    fn prune(&mut self) {
        if self.topics.len() <= MAX_TOPICS {
            return;
        }
        self.topics.retain(|_, topic| !topic.members.is_empty() || topic.touched.elapsed() < IDLE_TOPIC);
    }
}

/// A connection's place in the hub. Dropping it removes the connection from every topic.
pub struct Connection {
    pub id: u64,
    hub: RealtimeHub,
}

impl Drop for Connection {
    fn drop(&mut self) {
        locked(&self.hub.inner).remove_conn(self.id);
    }
}

/// What `join` found.
#[derive(Debug, PartialEq, Eq)]
pub enum Joined {
    Ok,
    /// The connection is gone (it was disconnected for being slow).
    Gone,
}

impl RealtimeHub {
    /// Register a connection of `user`, or `None` when they already have `max` open.
    pub fn connect(&self, org: &str, user: &str, max: u32) -> Option<(Connection, mpsc::Receiver<Value>)> {
        let mut state = locked(&self.inner);
        let open = state.per_user.entry(user_key(org, user)).or_insert(0);
        if *open >= max {
            return None;
        }
        *open += 1;
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = mpsc::channel(QUEUE);
        state.conns.insert(
            id,
            Conn { org: org.to_string(), user: user.to_string(), tx, topics: HashSet::new() },
        );
        Some((Connection { id, hub: self.clone() }, rx))
    }

    /// Join `topic` (the full `<plugin>:<name>`). The `sub` answer and any replayed messages are queued in order
    /// before anything newer, so the browser sees one ordered stream. `since` is the last sequence it has.
    pub fn join(&self, id: u64, topic: &str, cap: usize, presence: bool, since: Option<u64>) -> Joined {
        let mut state = locked(&self.inner);
        let Some(conn) = state.conns.get(&id) else { return Joined::Gone };
        let (org, user) = (conn.org.clone(), conn.user.clone());
        let key = (org, topic.to_string());
        let t = state.topics.entry(key.clone()).or_insert_with(|| Topic::new(cap));
        t.cap = cap;
        t.presence = presence;
        t.touched = Instant::now();
        let already = t.members.contains(&id);
        let seq = t.seq;
        let mut resync = false;
        let mut replay = Vec::new();
        if let Some(since) = since {
            if since > seq {
                resync = true; // the server restarted or the topic was dropped
            } else if since < seq {
                match t.ring.front() {
                    Some((oldest, _)) if *oldest <= since + 1 => {
                        replay = t.ring.iter().filter(|(n, _)| *n > since).map(|(_, f)| f.clone()).collect();
                    }
                    _ => resync = true,
                }
            }
        }
        let mut announce = false;
        let was_idle = t.user_idle(&user);
        if !already {
            t.members.insert(id);
            let count = t.users.entry(user.clone()).or_insert(0);
            *count += 1;
            announce = presence && *count == 1;
        }
        let wakes = presence && !announce && was_idle && !t.user_idle(&user);
        let mut present: Vec<String> = if presence { t.users.keys().cloned().collect() } else { Vec::new() };
        present.sort();
        let idle: Vec<String> = if presence { t.idle_list() } else { Vec::new() };
        let mut frames = vec![json!({ "t": "sub", "topic": topic, "ok": true, "seq": seq, "presence": present, "idle": idle })];
        if resync {
            frames.push(json!({ "t": "resync", "topic": topic }));
        }
        frames.extend(replay);
        let ok = state.conns.get(&id).is_some_and(|conn| frames.into_iter().all(|f| queue(conn, f)));
        if !ok {
            state.remove_conn(id);
            return Joined::Gone;
        }
        if let Some(conn) = state.conns.get_mut(&id) {
            conn.topics.insert(topic.to_string());
        }
        if announce {
            let frame = json!({ "t": "presence", "topic": topic, "join": user });
            state.deliver(&key, &frame, Some(&user));
        } else if wakes {
            // A new connection of a person who was idle: they are here again.
            let frame = json!({ "t": "presence", "topic": topic, "active": user });
            state.deliver(&key, &frame, None);
        }
        Joined::Ok
    }

    /// A connection is on a topic but not looking at it (`idle`), or looking again. Tells the topic when that changes the
    /// person's state: idle only when every connection of theirs is.
    pub fn set_idle(&self, id: u64, topic: &str, idle: bool) {
        let mut state = locked(&self.inner);
        let Some(conn) = state.conns.get(&id) else { return };
        if !conn.topics.contains(topic) {
            return;
        }
        let (key, user) = ((conn.org.clone(), topic.to_string()), conn.user.clone());
        let Some(t) = state.topics.get_mut(&key) else { return };
        let before = t.user_idle(&user);
        if idle {
            if t.idle_conns.insert(id) {
                *t.idle_users.entry(user.clone()).or_insert(0) += 1;
            }
        } else if t.idle_conns.remove(&id)
            && let Some(n) = t.idle_users.get_mut(&user)
        {
            *n = n.saturating_sub(1);
            if *n == 0 {
                t.idle_users.remove(&user);
            }
        }
        let after = t.user_idle(&user);
        if t.presence && before != after {
            let frame = if after {
                json!({ "t": "presence", "topic": topic, "idle": user })
            } else {
                json!({ "t": "presence", "topic": topic, "active": user })
            };
            state.deliver(&key, &frame, None);
        }
    }

    pub fn leave(&self, id: u64, topic: &str) {
        let mut state = locked(&self.inner);
        let Some(conn) = state.conns.get_mut(&id) else { return };
        if !conn.topics.remove(topic) {
            return;
        }
        let (org, user) = (conn.org.clone(), conn.user.clone());
        state.leave_topic(&(org, topic.to_string()), id, &user);
    }

    /// Publish to a topic of `org`. Returns the sequence number. `except_user` does not receive it (they know).
    pub fn publish(&self, org: &str, topic: &str, cap: usize, event: &str, payload: Value, except_user: Option<&str>) -> u64 {
        let mut state = locked(&self.inner);
        let key = (org.to_string(), topic.to_string());
        let t = state.topics.entry(key.clone()).or_insert_with(|| Topic::new(cap));
        t.cap = cap;
        t.touched = Instant::now();
        t.seq += 1;
        let seq = t.seq;
        let frame = json!({ "t": "msg", "topic": topic, "seq": seq, "event": event, "payload": payload });
        if cap > 0 {
            t.ring.push_back((seq, frame.clone()));
        }
        while t.ring.len() > cap {
            t.ring.pop_front();
        }
        state.deliver(&key, &frame, except_user);
        state.prune();
        seq
    }

    /// A client event (typing, …) from connection `id` to the other people on the topic. False when `id` is not on it.
    pub fn send_event(&self, id: u64, topic: &str, kind: &str, payload: Value) -> bool {
        let mut state = locked(&self.inner);
        let Some(conn) = state.conns.get(&id) else { return false };
        if !conn.topics.contains(topic) {
            return false;
        }
        let key = (conn.org.clone(), topic.to_string());
        let user = conn.user.clone();
        let frame = json!({ "t": "event", "topic": topic, "kind": kind, "user": user, "payload": payload });
        state.deliver(&key, &frame, Some(&user));
        true
    }

    /// End one person's membership of a topic (they left the channel). Returns how many connections it ended.
    pub fn drop_user(&self, org: &str, topic: &str, user: &str) -> usize {
        let mut state = locked(&self.inner);
        let ids: Vec<u64> = state
            .conns
            .iter()
            .filter(|(_, c)| c.org == org && c.user == user && c.topics.contains(topic))
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            if let Some(conn) = state.conns.get_mut(id) {
                conn.topics.remove(topic);
                let _ = queue(conn, json!({ "t": "dropped", "topic": topic }));
            }
            state.leave_topic(&(org.to_string(), topic.to_string()), *id, user);
        }
        ids.len()
    }

    /// Who is connected to a topic now.
    pub fn presence(&self, org: &str, topic: &str) -> Vec<String> {
        let state = locked(&self.inner);
        let mut users: Vec<String> = state
            .topics
            .get(&(org.to_string(), topic.to_string()))
            .map(|t| t.users.keys().cloned().collect())
            .unwrap_or_default();
        users.sort();
        users
    }

    /// Who is connected to a topic now, and whether each is idle.
    pub fn presence_state(&self, org: &str, topic: &str) -> Vec<(String, bool)> {
        let state = locked(&self.inner);
        let mut users: Vec<(String, bool)> = state
            .topics
            .get(&(org.to_string(), topic.to_string()))
            .map(|t| t.users.keys().map(|u| (u.clone(), t.user_idle(u))).collect())
            .unwrap_or_default();
        users.sort();
        users
    }

    /// End every connection; used at shutdown so open sockets do not hold the server up.
    pub fn close_all(&self) {
        let mut state = locked(&self.inner);
        state.conns.clear();
        state.topics.clear();
        state.per_user.clear();
    }

    /// Open connections across everyone; for logs and tests.
    pub fn open_connections(&self) -> usize {
        locked(&self.inner).conns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &mut mpsc::Receiver<Value>) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            out.push(frame);
        }
        out
    }

    fn t(frame: &Value) -> &str {
        frame["t"].as_str().unwrap_or_default()
    }

    #[test]
    fn members_get_messages_in_order_and_others_do_not() {
        let hub = RealtimeHub::default();
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        let (b, mut rb) = hub.connect("org", "bob", 4).expect("slot");
        let (_c, mut rc) = hub.connect("org", "cy", 4).expect("slot");
        assert_eq!(hub.join(a.id, "chat:channel.1", 10, false, None), Joined::Ok);
        assert_eq!(hub.join(b.id, "chat:channel.1", 10, false, None), Joined::Ok);
        assert_eq!(hub.publish("org", "chat:channel.1", 10, "message", json!({"id": 1}), None), 1);
        assert_eq!(hub.publish("org", "chat:channel.1", 10, "message", json!({"id": 2}), Some("bob")), 2);
        let ann = drain(&mut ra);
        assert_eq!(ann.iter().map(t).collect::<Vec<_>>(), ["sub", "msg", "msg"]);
        let bob = drain(&mut rb);
        assert_eq!(bob.iter().map(t).collect::<Vec<_>>(), ["sub", "msg"], "bob is the sender of 2");
        assert!(drain(&mut rc).is_empty(), "cy joined nothing");
        // Another organization's topic of the same name is separate.
        assert_eq!(hub.publish("other", "chat:channel.1", 10, "message", json!({}), None), 1);
        assert!(drain(&mut ra).is_empty());
    }

    #[test]
    fn a_returning_browser_resumes_from_its_last_sequence() {
        let hub = RealtimeHub::default();
        for n in 1..=5 {
            hub.publish("org", "chat:c", 3, "m", json!({ "n": n }), None);
        }
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        // Has seen 3 of 5; the ring holds 3..=5, so 4 and 5 are replayed.
        hub.join(a.id, "chat:c", 3, false, Some(3));
        let frames = drain(&mut ra);
        assert_eq!(frames.iter().map(t).collect::<Vec<_>>(), ["sub", "msg", "msg"]);
        assert_eq!(frames[1]["seq"], 4);
        // Has seen only 1: too old for the ring, so it must read from the database.
        let (b, mut rb) = hub.connect("org", "bob", 4).expect("slot");
        hub.join(b.id, "chat:c", 3, false, Some(1));
        assert_eq!(drain(&mut rb).iter().map(t).collect::<Vec<_>>(), ["sub", "resync"]);
        // A sequence from the future (the server restarted) also resyncs.
        let (c, mut rc) = hub.connect("org", "cy", 4).expect("slot");
        hub.join(c.id, "chat:c", 3, false, Some(99));
        assert!(drain(&mut rc).iter().any(|f| t(f) == "resync"));
    }

    #[test]
    fn presence_counts_a_person_once_across_tabs() {
        let hub = RealtimeHub::default();
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        hub.join(a.id, "chat:c", 0, true, None);
        let (b1, _rb1) = hub.connect("org", "bob", 4).expect("slot");
        let (b2, _rb2) = hub.connect("org", "bob", 4).expect("slot");
        hub.join(b1.id, "chat:c", 0, true, None);
        hub.join(b2.id, "chat:c", 0, true, None);
        let frames = drain(&mut ra);
        assert_eq!(frames.iter().filter(|f| t(f) == "presence").count(), 1, "bob joined once");
        assert_eq!(hub.presence("org", "chat:c"), ["ann", "bob"]);
        drop(b1);
        assert!(drain(&mut ra).is_empty(), "bob still has a tab");
        drop(b2);
        let frames = drain(&mut ra);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["leave"], "bob");
        assert_eq!(hub.presence("org", "chat:c"), ["ann"]);
    }

    #[test]
    fn a_person_is_idle_only_when_every_connection_of_theirs_is() {
        let hub = RealtimeHub::default();
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        hub.join(a.id, "chat:o", 0, true, None);
        let (b1, _rb1) = hub.connect("org", "bob", 4).expect("slot");
        let (b2, _rb2) = hub.connect("org", "bob", 4).expect("slot");
        hub.join(b1.id, "chat:o", 0, true, None);
        hub.join(b2.id, "chat:o", 0, true, None);
        drain(&mut ra);
        hub.set_idle(b1.id, "chat:o", true);
        assert!(drain(&mut ra).is_empty(), "bob has a tab that is looking");
        assert_eq!(hub.presence_state("org", "chat:o"), [("ann".to_string(), false), ("bob".to_string(), false)]);
        hub.set_idle(b2.id, "chat:o", true);
        let frames = drain(&mut ra);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["idle"], "bob");
        assert_eq!(hub.presence_state("org", "chat:o"), [("ann".to_string(), false), ("bob".to_string(), true)]);
        hub.set_idle(b2.id, "chat:o", true); // said twice: nothing new
        assert!(drain(&mut ra).is_empty());
        hub.set_idle(b1.id, "chat:o", false);
        let frames = drain(&mut ra);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["active"], "bob");
    }

    #[test]
    fn a_new_connection_wakes_an_idle_person_and_an_idle_one_leaving_does_too() {
        let hub = RealtimeHub::default();
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        let (b1, _rb1) = hub.connect("org", "bob", 4).expect("slot");
        hub.join(a.id, "chat:o", 0, true, None);
        hub.join(b1.id, "chat:o", 0, true, None);
        hub.set_idle(b1.id, "chat:o", true);
        drain(&mut ra);
        let (b2, _rb2) = hub.connect("org", "bob", 4).expect("slot");
        hub.join(b2.id, "chat:o", 0, true, None);
        let frames = drain(&mut ra);
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(frames[0]["active"], "bob");
        hub.set_idle(b2.id, "chat:o", true);
        drain(&mut ra);
        drop(b2); // the tab that was looking closes while the other is idle
        assert!(drain(&mut ra).is_empty(), "bob still has the idle connection and was idle");
        assert_eq!(hub.presence_state("org", "chat:o")[1], ("bob".to_string(), true));
    }

    #[test]
    fn the_join_answer_says_who_is_idle() {
        let hub = RealtimeHub::default();
        let (a, _ra) = hub.connect("org", "ann", 4).expect("slot");
        hub.join(a.id, "chat:o", 0, true, None);
        hub.set_idle(a.id, "chat:o", true);
        let (b, mut rb) = hub.connect("org", "bob", 4).expect("slot");
        hub.join(b.id, "chat:o", 0, true, None);
        let first = drain(&mut rb).remove(0);
        assert_eq!(first["idle"], json!(["ann"]));
        assert_eq!(first["presence"], json!(["ann", "bob"]));
    }

    #[test]
    fn client_events_reach_others_only_and_only_from_members() {
        let hub = RealtimeHub::default();
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        let (b, mut rb) = hub.connect("org", "bob", 4).expect("slot");
        let (c, _rc) = hub.connect("org", "cy", 4).expect("slot");
        hub.join(a.id, "chat:c", 0, false, None);
        hub.join(b.id, "chat:c", 0, false, None);
        drain(&mut ra);
        drain(&mut rb);
        assert!(hub.send_event(a.id, "chat:c", "typing", json!({})));
        assert!(!hub.send_event(c.id, "chat:c", "typing", json!({})), "cy is not on the topic");
        assert!(drain(&mut ra).is_empty());
        let bob = drain(&mut rb);
        assert_eq!((t(&bob[0]), bob[0]["user"].as_str()), ("event", Some("ann")));
    }

    #[test]
    fn dropping_a_person_ends_their_membership() {
        let hub = RealtimeHub::default();
        let (a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        hub.join(a.id, "chat:c", 0, false, None);
        drain(&mut ra);
        assert_eq!(hub.drop_user("org", "chat:c", "ann"), 1);
        assert_eq!(drain(&mut ra).iter().map(t).collect::<Vec<_>>(), ["dropped"]);
        hub.publish("org", "chat:c", 0, "m", json!({}), None);
        assert!(drain(&mut ra).is_empty());
    }

    #[test]
    fn the_connection_limit_and_a_slow_receiver_are_enforced() {
        let hub = RealtimeHub::default();
        let (a1, _r1) = hub.connect("org", "ann", 2).expect("slot");
        let (_a2, _r2) = hub.connect("org", "ann", 2).expect("slot");
        assert!(hub.connect("org", "ann", 2).is_none(), "a third tab is refused");
        drop(a1);
        assert!(hub.connect("org", "ann", 2).is_some(), "a slot is free again");
        // A receiver that never reads is cut off once its queue is full; others are unaffected.
        let (slow, mut slow_rx) = hub.connect("org", "slow", 4).expect("slot");
        let (fast, mut fast_rx) = hub.connect("org", "fast", 4).expect("slot");
        hub.join(slow.id, "chat:c", 0, false, None);
        hub.join(fast.id, "chat:c", 0, false, None);
        for n in 0..(QUEUE + 10) {
            hub.publish("org", "chat:c", 0, "m", json!({ "n": n }), None);
            drain(&mut fast_rx);
        }
        assert!(!hub.send_event(slow.id, "chat:c", "typing", json!({})), "slow was disconnected");
        assert!(hub.send_event(fast.id, "chat:c", "typing", json!({})));
        let mut seen = 0;
        while slow_rx.try_recv().is_ok() {
            seen += 1;
        }
        assert!(seen <= QUEUE);
    }

    #[test]
    fn close_all_ends_every_connection() {
        let hub = RealtimeHub::default();
        let (_a, mut ra) = hub.connect("org", "ann", 4).expect("slot");
        hub.close_all();
        assert_eq!(hub.open_connections(), 0);
        assert!(matches!(ra.try_recv(), Err(mpsc::error::TryRecvError::Disconnected)));
    }
}
