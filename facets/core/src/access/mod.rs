//! Who is making a request, and the audit trail of what they did.
//!
//! * [`identity`] resolves a request to an organization and an [`audit::Actor`]:
//!   a logged-in user or an anonymous visitor.
//! * [`visitor`] issues and looks up visitor identities (the anonymous
//!   counterpart of a session).
//! * [`ratelimit`] caps anonymous requests and visitor creation per client address.
//! * [`audit`] writes the append-only trail: page visits, plugin calls and
//!   every database access a plugin makes.
//! * [`chain`] makes the trail tamper-evident; [`chain_service`] seals and checks it in the background.
//! * [`ip`] decides which client address a request came from and how it is
//!   stored.

pub use aether_core_base::access::{api_token, chain, ratelimit};
pub mod chain_service;
pub mod identity;
pub mod ip;
pub mod org_databases;
pub mod organizations;
mod retention;

/// The audit trail (`aether_core_base::access::audit`) and the task that applies its retention.
pub mod audit {
    pub use aether_core_base::access::audit::*;

    pub use super::retention::spawn_retention_task;
}
pub mod visitor;
