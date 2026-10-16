//! Aether's documentation, compiled into the binary.
//!
//! `build.rs` checks and renders `docs/` and `capabilities/` into one bundle; this crate embeds it and
//! offers lookup and search. The docs site (`--serve-docs`, `/docs`) and the MCP server (`--mcp`) both
//! read from here, so they can never disagree and neither needs the source tree at run time.

mod export;
mod model;
pub mod plugin_docs;
mod provider;
mod render;
mod search;
mod site;

pub use model::{Bundle, Capability, Chunk, Doc, Heading, Operation, Status};
pub use search::{Hit, SearchOptions, search, search_in};
pub use export::export_site;
pub use provider::{PluginDocsProvider, StaticDocs};
pub use site::{router, router_with};

use std::sync::OnceLock;

static BUNDLE_JSON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/docs_bundle.json"));

/// The embedded documentation, parsed on first use.
pub fn bundle() -> &'static Bundle {
    static BUNDLE: OnceLock<Bundle> = OnceLock::new();
    BUNDLE.get_or_init(|| serde_json::from_slice(BUNDLE_JSON).expect("the embedded docs bundle is valid"))
}

/// A page by slug (`plugin/pages`). A trailing or leading `/`, a `.md` suffix and the
/// `README` name of a folder are accepted, so what a reader or an LLM types resolves.
pub fn doc(slug: &str) -> Option<&'static Doc> {
    let s = slug.trim_matches('/');
    let s = s.strip_suffix(".md").unwrap_or(s);
    let s = s.strip_suffix("/README").unwrap_or(s);
    let s = if s.is_empty() || s == "README" { "index" } else { s };
    bundle().docs.iter().find(|d| d.slug == s)
}

/// A capability group by its code (`db`).
pub fn capability(code: &str) -> Option<&'static Capability> {
    bundle().capabilities.iter().find(|c| c.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_has_the_sections() {
        let b = bundle();
        assert!(b.docs.len() > 40, "{} docs", b.docs.len());
        for section in ["guide", "architecture", "plugin", "capabilities"] {
            assert!(b.docs.iter().any(|d| d.section == section), "no {section} pages");
        }
        assert!(b.capabilities.len() >= 19);
    }

    #[test]
    fn lookup_is_forgiving() {
        let models = doc("architecture/models").expect("models page");
        assert_eq!(models.title, "Models");
        assert_eq!(doc("/architecture/models.md").map(|d| &d.slug), Some(&models.slug));
        assert_eq!(doc("plugin/README.md").map(|d| d.slug.as_str()), Some("plugin"));
        assert_eq!(doc("README.md").map(|d| d.slug.as_str()), Some("index"));
        assert!(doc("nope/missing").is_none());
    }

    #[test]
    fn proposals_are_marked() {
        assert_eq!(doc("architecture/history").unwrap().status, Status::Proposal);
        assert_eq!(doc("architecture/models").unwrap().status, Status::Stable);
    }

    #[test]
    fn page_links_are_rewritten_and_headings_get_ids() {
        let page = doc("architecture/models").unwrap();
        assert!(!page.html.contains("href=\"./"), "an unrewritten relative link");
        assert!(page.html.contains("<h2 id="));
        assert!(page.chunks.len() > 3);
        assert!(doc("architecture").unwrap().html.contains("href=\"doc:architecture/"));
    }

    #[test]
    fn capability_pages_are_generated() {
        let c = capability("db").expect("db capability");
        assert!(c.operations.iter().any(|o| o.code == "query"));
        let page = doc("capabilities/db").expect("db page");
        assert!(page.markdown.contains("`db::query`"));
    }

    #[test]
    fn no_machine_specific_text_in_the_bundle() {
        for d in &bundle().docs {
            assert!(!d.markdown.contains("/home/"), "{}", d.slug);
        }
    }
}
