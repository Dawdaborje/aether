//! Where the site gets plugin docs from. The site knows nothing about databases or sessions: whoever
//! mounts it decides what a viewer may see.

use crate::plugin_docs::{PluginDocs, Visibility};
use async_trait::async_trait;
use axum::http::HeaderMap;
use std::sync::Arc;

#[async_trait]
pub trait PluginDocsProvider: Send + Sync + 'static {
    /// The docs this viewer is allowed to read: the newest version of each plugin that has docs, sorted by
    /// label. `headers` are the request's (the session cookie is in them). A provider must leave out docs
    /// the viewer may not read, so the site cannot reveal that they exist.
    async fn visible(&self, headers: &HeaderMap) -> Vec<Arc<PluginDocs>>;
}

/// A fixed set of docs, shown to everyone as far as `allow` lets them through. Used when the site runs on
/// its own (no sessions, so only `public` docs) and in tests.
pub struct StaticDocs {
    docs: Vec<Arc<PluginDocs>>,
    public_only: bool,
}

impl StaticDocs {
    /// Everything given is shown to everyone (tests, trusted tools).
    pub fn all(docs: Vec<PluginDocs>) -> Self {
        Self { docs: Self::sorted(docs), public_only: false }
    }

    /// Only the docs marked `public` are shown, because there is no way to tell who is asking.
    pub fn public_only(docs: Vec<PluginDocs>) -> Self {
        Self { docs: Self::sorted(docs), public_only: true }
    }

    fn sorted(mut docs: Vec<PluginDocs>) -> Vec<Arc<PluginDocs>> {
        docs.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.plugin.cmp(&b.plugin)));
        docs.into_iter().map(Arc::new).collect()
    }
}

#[async_trait]
impl PluginDocsProvider for StaticDocs {
    async fn visible(&self, _headers: &HeaderMap) -> Vec<Arc<PluginDocs>> {
        self.docs.iter().filter(|d| !self.public_only || d.visibility == Visibility::Public).cloned().collect()
    }
}
