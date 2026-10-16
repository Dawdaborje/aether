//! Plain keyword search over the page chunks. The docs are ~60k words, so scanning every chunk per
//! query is fast and needs no index, no embeddings and no network.

use crate::{Doc, Status, bundle};

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Only this section (`guide`, `architecture`, `plugin`, `capabilities`).
    pub section: Option<String>,
    /// Leave out pages that describe designs that are not built.
    pub exclude_proposals: bool,
    /// At most this many hits (default 10).
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Hit {
    /// The plugin whose docs the page belongs to; `None` for Aether's own documentation.
    pub plugin: Option<String>,
    pub slug: String,
    pub title: String,
    /// Heading trail of the matching section.
    pub heading: String,
    pub anchor: String,
    pub status: Status,
    pub score: u32,
    pub snippet: String,
}

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .filter(|t| t.len() > 1)
        .map(str::to_owned)
        .collect()
}

/// Search Aether's own documentation.
pub fn search(query: &str, opts: &SearchOptions) -> Vec<Hit> {
    search_in(&bundle().docs, query, opts)
}

/// Search any set of pages (a plugin's docs, for example). Hits have `plugin: None`; the caller names it.
pub fn search_in<'a>(docs: impl IntoIterator<Item = &'a Doc>, query: &str, opts: &SearchOptions) -> Vec<Hit> {
    let terms = tokens(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for d in docs {
        if opts.section.as_deref().is_some_and(|s| s != d.section) || (opts.exclude_proposals && d.status == Status::Proposal) {
            continue;
        }
        let title = d.title.to_lowercase();
        for c in &d.chunks {
            let body = c.markdown.to_lowercase();
            let heading = c.heading.to_lowercase();
            let mut score = 0u32;
            let mut matched = 0;
            for t in &terms {
                let in_body = body.matches(t.as_str()).count().min(8) as u32;
                let in_head = heading.contains(t.as_str()) as u32 * 6;
                let in_title = title.contains(t.as_str()) as u32 * 3;
                if in_body + in_head > 0 {
                    matched += 1;
                }
                score += in_body + in_head + in_title;
            }
            if matched == 0 {
                continue;
            }
            // Sections matching every term outrank those matching one term many times.
            score += (matched as u32) * 5;
            if matched == terms.len() {
                score += 20;
            }
            if d.status == Status::Proposal {
                score = score * 3 / 4;
            }
            hits.push(Hit {
                plugin: None,
                slug: d.slug.clone(),
                title: d.title.clone(),
                heading: c.heading.clone(),
                anchor: c.anchor.clone(),
                status: d.status,
                score,
                snippet: snippet(&c.markdown, &body, &terms),
            });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.slug.cmp(&b.slug)));
    hits.truncate(opts.limit.unwrap_or(10));
    hits
}

fn snippet(markdown: &str, lower: &str, terms: &[String]) -> String {
    let at = terms.iter().filter_map(|t| lower.find(t.as_str())).min().unwrap_or(0);
    // `to_lowercase` can change byte offsets for exotic characters; only trust it when lengths agree.
    let at = if lower.len() == markdown.len() { at } else { 0 };
    let start = markdown[..at].char_indices().rev().nth(60).map(|(i, _)| i).unwrap_or(0);
    let rest = &markdown[start..];
    let end = rest.char_indices().nth(240).map(|(i, _)| i).unwrap_or(rest.len());
    let text = rest[..end].split_whitespace().collect::<Vec<_>>().join(" ");
    let (pre, post) = (if start > 0 { "…" } else { "" }, if end < rest.len() { "…" } else { "" });
    format!("{pre}{text}{post}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_page_that_explains_a_thing() {
        let hits = search("kernel commands db::create", &SearchOptions::default());
        assert!(!hits.is_empty());
        assert!(hits.iter().take(5).any(|h| h.slug == "plugin/commands"), "{:?}", hits.iter().map(|h| h.slug.as_str()).collect::<Vec<_>>());
    }

    #[test]
    fn filters_by_section_and_proposals() {
        let only_plugin = search("model", &SearchOptions { section: Some("plugin".into()), ..Default::default() });
        assert!(only_plugin.iter().all(|h| h.slug.starts_with("plugin")));
        let built = search("audit chain", &SearchOptions { exclude_proposals: true, ..Default::default() });
        assert!(built.iter().all(|h| h.status == Status::Stable));
    }

    #[test]
    fn empty_query_finds_nothing() {
        assert!(search("  ", &SearchOptions::default()).is_empty());
    }
}
