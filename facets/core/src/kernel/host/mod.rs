//! Kernel host commands callable by plugins (WASM guests / in-process SDK).
//!
//! Security model:
//! - Every command checks a capability (`db::query`, `db::mutate`, …).
//! - DB access is structured by **model name**; the kernel builds SurQL.
//! - There is no raw SurQL: a plugin can only touch the models it was granted, through
//!   the structured commands, so it can never reach another plugin's data.

pub mod context;
pub mod db;
pub mod dispatch;
pub mod error;

pub use context::{CallInfo, DbScope, ModelGrant, PluginHostContext};
pub use dispatch::kernel_command;
pub use error::HostError;
