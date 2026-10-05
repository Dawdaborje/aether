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
pub mod bridge_call;
pub mod communication;
pub mod error;
pub mod files;
pub mod plugin_call;
pub mod scheduling;
pub mod http;
pub mod storage;
pub mod store;

pub use context::{BridgeHandle, CallInfo, DbScope, HostServices, JobDefaults, ModelGrant, PluginCaller, PluginHostContext, SchedulerHandle};
pub use dispatch::kernel_command;
pub use error::HostError;

#[cfg(test)]
pub(crate) mod test_support;
