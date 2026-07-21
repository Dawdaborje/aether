use std::collections::HashMap;
use std::sync::Arc;

use aether_auth_bridge::AuthBridge;
use auth0::Auth0Bridge;
use authentik::AuthentikBridge;
use google_auth::GoogleAuthBridge;
use keycloak::KeycloakBridge;
use ldap::LdapBridge;
use microsoft_extra::MicrosoftBridge;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};
use serde::Deserialize;

#[derive(Debug, Deserialize, SurrealValue)]
struct ProviderRow {
    name: String,
    client_id: Option<String>,
    client_secret_encrypted: Option<String>,
    enabled: bool,
}

pub fn default_registry() -> HashMap<&'static str, Arc<dyn AuthBridge>> {
    let mut map: HashMap<&'static str, Arc<dyn AuthBridge>> = HashMap::new();
    map.insert("keycloak", Arc::new(KeycloakBridge));
    map.insert("auth0", Arc::new(Auth0Bridge));
    map.insert("authentik", Arc::new(AuthentikBridge));
    map.insert("ldap", Arc::new(LdapBridge));
    map.insert("microsoft", Arc::new(MicrosoftBridge));

    if let Some(google) = GoogleAuthBridge::from_env() {
        map.insert("google", Arc::new(google));
    }

    map
}

/// Prefer env-configured Google; else load client_id/secret from auth_providers.
pub async fn resolve_bridge(
    db: &Surreal<Client>,
    registry: &HashMap<&'static str, Arc<dyn AuthBridge>>,
    name: &str,
) -> Option<Arc<dyn AuthBridge>> {
    if name == "google" {
        if let Some(existing) = registry.get("google") {
            return Some(existing.clone());
        }
        // Load from DB (plaintext for now — encryption lands with secrets facet).
        let mut response = db
            .query(
                r#"
                SELECT name, client_id, client_secret_encrypted, enabled
                FROM auth_providers WHERE name = 'google' LIMIT 1;
                "#,
            )
            .await
            .ok()?
            .check()
            .ok()?;
        let rows: Vec<ProviderRow> = response.take(0).ok()?;
        let row = rows.into_iter().next()?;
        if !row.enabled {
            return None;
        }
        let client_id = row.client_id.filter(|s| !s.is_empty())?;
        let client_secret = row
            .client_secret_encrypted
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("AETHER_GOOGLE_CLIENT_SECRET").ok())?;
        return Some(Arc::new(GoogleAuthBridge::new(client_id, client_secret)));
    }

    registry.get(name).cloned()
}
