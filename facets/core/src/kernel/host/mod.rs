//! Kernel host commands callable by plugins (WASM guests / in-process SDK).
//!
//! Security model:
//! - Every command checks a capability (`db::query`, `db::surql`, …).
//! - DB access is structured by **model name**; the kernel builds SurQL.
//! - Raw SurQL requires `db::surql` and is still org-scoped + denylisted.

pub mod context;
pub mod db;
pub mod dispatch;
pub mod error;

pub use context::{ModelGrant, PluginHostContext};
pub use dispatch::kernel_command;
pub use error::HostError;
