//! Rooms and who is in them: the limits, the roster, locking and ending. No media here; the engine asks this module
//! whether a join is allowed and tells it what changed.

use std::collections::HashMap;

use crate::protocol::{Change, MediaKind, Participant, Role};

/// The name a room has inside the engine: the organization's database, the plugin and the room as the plugin names it,
/// so two organizations (or two plugins) can never land in the same room.
pub fn key(org_db: &str, plugin: &str, room: &str) -> String {
    format!("{org_db}|{plugin}:{room}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_rooms: usize,
    /// Per room; a room declared with a smaller `max_participants` uses that.
    pub max_participants: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_rooms: 100, max_participants: 50 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    Locked,
    RoomFull,
    TooManyRooms,
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            JoinError::Locked => "this meeting is locked",
            JoinError::RoomFull => "this meeting is full",
            JoinError::TooManyRooms => "this server has too many meetings open",
        })
    }
}

#[derive(Debug, Default)]
struct Room {
    people: Vec<Participant>,
    locked: bool,
}

/// All open rooms of one server. A room exists while someone is in it.
#[derive(Debug, Default)]
pub struct Rooms {
    limits: Limits,
    rooms: HashMap<String, Room>,
}

impl Rooms {
    pub fn new(limits: Limits) -> Self {
        Rooms { limits, rooms: HashMap::new() }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// Put `user` in `room`. A person who is already in it (a second tab) takes the same seat: their entry is replaced,
    /// not duplicated. `cap` is the room's own `max_participants`, if its plugin declared one.
    pub fn join(&mut self, room: &str, mut who: Participant, cap: Option<usize>, now: u64) -> Result<Change, JoinError> {
        if !self.rooms.contains_key(room) && self.rooms.len() >= self.limits.max_rooms {
            return Err(JoinError::TooManyRooms);
        }
        let limit = cap.map_or(self.limits.max_participants, |c| c.min(self.limits.max_participants));
        let entry = self.rooms.entry(room.to_string()).or_default();
        let seat = entry.people.iter().position(|p| p.user == who.user);
        // A locked room still lets back in someone who is already inside; nobody new.
        if entry.locked && seat.is_none() {
            let empty = entry.people.is_empty();
            if empty {
                self.rooms.remove(room);
            }
            return Err(JoinError::Locked);
        }
        who.since = now;
        match seat {
            Some(at) => {
                entry.people[at] = who.clone();
                Ok(Change::Update(who))
            }
            None => {
                if entry.people.len() >= limit {
                    return Err(JoinError::RoomFull);
                }
                entry.people.push(who.clone());
                Ok(Change::Join(who))
            }
        }
    }

    /// Take `user` out. The room is dropped when it empties.
    pub fn leave(&mut self, room: &str, user: &str) -> Option<Change> {
        let entry = self.rooms.get_mut(room)?;
        let at = entry.people.iter().position(|p| p.user == user)?;
        entry.people.remove(at);
        if entry.people.is_empty() {
            self.rooms.remove(room);
        }
        Some(Change::Leave { user: user.to_string() })
    }

    pub fn people(&self, room: &str) -> Vec<Participant> {
        self.rooms.get(room).map(|r| r.people.clone()).unwrap_or_default()
    }

    pub fn role_of(&self, room: &str, user: &str) -> Option<Role> {
        self.rooms.get(room)?.people.iter().find(|p| p.user == user).map(|p| p.role)
    }

    /// A person muted or unmuted themselves (or a host did it for them).
    pub fn set_muted(&mut self, room: &str, user: &str, kind: MediaKind, muted: bool) -> Option<Change> {
        let who = self.rooms.get_mut(room)?.people.iter_mut().find(|p| p.user == user)?;
        match kind {
            MediaKind::Audio => who.audio = !muted,
            MediaKind::Video => who.video = !muted,
        }
        Some(Change::Update(who.clone()))
    }

    pub fn set_screen(&mut self, room: &str, user: &str, sharing: bool) -> Option<Change> {
        let who = self.rooms.get_mut(room)?.people.iter_mut().find(|p| p.user == user)?;
        who.screen = sharing;
        Some(Change::Update(who.clone()))
    }

    /// Lock or unlock a room. Locking a room nobody is in does nothing: a room is only its people.
    pub fn set_locked(&mut self, room: &str, locked: bool) -> bool {
        match self.rooms.get_mut(room) {
            Some(r) => {
                r.locked = locked;
                true
            }
            None => false,
        }
    }

    /// End the room: everyone in it, so the caller can tell each of them.
    pub fn end(&mut self, room: &str) -> Vec<Participant> {
        self.rooms.remove(room).map(|r| r.people).unwrap_or_default()
    }

    pub fn open_rooms(&self) -> usize {
        self.rooms.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(user: &str, role: Role) -> Participant {
        Participant { sid: format!("s-{user}"), user: user.into(), name: user.into(), role, audio: true, video: true, screen: false, since: 0 }
    }

    #[test]
    fn rooms_of_other_organizations_and_plugins_differ() {
        assert_ne!(key("org_a", "meet", "m.1"), key("org_b", "meet", "m.1"));
        assert_ne!(key("org_a", "meet", "m.1"), key("org_a", "chat", "m.1"));
        assert_eq!(key("org_a", "meet", "m.1"), "org_a|meet:m.1");
    }

    #[test]
    fn a_room_lives_while_someone_is_in_it() {
        let mut rooms = Rooms::default();
        assert!(matches!(rooms.join("m.1", person("ann", Role::Host), None, 5), Ok(Change::Join(_))));
        assert_eq!(rooms.open_rooms(), 1);
        assert_eq!(rooms.people("m.1")[0].since, 5);
        assert_eq!(rooms.leave("m.1", "ann"), Some(Change::Leave { user: "ann".into() }));
        assert_eq!(rooms.open_rooms(), 0);
        assert_eq!(rooms.leave("m.1", "ann"), None);
    }

    #[test]
    fn joining_twice_takes_the_same_seat() {
        let mut rooms = Rooms::default();
        rooms.join("m.1", person("ann", Role::Member), None, 1).unwrap();
        assert!(matches!(rooms.join("m.1", person("ann", Role::Member), None, 2), Ok(Change::Update(_))));
        assert_eq!(rooms.people("m.1").len(), 1);
    }

    #[test]
    fn a_room_is_full_at_the_smaller_of_its_own_and_the_server_limit() {
        let mut rooms = Rooms::new(Limits { max_rooms: 10, max_participants: 3 });
        rooms.join("m.1", person("a", Role::Member), Some(2), 0).unwrap();
        rooms.join("m.1", person("b", Role::Member), Some(2), 0).unwrap();
        assert_eq!(rooms.join("m.1", person("c", Role::Member), Some(2), 0), Err(JoinError::RoomFull));
        // A plugin cannot ask for more than the server allows.
        rooms.join("m.2", person("a", Role::Member), Some(99), 0).unwrap();
        rooms.join("m.2", person("b", Role::Member), Some(99), 0).unwrap();
        rooms.join("m.2", person("c", Role::Member), Some(99), 0).unwrap();
        assert_eq!(rooms.join("m.2", person("d", Role::Member), Some(99), 0), Err(JoinError::RoomFull));
    }

    #[test]
    fn the_server_has_a_most_rooms() {
        let mut rooms = Rooms::new(Limits { max_rooms: 1, max_participants: 5 });
        rooms.join("m.1", person("a", Role::Member), None, 0).unwrap();
        assert_eq!(rooms.join("m.2", person("a", Role::Member), None, 0), Err(JoinError::TooManyRooms));
        assert!(rooms.join("m.1", person("b", Role::Member), None, 0).is_ok());
    }

    #[test]
    fn a_locked_room_lets_nobody_new_in_but_keeps_those_inside() {
        let mut rooms = Rooms::default();
        rooms.join("m.1", person("host", Role::Host), None, 0).unwrap();
        assert!(rooms.set_locked("m.1", true));
        assert_eq!(rooms.join("m.1", person("late", Role::Member), None, 0), Err(JoinError::Locked));
        assert!(rooms.join("m.1", person("host", Role::Host), None, 0).is_ok());
        assert!(rooms.set_locked("m.1", false));
        assert!(rooms.join("m.1", person("late", Role::Member), None, 0).is_ok());
        assert!(!rooms.set_locked("nope", true));
    }

    #[test]
    fn muting_updates_the_roster() {
        let mut rooms = Rooms::default();
        rooms.join("m.1", person("ann", Role::Member), None, 0).unwrap();
        let Some(Change::Update(p)) = rooms.set_muted("m.1", "ann", MediaKind::Audio, true) else { panic!("no update") };
        assert!(!p.audio && p.video);
        assert!(rooms.set_muted("m.1", "bob", MediaKind::Audio, true).is_none());
        let Some(Change::Update(p)) = rooms.set_screen("m.1", "ann", true) else { panic!("no update") };
        assert!(p.screen);
    }

    #[test]
    fn ending_returns_everyone_and_closes_the_room() {
        let mut rooms = Rooms::default();
        rooms.join("m.1", person("a", Role::Host), None, 0).unwrap();
        rooms.join("m.1", person("b", Role::Member), None, 0).unwrap();
        assert_eq!(rooms.end("m.1").len(), 2);
        assert_eq!(rooms.open_rooms(), 0);
        assert!(rooms.end("m.1").is_empty());
        assert_eq!(rooms.role_of("m.1", "a"), None);
    }
}
