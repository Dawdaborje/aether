//! A plugin's documentation (`[docs]`) travels through the catalog against a real SurrealDB (see
//! `host_db_audit.rs` for how to run): stored when the plugin is loaded, read back for the documentation site.

use std::path::PathBuf;
use std::sync::Arc;

use aether_core::docs_provider::{Viewer, read_catalog, visible_to};
use aether_core::plugin_manager::catalog::{CatalogError, load_plugin};
use aether_docs::plugin_docs::Visibility;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct World {
    db: Arc<Surreal<Client>>,
    namespace: String,
    app_dir: PathBuf,
    package: PathBuf,
    _directory: tempfile::TempDir,
}

impl World {
    async fn new() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Ok(address) = std::env::var("AETHER_TEST_DB") else {
            eprintln!("AETHER_TEST_DB is not set; skipping");
            return Ok(None);
        };
        let db = Surreal::<Client>::init();
        db.connect::<Ws>(address).await?;
        db.signin(Root { username: "root".into(), password: "root".into() }).await?;
        let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
        let namespace = format!("aether_docs_{suffix}");
        aether_orm::migrate_core(&db, &namespace, "core").await?;
        let directory = tempfile::tempdir()?;
        let world = Self {
            db: Arc::new(db),
            namespace,
            app_dir: directory.path().join("app"),
            package: directory.path().join("notes_pkg"),
            _directory: directory,
        };
        std::fs::create_dir_all(&world.package)?;
        std::fs::write(world.package.join("main.rhai"), "fn ping() { 1 }\n")?;
        Ok(Some(world))
    }

    fn manifest(&self, version: &str, docs_table: &str) -> TestResult {
        let text = format!("[plugin]\nname = \"notes\"\nlabel = \"Notes\"\nversion = \"{version}\"\nscript = \"main.rhai\"\ncapabilities = []\n{docs_table}");
        std::fs::write(self.package.join("plugin.toml"), text)?;
        Ok(())
    }

    fn page(&self, path: &str, text: &str) -> TestResult {
        let target = self.package.join("docs").join(path);
        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(target, text)?;
        Ok(())
    }

    async fn load(&self) -> Result<aether_core::plugin_manager::catalog::LoadedPlugin, CatalogError> {
        load_plugin(&self.db, &self.namespace, "core", &self.app_dir, &self.package).await
    }

    async fn catalog(&self) -> Result<Vec<Arc<aether_docs::plugin_docs::PluginDocs>>, surrealdb::Error> {
        self.db.use_ns(&self.namespace).use_db("core").await?;
        read_catalog(&self.db).await
    }
}

#[tokio::test]
async fn docs_are_stored_with_the_plugin_and_read_back() -> TestResult {
    let Some(w) = World::new().await? else { return Ok(()) };
    w.manifest("1.0.0", "\n[docs]\nvisibility = \"public\"\n")?;
    w.page("README.md", "# Notes\n\nTake notes. See [sharing](guide/sharing.md).\n")?;
    w.page("guide/sharing.md", "# Sharing\n\nUse `notes::share`.\n")?;
    w.load().await?;

    let docs = w.catalog().await?;
    assert_eq!(docs.len(), 1);
    let notes = &docs[0];
    assert_eq!((notes.plugin.as_str(), notes.version.as_str(), notes.label.as_str()), ("notes", "1.0.0", "Notes"));
    assert_eq!(notes.visibility, Visibility::Public);
    let slugs: Vec<&str> = notes.pages.iter().map(|p| p.slug.as_str()).collect();
    assert_eq!(slugs, ["index", "guide/sharing"]);
    assert!(notes.page("index").unwrap().html.contains("href=\"doc:guide/sharing\""));

    // Anonymous visitors may read public docs only.
    assert_eq!(visible_to(&docs, &Viewer::default()).len(), 1);
    Ok(())
}

#[tokio::test]
async fn editing_a_page_without_a_new_version_updates_the_docs() -> TestResult {
    let Some(w) = World::new().await? else { return Ok(()) };
    w.manifest("1.0.0", "\n[docs]\n")?;
    w.page("README.md", "# Notes\n\nFirst text.\n")?;
    w.load().await?;
    assert!(w.catalog().await?[0].page("index").unwrap().markdown.contains("First text"));

    w.page("README.md", "# Notes\n\nSecond text.\n")?;
    w.load().await?;
    let docs = w.catalog().await?;
    assert_eq!(docs.len(), 1, "one plugin, not two");
    assert!(docs[0].page("index").unwrap().markdown.contains("Second text"), "the edited page must be what is served");
    assert_eq!(docs[0].visibility, Visibility::Authenticated, "the default is the safe one");
    Ok(())
}

#[tokio::test]
async fn the_newest_version_decides_even_when_it_drops_its_docs() -> TestResult {
    let Some(w) = World::new().await? else { return Ok(()) };
    w.manifest("1.0.0", "\n[docs]\n")?;
    w.page("README.md", "# Notes v1\n")?;
    w.load().await?;
    assert_eq!(w.catalog().await?[0].version, "1.0.0");

    w.manifest("2.0.0", "\n[docs]\n")?;
    w.page("README.md", "# Notes v2\n")?;
    w.load().await?;
    let docs = w.catalog().await?;
    assert_eq!((docs.len(), docs[0].version.as_str()), (1, "2.0.0"));

    // Version 3 has no [docs] table: the plugin is no longer documented, whatever 1.0.0 and 2.0.0 had.
    w.manifest("3.0.0", "")?;
    w.load().await?;
    assert!(w.catalog().await?.is_empty(), "a newest version without docs hides the older ones");
    Ok(())
}

#[tokio::test]
async fn bad_docs_fail_the_load_and_store_nothing() -> TestResult {
    let Some(w) = World::new().await? else { return Ok(()) };
    w.manifest("1.0.0", "\n[docs]\n")?;
    w.page("README.md", "See [gone](missing.md) and <script>alert(1)</script>.\n")?;
    let error = w.load().await.expect_err("a broken link must fail the load");
    assert!(matches!(error, CatalogError::Docs { .. }), "{error}");
    assert!(error.to_string().contains("missing.md"), "{error}");
    assert!(w.catalog().await?.is_empty());

    // The same plugin without docs problems loads, and its script text never reaches the page as HTML.
    w.page("README.md", "# Fine\n\n<script>alert(1)</script>\n")?;
    w.load().await?;
    let docs = w.catalog().await?;
    let html = &docs[0].page("index").unwrap().html;
    assert!(!html.contains("<script") && html.contains("&lt;script&gt;"), "{html}");
    Ok(())
}
