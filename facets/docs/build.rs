//! Builds the documentation bundle that `aether_docs` embeds.
//!
//! Reads `docs/**/*.md` and `capabilities/*.json` from the repository, checks them, renders every page
//! and writes `docs_bundle.json` into `OUT_DIR`. The binary then carries the documentation: nothing is
//! read from disk at run time, so the docs do not depend on where the source tree is.
//!
//! The build fails (rather than shipping a broken bundle) when:
//! - a page lacks its frontmatter or has an unknown `status`/`section`;
//! - a link to another `.md` page points nowhere;
//! - a page contains a machine-specific path or a credential-looking string.

#[path = "src/model.rs"]
mod model;
#[path = "src/render.rs"]
mod render;

use model::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const SECTIONS: &[&str] = &["guide", "architecture", "plugin", "capabilities"];

/// Text that must never ship inside the binary: personal paths and secrets.
const FORBIDDEN: &[&str] = &[
    "/home/",
    "/Users/",
    "C:\\Users",
    "-----BEGIN ",
    "AKIA",
    "sk_live_",
    "sk-ant-",
];

/// Names the project's own docs never use.
const FORBIDDEN_WORDS: &[&str] = &["odoo", "erpnext", "frappe"];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.join("../..");
    let docs_dir = repo.join("docs");
    let caps_dir = repo.join("capabilities");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/model.rs");
    println!("cargo:rerun-if-changed=src/render.rs");
    println!("cargo:rerun-if-changed={}", docs_dir.display());
    println!("cargo:rerun-if-changed={}", caps_dir.display());

    let mut errors: Vec<String> = Vec::new();
    let mut pages: Vec<Page> = Vec::new();

    let mut files = Vec::new();
    walk(&docs_dir, &mut files);
    files.sort();
    for file in files {
        let rel = rel_path(&docs_dir, &file);
        let raw = fs::read_to_string(&file).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        match parse_frontmatter(&raw) {
            Ok((meta, body)) => pages.push(Page { rel, meta, body }),
            Err(e) => errors.push(format!("{rel}: {e}")),
        }
    }

    let capabilities = read_capabilities(&caps_dir, &mut errors);
    pages.extend(capability_pages(&capabilities));

    // The path of every page -> its slug, for resolving links between pages.
    let mut by_path: BTreeMap<String, String> = BTreeMap::new();
    let mut slugs: HashSet<String> = HashSet::new();
    for p in &pages {
        let slug = slug_of(&p.rel);
        if !slugs.insert(slug.clone()) {
            errors.push(format!("{}: slug `{slug}` is used twice", p.rel));
        }
        by_path.insert(p.rel.clone(), slug);
    }

    let mut docs = Vec::new();
    for p in &pages {
        scan_forbidden(&p.rel, &p.body, &mut errors);
        docs.push(render_page(p, &by_path, &mut errors));
    }
    docs.sort_by(|a, b| {
        (section_rank(&a.section), a.order, &a.slug).cmp(&(section_rank(&b.section), b.order, &b.slug))
    });

    if !errors.is_empty() {
        eprintln!("\nThe documentation bundle cannot be built:");
        for e in &errors {
            eprintln!("  - {e}");
        }
        panic!("{} documentation error(s)", errors.len());
    }

    let bundle = Bundle { docs, capabilities };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("docs_bundle.json");
    fs::write(out, serde_json::to_vec(&bundle).unwrap()).unwrap();
}

struct Page {
    /// Path relative to `docs/`, always with `/` and ending in `.md` (generated pages get a virtual one).
    rel: String,
    meta: Meta,
    body: String,
}

struct Meta {
    title: String,
    description: String,
    section: String,
    status: Status,
    order: u32,
    source: String,
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

fn rel_path(base: &Path, file: &Path) -> String {
    file.strip_prefix(base)
        .unwrap()
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn slug_of(rel: &str) -> String {
    let stem = rel.strip_suffix(".md").unwrap_or(rel);
    match stem {
        "README" => "index".to_string(),
        s if s.ends_with("/README") => s.trim_end_matches("/README").to_string(),
        s => s.to_string(),
    }
}

fn section_rank(s: &str) -> usize {
    SECTIONS.iter().position(|x| *x == s).unwrap_or(usize::MAX)
}

/// `---` block of `key: value` lines (values optionally in double quotes), then the body.
fn parse_frontmatter(raw: &str) -> Result<(Meta, String), String> {
    let rest = raw
        .strip_prefix("---\n")
        .ok_or("missing frontmatter (the file must start with `---`)")?;
    let end = rest.find("\n---\n").ok_or("frontmatter is not closed with `---`")?;
    let (block, body) = (&rest[..end], &rest[end + 5..]);

    let mut map: BTreeMap<&str, String> = BTreeMap::new();
    for line in block.lines() {
        let (k, v) = line.split_once(':').ok_or_else(|| format!("bad frontmatter line `{line}`"))?;
        map.insert(k.trim(), unquote(v.trim()));
    }
    let get = |k: &str| map.get(k).cloned().filter(|v| !v.is_empty()).ok_or(format!("frontmatter `{k}` is missing or empty"));

    let section = get("section")?;
    if !SECTIONS.contains(&section.as_str()) {
        return Err(format!("unknown section `{section}` (use one of {SECTIONS:?})"));
    }
    let status = match get("status")?.as_str() {
        "stable" => Status::Stable,
        "proposal" => Status::Proposal,
        other => return Err(format!("unknown status `{other}` (use `stable` or `proposal`)")),
    };
    let order = get("order")?.parse::<u32>().map_err(|_| "frontmatter `order` is not a number".to_string())?;
    Ok((
        Meta { title: get("title")?, description: get("description")?, section, status, order, source: String::new() },
        body.to_string(),
    ))
}

fn unquote(v: &str) -> String {
    match v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(inner) => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => v.to_string(),
    }
}

fn scan_forbidden(rel: &str, text: &str, errors: &mut Vec<String>) {
    for bad in FORBIDDEN {
        if text.contains(bad) {
            errors.push(format!("{rel}: contains `{bad}`, which must not ship in the binary"));
        }
    }
    let lower = text.to_lowercase();
    for w in FORBIDDEN_WORDS {
        if lower.contains(w) {
            errors.push(format!("{rel}: contains the word `{w}`"));
        }
    }
}

// ---- capabilities -----------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawCapability {
    code: String,
    name: String,
    description: String,
    capabilities: Vec<RawOperation>,
}

#[derive(Deserialize)]
struct RawOperation {
    code: String,
    name: String,
    description: String,
}

fn read_capabilities(dir: &Path, errors: &mut Vec<String>) -> Vec<Capability> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect())
        .unwrap_or_default();
    paths.sort();
    let mut out = Vec::new();
    for p in paths {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let parsed = fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<RawCapability>(&s).map_err(|e| e.to_string()));
        match parsed {
            Ok(c) => out.push(Capability {
                code: c.code,
                name: c.name,
                description: c.description,
                operations: c
                    .capabilities
                    .into_iter()
                    .map(|o| Operation { code: o.code, name: o.name, description: o.description })
                    .collect(),
            }),
            Err(e) => errors.push(format!("capabilities/{name}: {e}")),
        }
    }
    out
}

/// One generated page per capability group, plus an index.
fn capability_pages(caps: &[Capability]) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut index = String::from(
        "A plugin can only do what its `capabilities` list in `plugin.toml` allows. Each entry is \
         `<group>::<operation>`, for example `db::query`. The kernel refuses any kernel command the \
         plugin did not declare.\n\n| Group | What it is for |\n|---|---|\n",
    );
    for (i, c) in caps.iter().enumerate() {
        index.push_str(&format!("| [{}]({}.md) (`{}::`) | {} |\n", c.name, c.code, c.code, one_line(&c.description)));
        let mut body = format!("{}\n\nGroup code: `{}`.\n\n| Capability | Name | Allows |\n|---|---|---|\n", c.description, c.code);
        for o in &c.operations {
            body.push_str(&format!("| `{}::{}` | {} | {} |\n", c.code, o.code, o.name, one_line(&o.description)));
        }
        body.push_str("\nSee [Kernel commands](../plugin/commands.md) for the commands each capability unlocks.\n");
        pages.push(Page {
            rel: format!("capabilities/{}.md", c.code),
            meta: Meta {
                title: format!("{} ({}::)", c.name, c.code),
                description: one_line(&c.description),
                section: "capabilities".into(),
                status: Status::Stable,
                order: (i as u32 + 1) * 10,
                source: format!("capabilities/{}.json", c.code),
            },
            body,
        });
    }
    pages.push(Page {
        rel: "capabilities/README.md".into(),
        meta: Meta {
            title: "Capabilities".into(),
            description: "Every capability a plugin can declare, grouped by kernel feature.".into(),
            section: "capabilities".into(),
            status: Status::Stable,
            order: 0,
            source: "capabilities/*.json".into(),
        },
        body: index,
    });
    pages
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

// ---- rendering ----------------------------------------------------------------------------------

fn render_page(page: &Page, by_path: &BTreeMap<String, String>, errors: &mut Vec<String>) -> Doc {
    let r = render::render(&page.rel, &page.body, &page.meta.title, true, &|target| by_path.get(target).cloned(), errors);
    let source = if page.meta.source.is_empty() { page.rel.clone() } else { page.meta.source.clone() };
    Doc {
        slug: slug_of(&page.rel),
        title: page.meta.title.clone(),
        description: page.meta.description.clone(),
        section: page.meta.section.clone(),
        status: page.meta.status,
        order: page.meta.order,
        source,
        markdown: page.body.clone(),
        html: r.html,
        headings: r.headings,
        chunks: r.chunks,
    }
}
