use serde::{Deserialize, Serialize};
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginGraphError {
    #[error("surrealdb error: {0}")]
    Surreal(#[from] surrealdb::Error),

    #[error("unknown plugin `{0}`")]
    UnknownPlugin(String),

    #[error("dependency cycle involving `{0}`")]
    Cycle(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct DepsRow {
    pub depends_on: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
struct DependentsRow {
    pub dependents: Option<Vec<String>>,
}

/// Replace graph edges for `plugin_name` from its dependency list.
/// Also updates the denormalized `plugins.dependencies` array.
pub async fn sync_plugin_dependencies(
    db: &Surreal<Client>,
    plugin_name: &str,
    dependencies: &[String],
) -> Result<(), PluginGraphError> {
    db.query(
        r#"
        DELETE plugin_depends_on WHERE in = type::record('plugins', $name);
        UPDATE type::record('plugins', $name) SET dependencies = $deps;
        "#,
    )
    .bind(("name", plugin_name.to_string()))
    .bind(("deps", dependencies.to_vec()))
    .await?
    .check()?;

    for dep in dependencies {
        db.query(
            r#"
            RELATE type::record('plugins', $from)->plugin_depends_on->type::record('plugins', $to)
                SET requirement = 'required';
            "#,
        )
        .bind(("from", plugin_name.to_string()))
        .bind(("to", dep.clone()))
        .await?
        .check()?;
    }

    Ok(())
}

/// Direct dependencies of a plugin (one hop out).
pub async fn get_dependencies(
    db: &Surreal<Client>,
    plugin_name: &str,
) -> Result<Vec<String>, PluginGraphError> {
    let mut response = db
        .query(
            r#"
            SELECT ->plugin_depends_on->plugins.name AS depends_on
            FROM type::record('plugins', $name);
            "#,
        )
        .bind(("name", plugin_name.to_string()))
        .await?
        .check()?;

    let rows: Vec<DepsRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|r| r.depends_on)
        .unwrap_or_default())
}

/// Plugins that directly depend on this one (one hop in).
pub async fn get_dependents(
    db: &Surreal<Client>,
    plugin_name: &str,
) -> Result<Vec<String>, PluginGraphError> {
    let mut response = db
        .query(
            r#"
            SELECT <-plugin_depends_on<-plugins.name AS dependents
            FROM type::record('plugins', $name);
            "#,
        )
        .bind(("name", plugin_name.to_string()))
        .await?
        .check()?;

    let rows: Vec<DependentsRow> = response.take(0)?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|r| r.dependents)
        .unwrap_or_default())
}

/// Topological install order for `targets` including transitive dependencies.
pub async fn resolve_install_order(
    db: &Surreal<Client>,
    targets: &[String],
) -> Result<Vec<String>, PluginGraphError> {
    let mut order = Vec::new();
    let mut visiting = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();

    for target in targets {
        visit(db, target, &mut order, &mut visiting, &mut visited).await?;
    }

    Ok(order)
}

async fn visit(
    db: &Surreal<Client>,
    name: &str,
    order: &mut Vec<String>,
    visiting: &mut std::collections::HashSet<String>,
    visited: &mut std::collections::HashSet<String>,
) -> Result<(), PluginGraphError> {
    if visited.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name.to_string()) {
        return Err(PluginGraphError::Cycle(name.to_string()));
    }

    for dep in get_dependencies(db, name).await? {
        Box::pin(visit(db, &dep, order, visiting, visited)).await?;
    }

    visiting.remove(name);
    visited.insert(name.to_string());
    order.push(name.to_string());
    Ok(())
}
