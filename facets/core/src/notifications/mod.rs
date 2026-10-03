//! Notifications, driven by the kernel and delivered over server-sent events.
//!
//! The kernel (or a plugin, through the `notify::send` capability) calls
//! [`service::send`]. That stores the notification in the organization's database and
//! then wakes the people who may see it through the [`NotificationHub`]. Browsers hold
//! one `GET /api/ui/notifications/stream` connection each; nothing is ever sent from the
//! browser to the server over it, and everything else (listing, marking as read) is
//! plain REST. The table is the source of truth: a browser that reconnects asks for what
//! it missed (`Last-Event-ID`), so a dropped connection loses nothing.

pub mod api;
pub mod hub;
pub mod model;
pub mod service;
pub mod store;

pub use hub::{HubMessage, NotificationHub, StreamGuard, UiEvent};
pub use model::{Audience, Level, NewNotification, Notification, Viewer};
pub use service::{NotifyError, send, spawn_cleanup};
