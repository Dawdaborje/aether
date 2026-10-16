//! Markdown to HTML, shared by `build.rs` (the project's own docs, at compile time) and by the crate itself
//! (plugin docs, when a plugin is loaded). One implementation, so both behave the same.
//!
//! Plugin authors are not trusted: for them raw HTML in the markdown is shown as text, never passed through
//! (and the crate sanitizes the result again; see `plugin_docs`).

use crate::model::{Chunk, Heading};
use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Component, Path};

pub struct Rendered {
    /// Links to other pages are `href="doc:<slug>#anchor"`; the site swaps `doc:` for the right base path.
    pub html: String,
    pub headings: Vec<Heading>,
    /// The page cut at its headings (levels 1-3).
    pub chunks: Vec<Chunk>,
}

/// Renders `body`, the markdown of the page at `rel` (a `/` path ending in `.md`, relative to the docs root).
///
/// `resolve` turns the path of another page (relative to the docs root) into its slug, or `None` when there
/// is no such page; a relative link to a missing page is an error pushed to `errors`.
/// `trusted` allows raw HTML in the markdown.
pub fn render(
    rel: &str,
    body: &str,
    title: &str,
    trusted: bool,
    resolve: &dyn Fn(&str) -> Option<String>,
    errors: &mut Vec<String>,
) -> Rendered {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let events: Vec<(Event, Range<usize>)> = Parser::new_ext(body, opts).into_offset_iter().collect();

    // Headings: text, GitHub-style ids (so `file.md#some-heading` links keep working), source offset.
    struct H {
        level: u8,
        id: String,
        text: String,
        start: usize,
    }
    let mut hs: Vec<H> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut open: Option<(usize, String)> = None;
    for (ev, range) in &events {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                hs.push(H { level: level_num(*level), id: String::new(), text: String::new(), start: range.start });
                open = Some((hs.len() - 1, String::new()));
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, buf)) = open.as_mut() {
                    buf.push_str(t);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((i, text)) = open.take() {
                    let base = slugify(&text);
                    let mut id = base.clone();
                    let mut n = 1;
                    while !used.insert(id.clone()) {
                        id = format!("{base}-{n}");
                        n += 1;
                    }
                    hs[i].id = id;
                    hs[i].text = text;
                }
            }
            _ => {}
        }
    }

    // Chunks: from one heading (levels 1-3) up to the next one.
    let cuts: Vec<usize> = (0..hs.len()).filter(|&i| hs[i].level <= 3).collect();
    let mut chunks = Vec::new();
    if let Some(&first) = cuts.first() {
        if hs[first].start > 0 && !body[..hs[first].start].trim().is_empty() {
            chunks.push(Chunk { heading: title.to_string(), anchor: String::new(), markdown: body[..hs[first].start].trim().to_string() });
        }
    } else if !body.trim().is_empty() {
        chunks.push(Chunk { heading: title.to_string(), anchor: String::new(), markdown: body.trim().to_string() });
    }
    let mut trail: Vec<(u8, String)> = Vec::new();
    for (n, &i) in cuts.iter().enumerate() {
        let h = &hs[i];
        while trail.last().is_some_and(|(l, _)| *l >= h.level) {
            trail.pop();
        }
        trail.push((h.level, h.text.clone()));
        let end = cuts.get(n + 1).map(|&j| hs[j].start).unwrap_or(body.len());
        chunks.push(Chunk {
            heading: trail.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" › "),
            anchor: h.id.clone(),
            markdown: body[h.start..end].trim().to_string(),
        });
    }

    // Events -> HTML, with heading ids and links between pages.
    let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut next_heading = 0;
    let mut out_events: Vec<Event> = Vec::with_capacity(events.len());
    for (ev, _) in events {
        out_events.push(match ev {
            // Untrusted markdown never reaches the page as HTML: it is shown as the text it is.
            Event::Html(t) | Event::InlineHtml(t) if !trusted => Event::Text(t),
            Event::Start(Tag::Heading { level, .. }) => {
                let h = &hs[next_heading];
                next_heading += 1;
                Event::Html(format!("<h{} id=\"{}\">", level_num(level), h.id).into())
            }
            Event::End(TagEnd::Heading(level)) => Event::Html(format!("</h{}>\n", level_num(level)).into()),
            // Plugin docs cannot ship image files, so an image must be a web address.
            Event::Start(Tag::Image { link_type, dest_url, title, id }) if !trusted && !dest_url.starts_with("https://") => {
                errors.push(format!("{rel}: image `{dest_url}` must be an https:// address (relative images are not served)"));
                Event::Start(Tag::Image { link_type, dest_url: CowStr::from(""), title, id })
            }
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let dest = rewrite_link(rel, dir, &dest_url, resolve, errors);
                Event::Start(Tag::Link { link_type, dest_url: CowStr::from(dest), title, id })
            }
            other => other,
        });
    }
    let mut html_out = String::new();
    html::push_html(&mut html_out, out_events.into_iter());

    Rendered {
        html: html_out,
        headings: hs.into_iter().map(|h| Heading { level: h.level, id: h.id, text: h.text }).collect(),
        chunks,
    }
}

fn level_num(l: HeadingLevel) -> u8 {
    l as u8
}

/// GitHub's anchor rule: lower-case, spaces to `-`, anything but letters, digits, `-` and `_` dropped.
pub fn slugify(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

fn rewrite_link(from: &str, dir: &str, dest: &str, resolve: &dyn Fn(&str) -> Option<String>, errors: &mut Vec<String>) -> String {
    if dest.contains("://") || dest.starts_with('#') || dest.starts_with("mailto:") {
        return dest.to_string();
    }
    // `javascript:`, `data:` and the like: not a page and not a web address.
    if let Some((scheme, _)) = dest.split_once(':')
        && !scheme.contains(['/', '.', '#'])
    {
        errors.push(format!("{from}: link `{dest}` uses the unsupported scheme `{scheme}:`; link to a .md page, an https:// address or an #anchor"));
        return "#".to_string();
    }
    let (path, anchor) = match dest.split_once('#') {
        Some((p, a)) => (p, format!("#{a}")),
        None => (dest, String::new()),
    };
    if !path.ends_with(".md") {
        errors.push(format!("{from}: link `{dest}` is relative but not to a .md page"));
        return dest.to_string();
    }
    let mut parts: Vec<&str> = if dir.is_empty() { vec![] } else { dir.split('/').collect() };
    for c in Path::new(path).components() {
        match c {
            Component::ParentDir => {
                if parts.pop().is_none() {
                    errors.push(format!("{from}: link `{dest}` leaves the documentation folder"));
                    return dest.to_string();
                }
            }
            Component::Normal(s) => parts.push(s.to_str().unwrap_or_default()),
            _ => {}
        }
    }
    let target = parts.join("/");
    match resolve(&target) {
        Some(slug) => format!("doc:{slug}{anchor}"),
        None => {
            errors.push(format!("{from}: link `{dest}` points to a page that does not exist (`{target}`)"));
            dest.to_string()
        }
    }
}
