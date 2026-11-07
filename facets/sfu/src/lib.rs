//! The pure logic of the built-in meeting server (see `docs/architecture/meet_sfu.md`): the signaling frames, the
//! rooms and who is in them, and TURN credentials. The media engine and the kernel wiring are built on top of it.

pub mod engine;
pub mod layers;
pub mod protocol;
pub mod room;
pub mod speaker;
pub mod turn;
