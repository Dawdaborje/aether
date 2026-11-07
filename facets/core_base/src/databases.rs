use std::{collections::HashMap, sync::Arc};

use surrealdb::{Surreal, engine::remote::ws::Client as SurrealClient};
use tokio::sync::RwLock;

/// A long-lived database session already pointed at one database.
///
/// Cloning a `Surreal` handle opens a new session, whose first operation costs about
/// 25 ms. Sessions are therefore made once per database and shared by reference; they
/// are never switched to another database, so concurrent requests cannot interfere.
pub type Db = Arc<Surreal<SurrealClient>>;

/// One session per database, created on first use.
#[derive(Clone)]
struct SessionPool {
    /// Shared by reference: cloning the `Surreal` handle itself opens a session.
    connection: Arc<Surreal<SurrealClient>>,
    namespace: String,
    sessions: Arc<RwLock<HashMap<String, Db>>>,
}

impl SessionPool {
    async fn session(&self, database: &str) -> Result<Db, surrealdb::Error> {
        if let Some(session) = self.sessions.read().await.get(database) {
            return Ok(session.clone());
        }
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get(database) {
            return Ok(session.clone());
        }
        let session = Surreal::clone(&self.connection);
        session.use_ns(&self.namespace).use_db(database).await?;
        // Pay the session's first-operation cost now, not in a request.
        session.query("RETURN 1;").await?.check()?;
        let session = Arc::new(session);
        sessions.insert(database.to_string(), session.clone());
        Ok(session)
    }
}

/// Database access on its own: the sessions and the name of the core database.
///
/// A sub-state of `AppState` (in `aether_core`). Handlers and helpers that only read or write the database
/// take `State<Databases>` or `&Databases` (axum builds it from `AppState` with `FromRef`),
/// so they need no cache, configuration or limiter, and tests can build one without them.
#[derive(Clone)]
pub struct Databases {
    pool: SessionPool,
    core_database: String,
}

impl Databases {
    /// Sessions over one connection, all in `namespace`; `core_database` is the core one.
    pub fn new(
        connection: Surreal<SurrealClient>,
        namespace: impl Into<String>,
        core_database: impl Into<String>,
    ) -> Self {
        Self {
            pool: SessionPool {
                connection: Arc::new(connection),
                namespace: namespace.into(),
                sessions: Arc::default(),
            },
            core_database: core_database.into(),
        }
    }

    /// A new session that may be switched between databases freely. See
    /// `AppState::fresh_session`.
    pub fn fresh_session(&self) -> Surreal<SurrealClient> {
        Surreal::clone(&self.pool.connection)
    }

    /// The core database's session.
    pub async fn core(&self) -> Result<Db, surrealdb::Error> {
        self.pool.session(&self.core_database).await
    }

    /// The SurrealDB namespace every session is in.
    pub fn namespace(&self) -> &str {
        &self.pool.namespace
    }

    /// An organization database's session.
    pub async fn org(&self, org_db: &str) -> Result<Db, surrealdb::Error> {
        self.pool.session(org_db).await
    }
}
