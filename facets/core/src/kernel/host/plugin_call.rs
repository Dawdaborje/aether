//! `plugins::call`: one plugin calls a function of another.
//!
//! * The target must be listed in the caller's `dependencies` in plugin.toml.
//! * The target runs as the same actor as the original request, under *its own* capabilities
//!   and model grants. Nothing is lent from the caller to the target or back; an anonymous
//!   visitor still reaches only the target's `public_functions`.
//! * Calls nest at most [`MAX_DEPTH`] deep and a function cannot be called while it is already
//!   running further up the chain, so a plugin cannot loop.
//! * The answer is the target's JSON result; a failure carries the target's own message
//!   (`fail`/`Error::msg`) when it wrote one for callers, and a plain reason otherwise.

use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::context::PluginHostContext;
use super::error::HostError;

/// Most plugin calls that may be nested, counting the one made over HTTP as the first.
pub const MAX_DEPTH: usize = 4;

#[derive(Deserialize)]
struct Request {
    plugin: String,
    function: String,
    #[serde(default)]
    input: JsonValue,
}

pub async fn plugins_call(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("plugins::call")?;
    let request: Request =
        serde_json::from_value(payload.clone()).map_err(|error| HostError::InvalidPayload(error.to_string()))?;

    if !ctx.dependencies.iter().any(|name| name == &request.plugin) {
        return Err(HostError::Message(format!(
            "`{}` is not one of this plugin's dependencies; list it under `dependencies` in plugin.toml",
            request.plugin
        )));
    }
    let target = format!("{}.{}", request.plugin, request.function);
    if ctx.call_trail.len() >= MAX_DEPTH {
        return Err(HostError::Message(format!("plugin calls are nested more than {MAX_DEPTH} deep")));
    }
    if ctx.call_trail.contains(&target) {
        return Err(HostError::Message(format!("`{target}` is already running further up this chain of calls")));
    }
    let caller = ctx
        .caller
        .as_ref()
        .ok_or_else(|| HostError::Message("this call cannot call other plugins".into()))?;
    let mut trail = ctx.call_trail.clone();
    trail.push(target);
    let data = caller.call(&request.plugin, &request.function, request.input, trail).await?;
    Ok(serde_json::json!({ "ok": true, "data": data }))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::kernel::host::{dispatch::kernel_command, test_support::dummy_ctx};

    /// Records what it was asked and answers with the trail.
    struct Recorder(Mutex<Vec<(String, String, Vec<String>)>>);

    #[async_trait::async_trait]
    impl crate::kernel::PluginCaller for Recorder {
        async fn call(&self, plugin: &str, function: &str, payload: JsonValue, trail: Vec<String>) -> Result<JsonValue, HostError> {
            if let Ok(mut seen) = self.0.lock() {
                seen.push((plugin.into(), function.into(), trail.clone()));
            }
            Ok(serde_json::json!({ "echo": payload, "trail": trail }))
        }
    }

    fn ctx(dependencies: &[&str], trail: &[&str]) -> (PluginHostContext, Arc<Recorder>) {
        let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
        let ctx = dummy_ctx(&["plugins::call"]).with_plugin_calls(
            dependencies.iter().map(|d| (*d).to_string()).collect(),
            trail.iter().map(|t| (*t).to_string()).collect(),
            recorder.clone(),
        );
        (ctx, recorder)
    }

    #[tokio::test]
    async fn calls_a_declared_dependency_and_extends_the_trail() {
        let (ctx, recorder) = ctx(&["billing"], &["test.run"]);
        let answer = kernel_command(&ctx, "plugins::call", serde_json::json!({ "plugin": "billing", "function": "charge", "input": { "n": 1 } }))
            .await
            .unwrap();
        assert_eq!(answer["data"]["echo"], serde_json::json!({ "n": 1 }));
        assert_eq!(answer["data"]["trail"], serde_json::json!(["test.run", "billing.charge"]));
        assert_eq!(recorder.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuses_undeclared_plugins_loops_and_deep_chains() {
        let (ctx_a, recorder) = ctx(&["billing"], &[]);
        let undeclared = kernel_command(&ctx_a, "plugins::call", serde_json::json!({ "plugin": "other", "function": "f" })).await;
        assert!(matches!(&undeclared, Err(HostError::Message(m)) if m.contains("dependencies")), "{undeclared:?}");

        let (looping, _) = ctx(&["billing"], &["test.run", "billing.charge"]);
        let again = kernel_command(&looping, "plugins::call", serde_json::json!({ "plugin": "billing", "function": "charge" })).await;
        assert!(matches!(&again, Err(HostError::Message(m)) if m.contains("already running")), "{again:?}");

        let (deep, _) = ctx(&["billing"], &["a.1", "b.2", "c.3", "d.4"]);
        let too_deep = kernel_command(&deep, "plugins::call", serde_json::json!({ "plugin": "billing", "function": "charge" })).await;
        assert!(matches!(&too_deep, Err(HostError::Message(m)) if m.contains("nested")), "{too_deep:?}");
        assert!(recorder.0.lock().unwrap().is_empty(), "nothing refused may reach the caller");
    }

    #[tokio::test]
    async fn a_call_without_a_caller_says_so() {
        let mut bare = dummy_ctx(&["plugins::call"]);
        bare.dependencies = vec!["billing".into()];
        let result = kernel_command(&bare, "plugins::call", serde_json::json!({ "plugin": "billing", "function": "f" })).await;
        assert!(matches!(result, Err(HostError::Message(_))));
    }
}
