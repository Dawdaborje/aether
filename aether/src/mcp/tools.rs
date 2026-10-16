//! What the MCP tools do, as plain functions from arguments to text. `server.rs` only adapts them
//! to the protocol, so this file has no MCP types and is tested directly.
//!
//! Every function answers an LLM, so its text says what to do next (which page to open, how to fix
//! a mistake) rather than only reporting.

use crate::cli::scaffold::{self, Language, PluginDetails};
use aether_docs::{Doc, SearchOptions, Status, bundle, capability, doc, search};
use std::fmt::Write;
use std::path::Path;

pub const SECTIONS: &[&str] = &["guide", "plugin", "architecture", "capabilities"];

/// What `get_reference` can return: the pages that define each file format or feature.
pub const REFERENCES: &[(&str, &[&str], &str)] = &[
    ("manifest", &["plugin"], "plugin.toml: name, version, capabilities, functions, schedules, commands, roles; and workspace.toml"),
    ("model", &["architecture/models"], "models/<name>.json: fields, types, ids, rules, views"),
    ("pages", &["plugin/pages", "plugin/components"], "pages/*.xml and components/*.xml: widgets, data, layout"),
    ("rules", &["architecture/rules"], "rules/<model>.json: who may see and change records and fields"),
    ("commands", &["plugin/commands"], "Kernel commands a plugin sends (db::find, notify::send, ...) and the capability each needs"),
    ("cli-commands", &["plugin/cli-commands"], "Commands a plugin offers to administrators on the command line"),
    ("lua", &["plugin/lua"], "Writing a plugin as a Lua script"),
    ("rhai", &["plugin/rhai"], "Writing a plugin as a Rhai script"),
    ("events", &["architecture/events"], "Events between plugins"),
    ("scheduler", &["architecture/scheduler"], "Background jobs and recurring tasks"),
    ("capabilities", &["capabilities"], "Every capability a plugin can declare"),
];

fn status_note(d: &Doc) -> &'static str {
    match d.status {
        Status::Stable => "",
        Status::Proposal => {
            "> **PROPOSAL, NOT BUILT.** This page describes a design. The feature does not exist in Aether yet, so do not write plugins that rely on it.\n\n"
        }
    }
}

fn slug_help() -> &'static str {
    "Give a page path as in `list_docs` (for example `plugin/pages`), without `.md`."
}

pub fn search_docs(query: &str, section: Option<&str>, include_proposals: bool, limit: Option<usize>) -> Result<String, String> {
    if let Some(s) = section
        && !SECTIONS.contains(&s)
    {
        return Err(format!("Unknown section `{s}`. Use one of: {}.", SECTIONS.join(", ")));
    }
    if query.trim().is_empty() {
        return Err("Give some words to search for, for example `db::find filter` or `model field types`.".into());
    }
    let hits = search(
        query,
        &SearchOptions { section: section.map(str::to_owned), exclude_proposals: !include_proposals, limit: Some(limit.unwrap_or(8).clamp(1, 25)) },
    );
    if hits.is_empty() {
        return Ok(format!(
            "No section matched `{query}`. Try other words, drop `section`, or set `include_proposals` (designs that are not built). `list_docs` shows every page."
        ));
    }
    let mut out = format!("{} section(s) match `{query}`:\n", hits.len());
    for (i, h) in hits.iter().enumerate() {
        let proposal = if h.status == Status::Proposal { " [PROPOSAL, not built]" } else { "" };
        let _ = write!(out, "\n{}. {} › {}{}\n   get_doc path=\"{}\"", i + 1, h.title, h.heading, proposal, h.slug);
        if !h.anchor.is_empty() {
            let _ = write!(out, " section=\"{}\"", h.anchor);
        }
        let _ = write!(out, "\n   {}\n", h.snippet);
    }
    Ok(out)
}

pub fn get_doc(path: &str, section: Option<&str>) -> Result<String, String> {
    let Some(d) = doc(path) else {
        return Err(not_found(path));
    };
    let mut out = format!("<!-- aether docs: {} · {} · {} -->\n\n{}", d.slug, d.section, if d.status == Status::Proposal { "proposal" } else { "stable" }, status_note(d));
    match section.map(str::trim).filter(|s| !s.is_empty()) {
        None => out.push_str(&d.markdown),
        Some(wanted) => {
            let w = wanted.trim_start_matches('#').to_lowercase();
            let hits: Vec<_> = d
                .chunks
                .iter()
                .filter(|c| c.anchor == w || c.heading.to_lowercase().contains(&w))
                .collect();
            if hits.is_empty() {
                let mut names = String::new();
                for c in d.chunks.iter().filter(|c| !c.anchor.is_empty()) {
                    let _ = writeln!(names, "- {} (section=\"{}\")", c.heading, c.anchor);
                }
                return Err(format!("Page `{}` has no section matching `{wanted}`. Its sections:\n{names}", d.slug));
            }
            for c in hits {
                out.push_str(&c.markdown);
                out.push_str("\n\n");
            }
        }
    }
    out.push_str("\n\n---\nRelative links in this page point to other pages: open them with get_doc using the path without `.md`.");
    Ok(out)
}

fn not_found(path: &str) -> String {
    let wanted = path.trim_matches('/').trim_end_matches(".md");
    let mut near: Vec<&str> = bundle()
        .docs
        .iter()
        .filter(|d| d.slug.contains(wanted) || wanted.contains(d.slug.as_str()) || d.slug.rsplit('/').next() == wanted.rsplit('/').next())
        .map(|d| d.slug.as_str())
        .take(5)
        .collect();
    near.dedup();
    let hint = if near.is_empty() { "Use list_docs to see every page.".to_string() } else { format!("Did you mean: {}?", near.join(", ")) };
    format!("There is no page `{path}`. {} {hint}", slug_help())
}

pub fn list_docs(section: Option<&str>) -> Result<String, String> {
    if let Some(s) = section
        && !SECTIONS.contains(&s)
    {
        return Err(format!("Unknown section `{s}`. Use one of: {}.", SECTIONS.join(", ")));
    }
    let mut out = String::from("Pages of the Aether documentation. `[proposal]` pages are designs that are NOT built.\n");
    for name in SECTIONS.iter().filter(|n| section.is_none_or(|s| s == **n)) {
        let _ = write!(out, "\n## {name}\n");
        for d in bundle().docs.iter().filter(|d| d.section == *name) {
            let mark = if d.status == Status::Proposal { " [proposal]" } else { "" };
            let _ = writeln!(out, "- {}{} — {}", d.slug, mark, d.description);
        }
    }
    Ok(out)
}

pub fn list_capabilities() -> String {
    let mut out = String::from("Capability groups. A plugin lists the ones it needs in `plugin.toml` as `capabilities = [\"db::query\", ...]`; the kernel refuses anything not listed. `get_capability` shows a group.\n\n");
    for c in &bundle().capabilities {
        let _ = writeln!(out, "- {}:: ({}) — {}", c.code, c.name, one_line(&c.description));
    }
    out
}

pub fn get_capability(code: &str) -> Result<String, String> {
    let code = code.trim().trim_end_matches(':');
    // `db::query` names one capability; show its group.
    let (group, only) = code.split_once("::").map(|(g, o)| (g, Some(o))).unwrap_or((code, None));
    let Some(c) = capability(group) else {
        let all: Vec<&str> = bundle().capabilities.iter().map(|c| c.code.as_str()).collect();
        return Err(format!("There is no capability group `{group}`. Groups: {}.", all.join(", ")));
    };
    let mut out = format!("# {} (`{}::`)\n\n{}\n\n", c.name, c.code, c.description);
    for o in c.operations.iter().filter(|o| only.is_none_or(|w| w == o.code)) {
        let _ = writeln!(out, "- `{}::{}` — {}: {}", c.code, o.code, o.name, one_line(&o.description));
    }
    if let Some(w) = only
        && !c.operations.iter().any(|o| o.code == w)
    {
        return Err(format!("`{}::` has no capability `{w}`. It has: {}.", c.code, c.operations.iter().map(|o| o.code.as_str()).collect::<Vec<_>>().join(", ")));
    }
    out.push_str("\nSee `get_reference` kind=\"commands\" for the kernel commands each capability unlocks.");
    Ok(out)
}

pub fn get_reference(kind: &str) -> Result<String, String> {
    let kind = kind.trim().to_lowercase();
    let Some((_, slugs, _)) = REFERENCES.iter().find(|(k, _, _)| *k == kind) else {
        let kinds: Vec<String> = REFERENCES.iter().map(|(k, _, what)| format!("- {k}: {what}")).collect();
        return Err(format!("Unknown reference `{kind}`. Kinds:\n{}", kinds.join("\n")));
    };
    if kind == "capabilities" {
        return Ok(list_capabilities());
    }
    let mut out = String::new();
    for slug in *slugs {
        let d = doc(slug).ok_or_else(|| format!("The reference page `{slug}` is missing from this build."))?;
        let _ = write!(out, "<!-- aether docs: {} -->\n\n{}{}\n\n", d.slug, status_note(d), d.markdown);
    }
    let schema = match kind.as_str() {
        "manifest" => Some("manifest` and `workspace"),
        "model" => Some("model"),
        "rules" => Some("rules"),
        _ => None,
    };
    if let Some(kinds) = schema {
        let _ = write!(out, "---\nThe exact structure of this file format is available as JSON Schema: `get_schema` kind `{kinds}`.");
    }
    Ok(out)
}

/// The JSON Schema of a file format, generated from the types the kernel parses it with.
pub fn get_schema(kind: &str) -> Result<String, String> {
    use aether_core::schemas::{KINDS, schema};
    let kind = kind.trim().to_lowercase();
    let Some(value) = schema(&kind) else {
        let kinds: Vec<String> = KINDS.iter().map(|(k, file)| format!("- {k}: {file}")).collect();
        return Err(format!("There is no schema `{kind}`. Schemas:\n{}", kinds.join("\n")));
    };
    let file = KINDS.iter().find(|(k, _)| *k == kind).map(|(_, f)| *f).unwrap_or_default();
    Ok(format!(
        "JSON Schema of `{file}`, generated from the types the kernel loads it with, so it is exact. A TOML file is described as the \
         JSON it equals: a table is an object and `[[x]]` is an array under `x`. Descriptions are the kernel's own documentation of each field. \
         The schema checks structure only; `validate_plugin` runs the full checks. `get_reference` has the explanations and examples.\n\n{}",
        serde_json::to_string(&value).map_err(|e| e.to_string())?
    ))
}

pub fn languages() -> &'static str {
    "go, rust, typescript, javascript, python, rhai, lua (rhai and lua are scripts with nothing to compile)"
}

/// Generates a plugin in a temporary folder with the same templates as `aether --gen plugin`, and returns
/// its files as text. Nothing is written to the caller's disk: the caller creates the files itself.
pub async fn scaffold_plugin(name: &str, language: &str, label: Option<&str>, description: Option<&str>) -> Result<String, String> {
    let language: Language = language.parse().map_err(|_| format!("Unknown language `{language}`. Use one of: {}.", languages()))?;
    let dir = tempfile::tempdir().map_err(|e| format!("Could not make a scratch folder: {e}"))?;
    let target = dir.path().join(name);
    let details = PluginDetails { label: label.map(str::to_owned), description: description.map(str::to_owned), author: None };
    let created = scaffold::create_plugin_with(&target, language, &details).await.map_err(|e| e.to_string())?;

    let mut files = Vec::new();
    collect(&target, &target, &mut files).map_err(|e| format!("Could not read the generated plugin: {e}"))?;
    files.sort();
    // A licence is a choice for the author, and its text is long: leave it out.
    files.retain(|(path, _)| path != "LICENSE");
    let mut out = format!(
        "Plugin `{}` ({}). Create these files in a folder named `{}`; paths are relative to it.\n",
        created.name,
        created.language.label(),
        created.name
    );
    for (path, content) in &files {
        let _ = write!(out, "\n=== FILE: {path} ===\n{content}\n=== END FILE ===\n");
    }
    out.push_str("\n(No LICENSE file is included: choose one for your plugin.)\nNext: edit the files, then run validate_plugin on the folder and fix what it reports. ");
    if created.language.is_wasm() {
        out.push_str("The README explains how to build the WebAssembly module.");
    } else {
        out.push_str("A script plugin has nothing to compile.");
    }
    Ok(out)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            out.push((rel, text));
        }
    }
    Ok(())
}

/// Checks a plugin folder the way a load does, without a database. See
/// `aether_core::plugin_manager::catalog::check_plugin_offline` for what that covers.
pub async fn validate_plugin(path: &str) -> Result<String, String> {
    let path = Path::new(path.trim());
    if !path.exists() {
        return Err(format!("`{}` does not exist. Give the plugin folder (the one with plugin.toml), as an absolute path or relative to where the MCP server was started.", path.display()));
    }
    match aether_core::plugin_manager::catalog::check_plugin_offline(path).await {
        Ok(ok) => {
            let mut out = format!(
                "VALID: `{}` {} — {} model(s){}, {} page(s), {} component(s).\n\nNot checked by this tool (`aether --load-plugins --dry-run` also checks what needs a database):\n",
                ok.name,
                ok.version,
                ok.models.len(),
                if ok.models.is_empty() { String::new() } else { format!(" ({})", ok.models.join(", ")) },
                ok.pages,
                ok.components
            );
            for s in ok.skipped {
                let _ = writeln!(out, "- {s}");
            }
            Ok(out)
        }
        Err(e) => Err(format!(
            "INVALID: {e}\n\nFix this and run validate_plugin again; a load stops at the first problem. The pages `get_reference` returns (kind `manifest`, `model`, `pages`) describe the formats."
        )),
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- prompts ----------------------------------------------------------------------------------

pub fn build_plugin_prompt(goal: &str, language: Option<&str>) -> String {
    let language = language.filter(|l| !l.trim().is_empty()).unwrap_or("rhai");
    format!(
        "You are building an Aether plugin. Goal: {goal}\n\n\
         Language: {language} (choose another only if the goal needs it; rhai and lua are scripts with nothing to compile).\n\n\
         Work in this order:\n\
         1. Read what exists before writing anything: `get_reference` kind=\"manifest\", then \"model\" and \"pages\" as the goal needs. Use `search_docs` for anything else. Pages marked PROPOSAL are not built: never rely on them.\n\
         2. `scaffold_plugin` to get the starting files, and create them in the user's project.\n\
         3. Declare every capability the plugin uses in plugin.toml (`list_capabilities`, `get_capability`); the kernel refuses anything undeclared.\n\
         4. Write models (models/*.json), pages (pages/*.xml) and the code. Keep to what the documentation says; if it does not say how, search again instead of guessing.\n\
         5. `validate_plugin` on the plugin folder. Fix every problem it reports and run it again until it says VALID.\n\
         6. Tell the user how to load and try it: `aether --load-plugins <folder>`, then `aether --install-plugin <name> --org <org>`.\n"
    )
}

pub fn write_model_prompt(description: &str) -> String {
    format!(
        "Write an Aether model (models/<name>.json) for: {description}\n\n\
         First read `get_reference` kind=\"model\" in full: field types, ids, rules and views. Then `search_docs` for any field type you are unsure of. \
         Write the JSON, put it in the plugin's models/ folder, and run `validate_plugin` on the plugin. Ids are assigned by `aether --sync-models <plugin folder>`; do not invent them."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_points_at_the_page_to_open() {
        let out = search_docs("kernel commands db::create", None, false, None).unwrap();
        assert!(out.contains("get_doc path=\"plugin/commands\""), "{out}");
        assert!(search_docs("zzzqqqxxx", None, false, None).unwrap().contains("No section matched"));
        assert!(search_docs("x", Some("nope"), false, None).is_err());
        assert!(search_docs("  ", None, false, None).is_err());
    }

    #[test]
    fn search_hides_proposals_unless_asked() {
        let hidden = search_docs("audit chain tamper", None, false, None).unwrap();
        assert!(!hidden.contains("[PROPOSAL"), "{hidden}");
        let shown = search_docs("audit chain tamper", None, true, None).unwrap();
        assert!(shown.contains("[PROPOSAL, not built]"), "{shown}");
    }

    #[test]
    fn get_doc_warns_about_proposals_and_finds_sections() {
        let proposal = get_doc("architecture/history", None).unwrap();
        assert!(proposal.contains("PROPOSAL, NOT BUILT"));
        let stable = get_doc("architecture/models", None).unwrap();
        assert!(!stable.contains("PROPOSAL, NOT BUILT") && stable.contains("# Models"));
        let part = get_doc("architecture/models", Some("fields")).unwrap();
        assert!(part.len() < stable.len());
        let missing = get_doc("architecture/models", Some("no such heading")).unwrap_err();
        assert!(missing.contains("section=\""), "{missing}");
    }

    #[test]
    fn unknown_page_suggests_a_near_one() {
        let e = get_doc("models", None).unwrap_err();
        assert!(e.contains("architecture/models"), "{e}");
        assert!(get_doc("totally/absent/thing", None).unwrap_err().contains("list_docs"));
    }

    #[test]
    fn listings_cover_every_section() {
        let all = list_docs(None).unwrap();
        for s in SECTIONS {
            assert!(all.contains(&format!("## {s}")));
        }
        assert!(all.contains("architecture/history [proposal]"));
        assert!(list_docs(Some("plugin")).unwrap().contains("plugin/pages"));
    }

    #[test]
    fn capabilities_resolve_by_group_or_operation() {
        assert!(list_capabilities().contains("db::"));
        assert!(get_capability("db").unwrap().contains("`db::query`"));
        let one = get_capability("db::query").unwrap();
        assert!(one.contains("`db::query`") && !one.contains("`db::mutate`"));
        assert!(get_capability("db::nope").is_err());
        assert!(get_capability("zzz").unwrap_err().contains("Groups:"));
    }

    #[test]
    fn every_reference_resolves_to_real_pages() {
        for (kind, _, _) in REFERENCES {
            assert!(get_reference(kind).is_ok(), "reference {kind}");
        }
        assert!(get_reference("nope").unwrap_err().contains("manifest"));
    }

    #[test]
    fn schemas_are_served_for_every_format() {
        for kind in ["manifest", "workspace", "model", "rules"] {
            let out = get_schema(kind).unwrap();
            assert!(out.contains("\"$schema\"") && out.contains("\"properties\""), "{kind}");
        }
        assert!(get_schema("MANIFEST").is_ok());
        assert!(get_schema("pages").unwrap_err().contains("manifest"));
    }

    /// Every key the scaffolder writes into plugin.toml must be a property in the generated manifest schema
    /// (at the top level and inside `[plugin]`), so the schema and the templates cannot drift apart.
    #[tokio::test]
    async fn scaffolded_manifests_use_only_keys_the_schema_knows() {
        let schema: serde_json::Value = {
            let out = get_schema("manifest").unwrap();
            serde_json::from_str(out.split("\n\n").last().unwrap()).unwrap()
        };
        let top = schema["properties"].as_object().unwrap();
        let plugin_ref = schema["properties"]["plugin"]["$ref"].as_str().expect("plugin is a $ref");
        let plugin_def = &schema[plugin_ref.trim_start_matches("#/").split('/').next().unwrap()][plugin_ref.rsplit('/').next().unwrap()];
        let plugin_props = plugin_def["properties"].as_object().expect("PluginDefinition properties");
        for lang in ["rhai", "lua", "rust", "go", "typescript", "python"] {
            let out = scaffold_plugin("demo_tool", lang, Some("Demo"), Some("A demo.")).await.unwrap();
            let text = out.split("=== FILE: plugin.toml ===\n").nth(1).unwrap().split("\n=== END FILE ===").next().unwrap();
            let doc: toml_edit::DocumentMut = text.parse().unwrap();
            for (key, item) in doc.iter() {
                assert!(top.contains_key(key), "{lang}: `{key}` is not in the manifest schema");
                if key == "plugin" {
                    for (k, _) in item.as_table().unwrap().iter() {
                        assert!(plugin_props.contains_key(k), "{lang}: `plugin.{k}` is not in the schema");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn scaffold_then_validate_round_trip() {
        for lang in ["rhai", "lua"] {
            let out = scaffold_plugin("demo_tool", lang, Some("Demo tool"), Some("A demo.")).await.unwrap();
            assert!(out.contains("=== FILE: plugin.toml ==="), "{out}");

            // The returned files, written out, must be a valid plugin.
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("demo_tool");
            for part in out.split("\n=== FILE: ").skip(1) {
                let (path, rest) = part.split_once(" ===\n").unwrap();
                let content = rest.split("\n=== END FILE ===").next().unwrap();
                let target = root.join(path);
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::write(target, content).unwrap();
            }
            assert!(!root.join("LICENSE").exists());
            let verdict = validate_plugin(root.to_str().unwrap()).await;
            assert!(verdict.as_ref().is_ok_and(|v| v.starts_with("VALID")), "{lang}: {verdict:?}");
        }
        assert!(scaffold_plugin("x", "cobol", None, None).await.unwrap_err().contains("Unknown language"));
        assert!(scaffold_plugin("Bad Name!", "rhai", None, None).await.is_err());
    }

    #[tokio::test]
    async fn an_unbuilt_compiled_plugin_is_valid_with_a_note() {
        let out = scaffold_plugin("rs_tool", "rust", None, None).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("rs_tool");
        for part in out.split("\n=== FILE: ").skip(1) {
            let (path, rest) = part.split_once(" ===\n").unwrap();
            let target = root.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, rest.split("\n=== END FILE ===").next().unwrap()).unwrap();
        }
        let verdict = validate_plugin(root.to_str().unwrap()).await.unwrap();
        assert!(verdict.starts_with("VALID") && verdict.contains("not built yet"), "{verdict}");
    }

    #[tokio::test]
    async fn validate_reports_what_is_wrong() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_plugin(dir.path().join("nope").to_str().unwrap()).await.unwrap_err().contains("does not exist"));
        std::fs::write(dir.path().join("plugin.toml"), "this is = not [valid").unwrap();
        let e = validate_plugin(dir.path().to_str().unwrap()).await.unwrap_err();
        assert!(e.starts_with("INVALID"), "{e}");
    }

    #[test]
    fn prompts_name_the_tools() {
        let p = build_plugin_prompt("track expenses", None);
        assert!(p.contains("validate_plugin") && p.contains("track expenses") && p.contains("rhai"));
        assert!(write_model_prompt("an invoice").contains("get_reference"));
    }
}
