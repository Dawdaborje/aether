use std::collections::HashMap;
use std::sync::Arc;

use aether_auth_bridge::AuthBridge;
use auth0::Auth0Bridge;
use authentik::AuthentikBridge;
use keycloak::KeycloakBridge;
use ldap::LdapBridge;
use microsoft_extra::MicrosoftBridge;
use serde::Deserialize;
use surrealdb::{Surreal, engine::remote::ws::Client, types::SurrealValue};

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
    map
}

/// Prefer env-configured Google; else load client_id/secret from auth_providers.
pub async fn resolve_bridge(
    db: &Surreal<Client>,
    registry: &HashMap<&'static str, Arc<dyn AuthBridge>>,
    name: &str,
) -> Option<Arc<dyn AuthBridge>> {
    registry.get(name).cloned()
}
