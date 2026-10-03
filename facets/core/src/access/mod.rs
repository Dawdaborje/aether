//! Who is making a request, and the audit trail of what they did.
//!
//! * [`identity`] resolves a request to an organization and an [`audit::Actor`]:
//!   a logged-in user or an anonymous visitor.
//! * [`visitor`] issues and looks up visitor identities (the anonymous
//!   counterpart of a session).
//! * [`ratelimit`] caps anonymous requests and visitor creation per client address.
//! * [`audit`] writes the append-only trail: page visits, plugin calls and
//!   every database access a plugin makes.
//! * [`ip`] decides which client address a request came from and how it is
//!   stored.

pub mod audit;
pub mod identity;
pub mod ip;
pub mod organizations;
pub mod ratelimit;
pub mod visitor;
