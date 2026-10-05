//! OpenStreetMap geocoding through Nominatim. <https://nominatim.org/release-docs/latest/api/Overview/>
//!
//! `geocode` turns an address into coordinates and `reverse` turns coordinates into an address. The
//! public server at nominatim.openstreetmap.org is free but has a usage policy: at most one request
//! a second, a User-Agent that identifies the application, and no heavy use. This bridge spaces
//! requests at least a second apart and identifies itself (set a contact in the settings); for real
//! volume run your own Nominatim and put its address in `base_url`.

use std::time::Duration;

use aether_communication::{Action, ActionBridge, ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Spec, Values};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::{sync::Mutex, time::Instant};

pub const SPEC: Spec = Spec {
    key: "open_street_map",
    label: "OpenStreetMap",
    account: &[],
    options: &[
        Field { name: "base_url", label: "Nominatim address", secret: false, required: false, default: Some("https://nominatim.openstreetmap.org") },
        Field { name: "contact", label: "Contact (an email or website, required by the usage policy)", secret: false, required: false, default: None },
    ],
};

pub const ACTIONS: &[Action] = &[
    Action { name: "geocode", help: "{ query, limit? (1 to 10, default 5), country_codes? (\"ng,gh\") } -> [{ display_name, lat, lon, kind, importance }]" },
    Action { name: "reverse", help: "{ lat, lon } -> { display_name, lat, lon, address } or null where there is nothing" },
];

/// The least time between two requests to the server.
const SPACING: Duration = Duration::from_secs(1);

pub struct OpenStreetMap {
    base_url: String,
    user_agent: String,
    last_request: Mutex<Option<Instant>>,
    spacing: Duration,
}

impl OpenStreetMap {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let contact = values.get("contact").map(|c| c.trim()).filter(|c| !c.is_empty());
        Ok(Self {
            base_url: values
                .get("base_url")
                .filter(|url| !url.is_empty())
                .map_or("https://nominatim.openstreetmap.org", String::as_str)
                .trim_end_matches('/')
                .to_string(),
            user_agent: match contact {
                Some(contact) => format!("Aether-Kernel/{} ({contact})", env!("CARGO_PKG_VERSION")),
                None => format!("Aether-Kernel/{}", env!("CARGO_PKG_VERSION")),
            },
            last_request: Mutex::new(None),
            spacing: SPACING,
        })
    }

    /// Talk to another server, without waiting between requests (tests).
    #[must_use]
    pub fn with_base_url(mut self, url: &str, spacing: Duration) -> Self {
        self.base_url = url.trim_end_matches('/').to_string();
        self.spacing = spacing;
        self
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, SendError> {
        // One request at a time, a second apart: the public server's policy.
        let mut last = self.last_request.lock().await;
        if let Some(previous) = *last {
            tokio::time::sleep_until(previous + self.spacing).await;
        }
        let result = reqwest::Client::new()
            .get(format!("{}{path}", self.base_url))
            .header("user-agent", &self.user_agent)
            .query(query)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .send()
            .await;
        *last = Some(Instant::now());
        drop(last);
        let response = result.map_err(|e| SendError::transient(format!("the map server could not be reached ({})", if e.is_timeout() { "timed out" } else { "network error" })))?;
        let status = response.status();
        if !status.is_success() {
            return Err(SendError::from_status(status.as_u16(), "the map server refused the request"));
        }
        response.json().await.map_err(|_| SendError::transient("the map server sent something that is not JSON"))
    }
}

fn number(params: &Value, name: &str, low: f64, high: f64) -> Result<f64, SendError> {
    params[name]
        .as_f64()
        .filter(|n| n.is_finite() && (low..=high).contains(n))
        .ok_or_else(|| SendError::permanent(format!("`{name}` is required: a number from {low} to {high}")))
}

/// Nominatim sends coordinates as text.
fn coordinate(value: &Value) -> Value {
    value.as_str().and_then(|text| text.parse::<f64>().ok()).map_or(Value::Null, |n| json!(n))
}

#[async_trait]
impl ActionBridge for OpenStreetMap {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        match action {
            "geocode" => {
                let query = params["query"].as_str().map(str::trim).filter(|q| !q.is_empty() && q.len() <= 300);
                let Some(query) = query else { return Err(SendError::permanent("`query` is required: the address or place to look for (at most 300 characters)")) };
                let limit = params["limit"].as_u64().unwrap_or(5);
                if !(1..=10).contains(&limit) {
                    return Err(SendError::permanent("`limit` is 1 to 10"));
                }
                let mut q = vec![("q", query.to_string()), ("format", "jsonv2".into()), ("limit", limit.to_string())];
                if let Some(codes) = params["country_codes"].as_str().filter(|c| !c.is_empty()) {
                    if !codes.chars().all(|c| c.is_ascii_alphabetic() || c == ',') {
                        return Err(SendError::permanent("`country_codes` is a comma-separated list such as \"ng,gh\""));
                    }
                    q.push(("countrycodes", codes.to_ascii_lowercase()));
                }
                let found = self.get("/search", &q).await?;
                let places: Vec<Value> = found
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| json!({
                                "display_name": item["display_name"],
                                "lat": coordinate(&item["lat"]),
                                "lon": coordinate(&item["lon"]),
                                "kind": item["type"],
                                "importance": item["importance"],
                            }))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Value::Array(places))
            }
            "reverse" => {
                let lat = number(params, "lat", -90.0, 90.0)?;
                let lon = number(params, "lon", -180.0, 180.0)?;
                let found = self.get("/reverse", &[("lat", lat.to_string()), ("lon", lon.to_string()), ("format", "jsonv2".into())]).await?;
                if found.get("error").is_some() {
                    return Ok(Value::Null);
                }
                Ok(json!({
                    "display_name": found["display_name"],
                    "lat": coordinate(&found["lat"]),
                    "lon": coordinate(&found["lon"]),
                    "address": found["address"],
                }))
            }
            other => Err(SendError::permanent(format!(
                "open_street_map has no action `{other}`; it offers: {}",
                ACTIONS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_communication::testing::serve_once;

    fn bridge(port: u16) -> OpenStreetMap {
        let values: Values = [("contact".to_string(), "ops@acme.test".to_string())].into_iter().collect();
        OpenStreetMap::new(&values).unwrap_or_else(|e| panic!("{e}")).with_base_url(&format!("http://127.0.0.1:{port}"), Duration::ZERO)
    }

    #[tokio::test]
    async fn an_address_becomes_coordinates_and_the_server_is_told_who_asks() {
        let reply = r#"[{"display_name":"Lagos, Nigeria","lat":"6.4550575","lon":"3.3841147","type":"city","importance":0.7,"place_id":1}]"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port).call("geocode", &json!({ "query": "Lagos", "limit": 2, "country_codes": "NG,GH" })).await;
        assert_eq!(answer, Ok(json!([{ "display_name": "Lagos, Nigeria", "lat": 6.4550575, "lon": 3.3841147, "kind": "city", "importance": 0.7 }])));
        let request = seen.await.unwrap_or_default();
        assert!(request.starts_with("GET /search?"), "{request}");
        assert!(request.contains("q=Lagos") && request.contains("limit=2") && request.contains("countrycodes=ng%2Cgh"), "{request}");
        assert!(request.to_ascii_lowercase().contains("user-agent: aether-kernel/") && request.contains("ops@acme.test"), "{request}");
    }

    #[tokio::test]
    async fn coordinates_become_an_address_or_nothing() {
        let reply = r#"{"display_name":"Marina, Lagos","lat":"6.45","lon":"3.39","address":{"city":"Lagos","country":"Nigeria"}}"#;
        let (port, seen) = serve_once(200, reply).await;
        let answer = bridge(port).call("reverse", &json!({ "lat": 6.45, "lon": 3.39 })).await;
        assert_eq!(answer, Ok(json!({ "display_name": "Marina, Lagos", "lat": 6.45, "lon": 3.39, "address": { "city": "Lagos", "country": "Nigeria" } })));
        assert!(seen.await.unwrap_or_default().starts_with("GET /reverse?lat=6.45&lon=3.39&format=jsonv2"));
        let (port, _) = serve_once(200, r#"{"error":"Unable to geocode"}"#).await;
        assert_eq!(bridge(port).call("reverse", &json!({ "lat": 0.0, "lon": 0.0 })).await, Ok(Value::Null));
    }

    #[tokio::test]
    async fn mistakes_are_refused_before_anything_is_sent() {
        let b = bridge(1);
        for (action, params) in [
            ("geocode", json!({})),
            ("geocode", json!({ "query": "  " })),
            ("geocode", json!({ "query": "x".repeat(301) })),
            ("geocode", json!({ "query": "Lagos", "limit": 50 })),
            ("geocode", json!({ "query": "Lagos", "country_codes": "n g;" })),
            ("reverse", json!({ "lat": 91, "lon": 0 })),
            ("reverse", json!({ "lat": 0 })),
            ("fly_there", json!({})),
        ] {
            let error = b.call(action, &params).await.err();
            assert!(error.as_ref().is_some_and(|e| e.permanent), "{action} {params}: {error:?}");
        }
    }

    #[tokio::test]
    async fn requests_are_spaced_out() {
        let values = Values::new();
        let mut spaced = OpenStreetMap::new(&values).unwrap_or_else(|e| panic!("{e}"));
        spaced.spacing = Duration::from_millis(300);
        let (port, _) = serve_once(200, "[]").await;
        spaced.base_url = format!("http://127.0.0.1:{port}");
        let started = Instant::now();
        let _ = spaced.call("geocode", &json!({ "query": "a" })).await;
        // The test server answers once; the second request fails to connect, but only after the wait.
        let _ = spaced.call("geocode", &json!({ "query": "b" })).await;
        assert!(started.elapsed() >= Duration::from_millis(300), "the second request waited");
    }
}
