//! Which bridges plugins can call, and how to build one from its settings. Calling them, which
//! needs the server state, is in `aether_core::bridges`.

use aether_communication::{Action, ActionBridge, ConfigError, SendError, Spec, Values};

/// The bridges plugins can call, with what each offers.
pub fn all() -> [(&'static Spec, &'static [Action]); 6] {
    [
        (
            &aether_paystack_integration::SPEC,
            aether_paystack_integration::ACTIONS,
        ),
        (&open_street_map::SPEC, open_street_map::ACTIONS),
        (&jitsi::SPEC, jitsi::ACTIONS),
        (&collabora::SPEC, collabora::ACTIONS),
        (&jmap::SPEC, jmap::ACTIONS),
        (&imap::SPEC, imap::ACTIONS),
    ]
}

/// Bridges that work on a **person's** behalf with a login of their own (see [`crate::bridge_credentials`]). A plugin calling one is
/// answered as the person who is calling; with no login connected, the call is refused and says so.
pub const PER_USER: &[&str] = &["jmap", "imap"];

/// Bridges only the kernel's own commands call (`ocr::parse`, `ai::complete` and the like). They are not in
/// [`all`], so no plugin can name them in `bridges` or reach them with `bridge::call`.
pub fn internal() -> [(&'static Spec, &'static [Action]); 4] {
    [
        (&liteparse::SPEC, liteparse::ACTIONS),
        (&ollama::SPEC, ollama::ACTIONS),
        (&openai_compatible::SPEC, openai_compatible::ACTIONS),
        (&anthropic::SPEC, anthropic::ACTIONS),
    ]
}

pub fn find(key: &str) -> Option<(&'static Spec, &'static [Action])> {
    all().into_iter().find(|(spec, _)| spec.key == key)
}

/// The names of the bridges plugins can call, for messages.
pub fn names() -> String {
    all()
        .iter()
        .map(|(spec, _)| spec.key)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn config(error: ConfigError) -> SendError {
    SendError::transient(error.to_string())
}

pub fn build(key: &str, values: &Values) -> Result<Box<dyn ActionBridge>, SendError> {
    Ok(match key {
        "paystack" => Box::new(aether_paystack_integration::Paystack::new(values).map_err(config)?),
        "open_street_map" => Box::new(open_street_map::OpenStreetMap::new(values).map_err(config)?),
        "jitsi" => Box::new(jitsi::Jitsi::new(values).map_err(config)?),
        "collabora" => Box::new(collabora::Collabora::new(values).map_err(config)?),
        "jmap" => Box::new(jmap::Jmap::new(values).map_err(config)?),
        "imap" => Box::new(imap::Imap::new(values).map_err(config)?),
        other => {
            return Err(SendError::permanent(format!(
                "`{other}` is not a bridge plugins can call (use one of: {})",
                names()
            )));
        }
    })
}
