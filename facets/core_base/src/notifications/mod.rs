//! The parts of notifications that need nothing else from the kernel: the stored shapes, the
//! queries and the hub that wakes open streams. Sending and the HTTP API are in
//! `aether_core::notifications`, which re-exports these.

pub mod hub;
pub mod model;
pub mod service;
pub mod store;

pub use hub::{HubMessage, NotificationHub, StreamGuard, UiEvent};
pub use model::{Audience, Level, NewNotification, Notification, Viewer};
pub use service::{NotifyError, send};
