//! The plugin host: the kernel commands plugins call and the plugin manager that loads and runs
//! them. It depends on `aether_core_base` only; what it needs from the rest of the server it asks
//! for through the traits in [`kernel::host::context`]. `aether_core` re-exports these modules.

pub use aether_core_base::{
    access, app_dir, cache, company_context, data_model, databases, error_pages, magic_link, mail,
    media_image, messaging, notifications, org_admin, org_storage, realtime, roles, scheduler, secrets, services,
    session_cache, user_avatar,
};

pub mod ai_jobs;
pub mod bridges;
pub mod config_manager;
pub mod import;
pub mod kernel;
pub mod member_mail;
pub mod ocr_jobs;
pub mod plugin_events;
pub mod plugin_files;
pub mod plugin_manager;
pub mod realtime_host;
pub mod sfu_host;
pub mod webhooks;
