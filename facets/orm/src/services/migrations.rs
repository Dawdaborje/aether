use include_dir::{Dir, include_dir};
use surrealdb::{Surreal, engine::remote::ws::Client};

static MIGRATIONS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../migrations");

pub async fn migrate_core_migrations(db_conn: &Surreal<Client>) {
    for file in MIGRATIONS.files() {
        println!("{}", file.path().display());

        let content = file.contents_utf8();

        log::info!("{:?}", content);
    }

    db_conn
        .use_ns("main")
        .await
        .expect("Error selecting main namespace in surrealdb");
    db_conn
        .use_db("main")
        .await
        .expect("Error selecting main db name in surrealdb");
}

pub async fn run_migrations(db_conn: &Surreal<Client>) {
    migrate_core_migrations(db_conn).await;
}
