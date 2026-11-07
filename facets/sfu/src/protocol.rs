//! The frames of the signaling socket (`GET /api/ui/sfu/{room}`), as JSON text.

use serde::{Deserialize, Serialize};

/// Which of a person's two connections a candidate belongs to: the one they send on, or the one they receive on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Leg {
    Pub,
    Sub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    Low,
    Off,
}

/// What the browser sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum ClientFrame {
    /// The publisher offer: what this person sends.
    Offer { sdp: String },
    /// The answer to the server's subscriber offer.
    Answer { sdp: String },
    Candidate { pc: Leg, candidate: String },
    /// The person muted or unmuted; the media stops by itself, this tells the others.
    Mute { kind: MediaKind, muted: bool },
    /// Started or stopped sharing the screen. Sent *before* the offer that carries the screen's video line, so the server
    /// knows that line is a screen and not a camera.
    Screen { sharing: bool },
    /// What this person wants to receive from `user`.
    Want { user: String, quality: Quality },
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Host,
    Member,
}

/// One person in a room, as everybody sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    /// What the media of this person is called on the wire (the stream id of their tracks): a short token, because a
    /// user id may hold characters an SDP stream id cannot.
    #[serde(default)]
    pub sid: String,
    pub user: String,
    pub name: String,
    pub role: Role,
    pub audio: bool,
    pub video: bool,
    pub screen: bool,
    /// Unix seconds.
    pub since: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Join(Participant),
    Update(Participant),
    Leave { user: String },
}

/// What the server sends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum ServerFrame {
    Joined { you: String, role: Role, ice: Vec<IceServer>, participants: Vec<Participant> },
    /// The subscriber offer: the tracks of the others.
    Offer { sdp: String },
    /// The answer to the publisher offer.
    Answer { sdp: String },
    Candidate { pc: Leg, candidate: String },
    Participant { change: Change },
    Speaking { users: Vec<String> },
    /// A host muted this person's microphone or camera: their browser should stop sending it.
    Muted { kind: MediaKind },
    /// The person was removed, or the room ended.
    Removed { reason: String },
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_frames_use_the_documented_names() {
        let f: ClientFrame = serde_json::from_str(r#"{"op":"candidate","pc":"sub","candidate":"x"}"#).unwrap();
        assert_eq!(f, ClientFrame::Candidate { pc: Leg::Sub, candidate: "x".into() });
        let f: ClientFrame = serde_json::from_str(r#"{"op":"want","user":"ann","quality":"low"}"#).unwrap();
        assert_eq!(f, ClientFrame::Want { user: "ann".into(), quality: Quality::Low });
        assert_eq!(serde_json::from_str::<ClientFrame>(r#"{"op":"leave"}"#).unwrap(), ClientFrame::Leave);
    }

    #[test]
    fn an_unknown_op_is_refused() {
        assert!(serde_json::from_str::<ClientFrame>(r#"{"op":"explode"}"#).is_err());
        assert!(serde_json::from_str::<ClientFrame>(r#"{"sdp":"x"}"#).is_err());
    }

    #[test]
    fn server_frames_carry_their_tag() {
        let text = serde_json::to_string(&ServerFrame::Removed { reason: "ended".into() }).unwrap();
        assert_eq!(text, r#"{"t":"removed","reason":"ended"}"#);
        let text = serde_json::to_string(&ServerFrame::Speaking { users: vec!["ann".into()] }).unwrap();
        assert_eq!(text, r#"{"t":"speaking","users":["ann"]}"#);
        let text = serde_json::to_string(&ServerFrame::Muted { kind: MediaKind::Audio }).unwrap();
        assert_eq!(text, r#"{"t":"muted","kind":"audio"}"#);
    }
}
