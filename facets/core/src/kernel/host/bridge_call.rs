//! `bridge::call`: a plugin calls an outside service through one of the kernel's bridges.
//!
//! ```json
//! { "bridge": "paystack", "action": "initialize_transaction", "params": { "email": "ann@example.com", "amount": 50000 } }
//! ```
//!
//! Needs the capability `bridge::call`, and the bridge listed under `bridges` in `plugin.toml`. The
//! credentials are the organization's (its own, or the global ones) and never reach the plugin. The
//! call waits for the answer. See [`crate::bridges`] for the bridges there are and their actions.

use std::time::Instant;

use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::error::HostError;

#[derive(Deserialize)]
struct Request {
    bridge: String,
    action: String,
    #[serde(default)]
    params: JsonValue,
}

pub async fn bridge_call(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("bridge::call")?;
    let request: Request = serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))?;
    if !ctx.bridge_names.iter().any(|name| name == &request.bridge) {
        return Err(HostError::Message(format!(
            "this plugin may not call bridge `{}`; list it under `bridges` in plugin.toml",
            request.bridge
        )));
    }
    let params = if request.params.is_null() { serde_json::json!({}) } else { request.params };
    if !params.is_object() {
        return Err(HostError::InvalidPayload("`params` is a JSON object".into()));
    }
    let handle = ctx
        .services()?
        .bridges
        .clone()
        .ok_or_else(|| HostError::Message("bridges are not available here".into()))?;
    let started = Instant::now();
    let answer = handle.call(&ctx.database, &request.bridge, &request.action, params).await;
    // Never the parameters or the answer: they can hold card holders' emails and amounts.
    log::info!(
        "{} bridge::call {}.{} -> {} in {}ms",
        ctx.plugin_name,
        request.bridge,
        request.action,
        if answer.is_ok() { "ok" } else { "failed" },
        started.elapsed().as_millis()
    );
    match answer {
        Ok(data) => Ok(serde_json::json!({ "ok": true, "data": data })),
        Err(error) => Err(HostError::Message(error.message)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::kernel::host::{
        context::BridgeHandle,
        dispatch::kernel_command,
        test_support::{dummy_ctx, in_memory_media, with_services},
    };
    use aether_communication::SendError;
    use serde_json::json;

    /// Records the call and answers with it.
    struct Recorder(Mutex<Vec<(String, String, String, JsonValue)>>);

    #[async_trait::async_trait]
    impl BridgeHandle for Recorder {
        async fn call(&self, org: &str, bridge: &str, action: &str, params: JsonValue) -> Result<JsonValue, SendError> {
            if let Ok(mut seen) = self.0.lock() {
                seen.push((org.into(), bridge.into(), action.into(), params.clone()));
            }
            if action == "fails" {
                return Err(SendError::permanent("the provider refused it"));
            }
            Ok(json!({ "echo": params }))
        }
    }

    fn context(bridges: &[&str], caps: &[&str]) -> (PluginHostContext, Arc<Recorder>) {
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let mut ctx = with_services(dummy_ctx(caps), in_memory_media()).with_bridges(bridges.iter().map(|b| (*b).to_string()).collect());
        ctx.database = "org_acme".into();
        if let Some(services) = ctx.services.as_mut() {
            services.bridges = Some(recorder.clone());
        }
        (ctx, recorder)
    }

    #[tokio::test]
    async fn a_declared_bridge_is_called_for_the_calls_organization() -> Result<(), HostError> {
        let (ctx, recorder) = context(&["paystack"], &["bridge::call"]);
        let answer = kernel_command(&ctx, "bridge::call", json!({ "bridge": "paystack", "action": "verify_transaction", "params": { "reference": "r1" } })).await?;
        assert_eq!(answer["data"]["echo"], json!({ "reference": "r1" }));
        let seen = recorder.0.lock().map(|seen| seen.clone()).unwrap_or_default();
        assert_eq!(seen[0].0, "org_acme");
        assert_eq!((seen[0].1.as_str(), seen[0].2.as_str()), ("paystack", "verify_transaction"));
        // `params` may be left out.
        kernel_command(&ctx, "bridge::call", json!({ "bridge": "paystack", "action": "verify_transaction" })).await?;
        Ok(())
    }

    #[tokio::test]
    async fn an_undeclared_bridge_a_missing_capability_and_a_provider_refusal_are_refused() {
        let (ctx, recorder) = context(&["paystack"], &["bridge::call"]);
        let undeclared = kernel_command(&ctx, "bridge::call", json!({ "bridge": "open_street_map", "action": "geocode", "params": {} })).await;
        assert!(matches!(&undeclared, Err(HostError::Message(m)) if m.contains("bridges")), "{undeclared:?}");
        let not_object = kernel_command(&ctx, "bridge::call", json!({ "bridge": "paystack", "action": "x", "params": [1] })).await;
        assert!(matches!(not_object, Err(HostError::InvalidPayload(_))));
        let refused = kernel_command(&ctx, "bridge::call", json!({ "bridge": "paystack", "action": "fails" })).await;
        assert!(matches!(&refused, Err(HostError::Message(m)) if m == "the provider refused it"), "{refused:?}");
        assert_eq!(recorder.0.lock().map(|seen| seen.len()).unwrap_or(0), 1, "only the allowed bridge was reached");

        let (without, _) = context(&["paystack"], &[]);
        assert!(matches!(
            kernel_command(&without, "bridge::call", json!({ "bridge": "paystack", "action": "a" })).await,
            Err(HostError::Capability(_))
        ));
        let mut bare = dummy_ctx(&["bridge::call"]).with_bridges(vec!["paystack".into()]);
        bare.services = None;
        assert!(matches!(kernel_command(&bare, "bridge::call", json!({ "bridge": "paystack", "action": "a" })).await, Err(HostError::Message(_))));
    }
}
