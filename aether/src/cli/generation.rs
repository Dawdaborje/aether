use std::{io, path::PathBuf};

use tokio::fs;

const DEFAULT_CONFIG_TEMPLATE: &str = r#"app_dir = "app_dir"

[core]
instance_name = "Aether Test"

[server]
host = "0.0.0.0"
port = 7890
addon_paths = []
# Reverse proxies whose X-Forwarded-For header is believed. Leave empty unless
# a proxy sits in front of Aether; otherwise audit rows record the proxy's address.
# trusted_proxies = ["10.0.0.1"]

[configuration]
is_development_mode = false
is_development_with_assets = false

[database]
host = "localhost"
port = 8000
user = "root"
password = "root"
namespace = "aether"

[cache]
backend = "moka"
default_ttl_secs = 300
max_entries = 10000
max_value_bytes = 1048576

# [cache.redis]
# url = "redis://127.0.0.1:6379/0"
# key_prefix = "aether:cache:"

[media]
# Where uploaded files live: "local" or "s3". Plugins never choose the backend.
backend = "local"

# Default: <app_dir>/media. `--media-dir` overrides this.
# [media.local]
# base_path = "media"

# [media.s3]
# bucket = "aether-media"
# region = "us-east-1"
# endpoint = "http://127.0.0.1:3900"   # Garage, MinIO, … (omit for AWS)
# access_key_id = "..."                # or AWS_ACCESS_KEY_ID
# secret_access_key = "..."            # or AWS_SECRET_ACCESS_KEY
# allow_http = false
# virtual_hosted_style = false
# prefix = "aether"

# Audit trail of page visits, plugin calls and data access.
[audit]
# How client IPs are stored: "full", "truncated" (IPv4 /24, IPv6 /48) or
# "hashed" (keyed hash; needs ip_hash_key of at least 16 characters).
ip = "full"
# ip_hash_key = "change-me-to-a-long-random-secret"
# Delete audit rows older than this many days. Omit to keep them forever.
# retention_days = 365

# Limits on anonymous traffic.
[public]
# Requests one client address may make per minute while not logged in
# (logged-in users are not limited). Over the limit: 429 with Retry-After.
max_requests_per_ip_per_minute = 300
# New visitor identities one client address may create per minute.
max_new_visitors_per_ip_per_minute = 30

[tenancy]
# Public pages have no session to name an organization, so anonymous traffic
# needs "subdomain", "path" or "header" here; "session_only" serves logged-in users only.
org_resolution = "session_only"
org_header = "X-Org-Slug"
org_path_prefix = "/o"
"#;

const DEFAULT_PLUGIN_WORKSPACE_TEMPLATE: &str = r#"[workspace]
name = "example"
label = "Example Plugin"
version = "0.0.1"
description = "This is a short description of the plugin workspace."
long_description = "This is a long description of the plugin workspace."
authors = [{name = "Your Name", email = "your.email@example.com"}]
website = "https://your.website.com"
categories = []
dependencies = []

# Member plugins. `aether --gen plugin --plugin-path <name>` adds an entry
# here; a plugin can also be added by hand:
#   my_plugin = { path = "./my_plugin" }
[workspace.plugins]
"#;

/// Writes `content` to `file_path`, creating parent directories as needed.
async fn write_file(file_path: PathBuf, content: &str) -> io::Result<()> {
    log::info!("Generating file at: {:?}", file_path);

    if let Some(parent) = file_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }

    fs::write(&file_path, content).await
}

pub async fn generate_default_config_template(file_name: PathBuf) -> io::Result<()> {
    log::info!("File name: {:?}", file_name);

    write_file(file_name, DEFAULT_CONFIG_TEMPLATE).await
}

pub async fn generate_plugin_workspace(path: String) -> io::Result<()> {
    let workspace_path = PathBuf::from(path).join("workspace.toml");

    log::info!("Generating plugin workspace at: {:?}", workspace_path);

    write_file(workspace_path, DEFAULT_PLUGIN_WORKSPACE_TEMPLATE).await
}

#[cfg(test)]
mod tests {
    use aether_core::config_manager::{
        models::MediaBackendKind,
        services::{ConfigOverrides, load_config},
    };

    use super::DEFAULT_CONFIG_TEMPLATE;

    #[test]
    fn default_config_template_loads() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("aether.toml");
        std::fs::write(&path, DEFAULT_CONFIG_TEMPLATE)?;

        let config = load_config(Some(&path.to_string_lossy()), &ConfigOverrides::default())?;

        assert_eq!(config.app_dir, directory.path().join("app_dir"));
        assert_eq!(config.media.backend, MediaBackendKind::Local);
        assert_eq!(config.audit.retention_days, None);
        assert_eq!(config.public.max_new_visitors_per_ip_per_minute, 30);
        assert_eq!(config.public.max_requests_per_ip_per_minute, 300);
        assert_eq!(
            config.media.local.map(|local| local.base_path),
            Some(directory.path().join("app_dir").join("media"))
        );
        Ok(())
    }
}
