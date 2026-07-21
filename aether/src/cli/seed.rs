use include_dir::{Dir, include_dir};
use surrealdb::{Surreal, engine::remote::ws::Client};

static SEEDS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../seeds");

/// Apply core seed scripts (idempotent best-effort — re-run may duplicate;
/// prefer running once after `--init`).
pub async fn seed_system(db: &Surreal<Client>, namespace: &str, database: &str) {
    if let Err(err) = db.use_ns(namespace).await {
        log::error!("seed: failed to use ns `{namespace}`: {err}");
        return;
    }
    if let Err(err) = db.use_db(database).await {
        log::error!("seed: failed to use db `{database}`: {err}");
        return;
    }

    let mut files: Vec<_> = SEEDS.files().collect();
    files.sort_by_key(|f| f.path().to_path_buf());

    for file in files {
        let name = file
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        if !name.ends_with(".surql") {
            continue;
        }
        let Some(sql) = file.contents_utf8() else {
            log::error!("seed `{name}` is not valid UTF-8");
            continue;
        };
        log::info!("Seeding {name}…");
        match db.query(sql).await {
            Ok(response) => {
                if let Err(err) = response.check() {
                    log::warn!("seed `{name}` reported: {err}");
                } else {
                    log::info!("Seeded {name}");
                }
            }
            Err(err) => log::error!("seed `{name}` failed: {err}"),
        }
    }
}
