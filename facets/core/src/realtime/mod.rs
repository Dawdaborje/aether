//! Realtime: a WebSocket with topics, presence and client events (see `docs/architecture/realtime.md`).
//!
//! - [`hub`]: who is on which topic, sequence numbers, the short history used to resume.
//! - [`ws`]: `GET /api/ui/realtime`, the upgrade and the frame loop.
//! - [`host`]: the `realtime::publish`, `realtime::drop` and `realtime::presence` commands for plugins.

pub use aether_kernel::realtime_host as host;
pub use aether_core_base::realtime::{RealtimeHub, hub};
pub mod ws;

