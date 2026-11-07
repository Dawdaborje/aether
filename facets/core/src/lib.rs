// Modules that depend on nothing else in this crate live in `aether_core_base` and are
// re-exported here so every `crate::<module>` and `aether_core::<module>` path still works.
pub use aether_core_base::{
    app_dir, cache, company_context, data_model, databases, error_pages, magic_link, media_image, org_admin, org_storage, roles,
    secrets, services, session_cache, user_avatar,
};

pub use aether_kernel::{kernel, plugin_events};

pub mod access;
pub mod application;
pub mod bridges;
pub mod chatter;
pub mod mail;
pub mod config_manager;
pub mod media;
pub mod media_api;
pub mod bridge_credentials;
pub mod import;
pub mod inbound;
pub mod webhooks;
pub mod webhooks_api;
pub mod media_cleanup;
pub mod member_mail;
pub mod messaging;
pub mod ai_jobs;
pub mod ocr_jobs;
pub mod notifications;
pub mod pages_api;
pub mod plugin_files;
pub mod plugin_manager;
pub mod request_log;
pub mod scheduler;
pub mod realtime;
pub mod sfu;
pub mod state;
pub mod tenancy;
