//! Loading, installing and upgrading plugin versions against a real SurrealDB (see
//! `host_db_audit.rs` for how to run), with the files stored as revisions.

use std::path::{Path, PathBuf};

use aether_core::app_dir::AppDir;
use aether_core::config_manager::models::{CompileCacheConfig, PluginRuntimeConfig};
use aether_core::plugin_manager::catalog::{
    CatalogError, PluginSpec, install_plugins, load_plugin, upgrade_plugins,
};
use aether_core::plugin_manager::revisions::{INDEX_FILE, read_index};
use aether_core::plugin_manager::runtime::PluginRuntime;
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct World {
    db: Surreal<Client>,
    namespace: String,
    app_dir: PathBuf,
    package: PathBuf,
    _directory: tempfile::TempDir,
}

const MANIFEST: &str = "[plugin]\nname = \"notes\"\nlabel = \"Notes\"\nversion = \"VERSION\"\ncapabilities = [\"db::query\"]\nwasm_file = \"out/plugin.wasm\"\n";

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
        let namespace = format!("aether_revisions_{suffix}");
        aether_orm::migrate_core(&db, &namespace, "core").await?;
        let directory = tempfile::tempdir()?;
        let app_dir = directory.path().join("app");
        let package = directory.path().join("notes_pkg");
        let world = Self { db, namespace, app_dir, package, _directory: directory };
        world.write("0.1.0", "wasm-one", "page-one").await?;
        Ok(Some(world))
    }

    /// The package on the developer's disk, as it is after a build.
    async fn write(&self, version: &str, wasm: &str, page_title: &str) -> TestResult {
        tokio::fs::create_dir_all(self.package.join("pages")).await?;
        tokio::fs::create_dir_all(self.package.join("out")).await?;
        tokio::fs::write(self.package.join("plugin.toml"), MANIFEST.replace("VERSION", version)).await?;
        tokio::fs::write(
            self.package.join("pages/notes.xml"),
            format!("<page route=\"/notes\" title=\"{page_title}\"><header title=\"{page_title}\"/></page>"),
        ).await?;
        // A distinct, valid module per `wasm` text: it returns a number made from the text.
        let number: i32 = wasm.bytes().map(i32::from).sum();
        let module = format!("(module (func (export \"hello\") (result i32) i32.const {number}))");
        tokio::fs::write(self.package.join("out/plugin.wasm"), wat::parse_str(&module)?).await?;
        Ok(())
    }

    async fn load(&self) -> Result<aether_core::plugin_manager::catalog::LoadedPlugin, CatalogError> {
        load_plugin(&self.db, &self.namespace, "core", &self.app_dir, &self.package).await
    }

    fn folders(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut names: Vec<String> = std::fs::read_dir(self.app_dir.join("plugins/notes"))?
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        Ok(names)
    }

    fn files_in(&self, folder: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        fn walk(directory: &Path, base: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
            for entry in std::fs::read_dir(directory)?.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, out)?;
                } else {
                    out.push(path.strip_prefix(base).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
                }
            }
            Ok(())
        }
        let base = self.app_dir.join("plugins/notes").join(folder);
        let mut out = Vec::new();
        walk(&base, &base, &mut out)?;
        out.sort();
        Ok(out)
    }

    async fn organization(&self, name: &str) -> TestResult {
        aether_orm::migrate_org(&self.db, &self.namespace, name).await?;
        self.db.use_ns(&self.namespace).await?;
        self.db.use_db("core").await?;
        self.db.query("CREATE org_databases SET db_name = $name;").bind(("name", name.to_string())).await?.check()?;
        Ok(())
    }
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn only_changed_files_are_stored_in_a_new_timestamped_folder() -> TestResult {
    let Some(world) = World::new().await? else { return Ok(()) };

    // The first load stores everything.
    let first = world.load().await?;
    assert!(first.created);
    assert_eq!(first.version, "0.1.0");
    assert_eq!((first.written_files, first.reused_files), (3, 0));
    let first_folder = first.revision.clone().ok_or("no revision")?;
    assert_eq!(world.folders()?, [first_folder.clone()]);
    assert_eq!(world.files_in(&first_folder)?, [INDEX_FILE, "pages/notes.xml", "plugin.toml", "plugin.wasm"]);
    assert_eq!(first.artifact_path.as_deref(), Some(format!("plugins/notes/{first_folder}/plugin.wasm").as_str()));

    // The same content again changes nothing.
    let again = world.load().await?;
    assert!(!again.created);
    assert_eq!(again.revision.as_deref(), Some(first_folder.as_str()));
    assert_eq!(world.folders()?.len(), 1, "no new folder");

    // Rebuild: only the wasm differs, so only the wasm is stored, in a new folder.
    world.write("0.1.0", "wasm-two", "page-one").await?;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await; // a new second, a new stamp
    let rebuilt = world.load().await?;
    assert!(rebuilt.created);
    let second_folder = rebuilt.revision.clone().ok_or("no revision")?;
    assert_ne!(second_folder, first_folder);
    assert_eq!(rebuilt.version, format!("0.1.0+{second_folder}"), "no manual version bump needed");
    assert_eq!((rebuilt.written_files, rebuilt.reused_files), (1, 2));
    assert_eq!(world.files_in(&second_folder)?, [INDEX_FILE, "plugin.wasm"], "nothing is duplicated");
    assert_eq!(world.files_in(&first_folder)?.len(), 4, "the first revision is untouched");

    // The index of the new revision is complete and points at the older folder.
    let index = read_index(&AppDir::new(&world.app_dir), "notes", &second_folder).await?.ok_or("no index")?;
    assert_eq!(index.files["plugin.toml"].revision, first_folder);
    assert_eq!(index.files["pages/notes.xml"].revision, first_folder);
    assert_eq!(index.files["plugin.wasm"].revision, second_folder);
    assert_eq!(index.previous.as_deref(), Some(first_folder.as_str()));

    // A new version number whose page and manifest change: the wasm is the rebuilt one, reused.
    world.write("0.2.0", "wasm-two", "page-two").await?;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let bumped = world.load().await?;
    assert_eq!(bumped.version, "0.2.0", "the first load of a version keeps it as written");
    assert_eq!((bumped.written_files, bumped.reused_files), (2, 1));
    let third_folder = bumped.revision.clone().ok_or("no revision")?;
    assert_eq!(world.files_in(&third_folder)?, [INDEX_FILE, "pages/notes.xml", "plugin.toml"]);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn organizations_pin_a_version_and_upgrade_to_a_newer_one() -> TestResult {
    let Some(world) = World::new().await? else { return Ok(()) };
    world.organization("acme").await?;

    let v1 = world.load().await?;
    world.write("0.1.0", "wasm-two", "page-one").await?;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let v2 = world.load().await?;
    assert_ne!(v1.version, v2.version);

    // Installing by name takes the newest; naming a version takes that one.
    let report = install_plugins(&world.db, &world.namespace, "core", "acme", &["notes@0.1.0".parse::<PluginSpec>()?]).await?;
    assert_eq!(report.installed, [("notes".to_string(), "0.1.0".to_string())]);

    // The runtime can run both: the newer version's plugin.toml lives in the older folder.
    let runtime = PluginRuntime::new(
        world.app_dir.clone(),
        PluginRuntimeConfig {
            compile_cache: CompileCacheConfig { enabled: false, ..Default::default() },
            ..Default::default()
        },
    )?;
    let core = world.db.clone();
    core.use_ns(&world.namespace).use_db("core").await?;
    for version in [&v1.version, &v2.version] {
        let loaded = runtime.ensure_loaded(&core, "notes", version).await?;
        assert_eq!(loaded.manifest.plugin.name, "notes");
        assert_eq!(loaded.manifest.plugin.version, "0.1.0");
    }

    // Upgrade moves the organization's pin; a second upgrade has nothing to do.
    let upgraded = upgrade_plugins(&world.db, &world.namespace, "core", "acme", &["notes".parse::<PluginSpec>()?]).await?;
    assert_eq!(upgraded.upgraded, [("notes".to_string(), "0.1.0".to_string(), v2.version.clone())]);
    let again = upgrade_plugins(&world.db, &world.namespace, "core", "acme", &["notes".parse::<PluginSpec>()?]).await?;
    assert_eq!(again.already_current, [("notes".to_string(), v2.version.clone())]);
    assert!(again.upgraded.is_empty());

    // Rolling back is just naming the older version.
    let back = upgrade_plugins(&world.db, &world.namespace, "core", "acme", &["notes@0.1.0".parse::<PluginSpec>()?]).await?;
    assert_eq!(back.upgraded.len(), 1);

    // A plugin the organization never installed cannot be upgraded.
    world.organization("empty").await?;
    let missing = upgrade_plugins(&world.db, &world.namespace, "core", "empty", &["notes".parse::<PluginSpec>()?]).await;
    assert!(matches!(missing, Err(CatalogError::NotInstalled(_))));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn manifests_are_checked_against_the_capability_catalog() -> TestResult {
    let Some(world) = World::new().await? else { return Ok(()) };
    for (capabilities, public) in [
        ("[\"db::mutat\"]", "[]"),
        ("[\"db::query\", \"db::surql\"]", "[]"),
        ("[\"db::query\"]", "[\"db::mutate\"]"),
    ] {
        let manifest = format!(
            "[plugin]\nname = \"notes\"\nversion = \"0.1.0\"\ncapabilities = {capabilities}\npublic_capabilities = {public}\n"
        );
        tokio::fs::write(world.package.join("plugin.toml"), manifest).await?;
        let result = world.load().await;
        assert!(result.is_err(), "{capabilities} / {public} should be refused");
    }
    Ok(())
}
