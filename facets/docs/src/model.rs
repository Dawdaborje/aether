//! The shape of the documentation bundle. `build.rs` writes it as JSON into `OUT_DIR` and the crate
//! embeds and reads it back, so both include this file.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub docs: Vec<Doc>,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Describes what exists.
    Stable,
    /// A design that is not built yet. Readers (people and LLMs) must not treat it as available.
    Proposal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    /// Address of the page: `plugin/pages`, `architecture/models`; a folder's README is the folder
    /// name (`plugin`), and the top README is `index`.
    pub slug: String,
    pub title: String,
    pub description: String,
    /// `guide`, `architecture`, `plugin` or `capabilities`.
    pub section: String,
    pub status: Status,
    pub order: u32,
    /// Where the markdown came from, relative to `docs/` (for generated pages: `capabilities/<code>.json`).
    pub source: String,
    pub markdown: String,
    /// Rendered body. Links to other pages are `href="doc:<slug>#anchor"`; the server swaps `doc:` for its base path.
    pub html: String,
    pub headings: Vec<Heading>,
    /// The page cut at its headings (levels 1-3), for search and for returning one section.
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// The heading trail, such as `Models › Fields`.
    pub heading: String,
    pub anchor: String,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub code: String,
    pub name: String,
    pub description: String,
    pub operations: Vec<Operation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub code: String,
    pub name: String,
    pub description: String,
}
