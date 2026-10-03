//! `core_schema_status` against a real SurrealDB (see `host_db_audit.rs` for how to run).

use aether_orm::{SchemaStatus, core_schema_status, migrate_core};
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

const NAMESPACE: &str = "aether_schema_status_test";

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn reports_uninitialized_pending_and_current() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(());
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root { username: "root".into(), password: "root".into() }).await?;
    let database = format!("core_{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos());

    assert_eq!(core_schema_status(&db, NAMESPACE, &database).await?, SchemaStatus::Uninitialized);

    migrate_core(&db, NAMESPACE, &database).await?;
    assert_eq!(core_schema_status(&db, NAMESPACE, &database).await?, SchemaStatus::UpToDate);

    db.query("DELETE schema_migrations WHERE version = '016_page_route_patterns';").await?.check()?;
    assert_eq!(
        core_schema_status(&db, NAMESPACE, &database).await?,
        SchemaStatus::Pending(vec!["016_page_route_patterns".to_string()])
    );

    migrate_core(&db, NAMESPACE, &database).await?;
    assert_eq!(core_schema_status(&db, NAMESPACE, &database).await?, SchemaStatus::UpToDate);
    Ok(())
}
