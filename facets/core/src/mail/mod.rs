//! Inbound mail: a message a provider received becomes a record, a chatter line or an item for an
//! administrator (see `docs/architecture/inbound_mail.md`).

pub mod api;
pub use aether_core_base::mail::{message, thread};
pub mod service;
