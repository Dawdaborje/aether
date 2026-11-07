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
pub mod service;

pub use aether_core_base::notifications::{
    Audience, HubMessage, Level, NewNotification, Notification, NotificationHub, StreamGuard, UiEvent,
    Viewer, hub, model, store,
};
pub use service::{NotifyError, send, spawn_cleanup};
