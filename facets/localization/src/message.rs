//! Messages that travel as data: what a function returns, and `%key%` in a page.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::catalog::Translator;

/// A translatable message: `{ "key": "note.saved", "params": { "name": "Ada" }, "default": "Saved" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub key: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// Shown when no catalog has the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

impl Message {
    /// A message from a JSON value, when it is one (an object with a text `key`).
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let key = object.get("key")?.as_str()?.to_string();
        if !is_key(&key) {
            return None;
        }
        Some(Self {
            key,
            params: object.get("params").and_then(Value::as_object).cloned().unwrap_or_default(),
            default: object.get("default").and_then(Value::as_str).map(str::to_string),
        })
    }

    /// The text: the catalog's, else `default`, else the key.
    pub fn render(&self, translator: &Translator, plugin: &str, chain: &[String]) -> String {
        translator
            .translate(plugin, &self.key, &self.params, chain)
            .or_else(|| self.default.clone())
            .unwrap_or_else(|| self.key.clone())
    }
}

/// A key is letters, digits and `. _ - :`.
fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 200
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'))
}

/// The key in `%key%`, when the whole text is one.
fn reference(text: &str) -> Option<&str> {
    let key = text.strip_prefix('%')?.strip_suffix('%')?;
    is_key(key).then_some(key)
}

/// Replace every string of `tree` that is exactly `%key%` with its text. Text that only
/// contains a `%` (`100%`) is left alone. A key nobody has stays as the key.
pub fn resolve_tree(tree: &mut Value, plugin: &str, translator: &Translator, chain: &[String]) {
    match tree {
        Value::String(text) => {
            if let Some(key) = reference(text) {
                let key = key.to_string();
                *text = translator
                    .translate(plugin, &key, &Map::new(), chain)
                    .unwrap_or(key);
            }
        }
        Value::Array(items) => {
            for item in items {
                resolve_tree(item, plugin, translator, chain);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                resolve_tree(value, plugin, translator, chain);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{catalog::Catalogs, locale::fallback_chain};
    use serde_json::json;

    fn translator() -> Result<Translator, crate::catalog::CatalogError> {
        let mut translator = Translator::new();
        translator.add_plugin(
            "notes",
            &Catalogs::from_files(
                "en",
                [
                    ("en", r#"{"title": "Title", "saved": "Saved {name}"}"#),
                    ("fr", r#"{"title": "Titre", "saved": "{name} enregistré"}"#),
                ],
            )?,
        );
        Ok(translator)
    }

    #[test]
    fn a_page_tree_is_resolved_in_place() -> Result<(), crate::catalog::CatalogError> {
        let translator = translator()?;
        let mut tree = json!({
            "title": "%title%",
            "children": [{"label": "%title%"}, {"label": "Plain"}, {"label": "100%"}],
            "missing": "%nope%"
        });
        resolve_tree(&mut tree, "notes", &translator, &fallback_chain(&[Some("fr")]));
        assert_eq!(
            tree,
            json!({
                "title": "Titre",
                "children": [{"label": "Titre"}, {"label": "Plain"}, {"label": "100%"}],
                "missing": "nope"
            })
        );
        Ok(())
    }

    #[test]
    fn a_returned_message_is_rendered_with_params() -> Result<(), crate::catalog::CatalogError> {
        let translator = translator()?;
        let value = json!({"key": "saved", "params": {"name": "Ada"}});
        let message = Message::from_value(&value);
        let chain = fallback_chain(&[Some("fr")]);
        assert_eq!(
            message.map(|m| m.render(&translator, "notes", &chain)).as_deref(),
            Some("Ada enregistré")
        );
        Ok(())
    }

    #[test]
    fn default_text_is_used_when_the_key_is_unknown() -> Result<(), crate::catalog::CatalogError> {
        let translator = translator()?;
        let chain = fallback_chain(&[Some("en")]);
        let with = Message::from_value(&json!({"key": "x.y", "default": "Hello"}));
        let without = Message::from_value(&json!({"key": "x.y"}));
        assert_eq!(with.map(|m| m.render(&translator, "notes", &chain)).as_deref(), Some("Hello"));
        assert_eq!(without.map(|m| m.render(&translator, "notes", &chain)).as_deref(), Some("x.y"));
        Ok(())
    }

    #[test]
    fn things_that_are_not_messages_are_not_taken_for_one() {
        assert_eq!(Message::from_value(&json!("text")), None);
        assert_eq!(Message::from_value(&json!({"id": 1})), None);
        assert_eq!(Message::from_value(&json!({"key": "has space"})), None);
    }
}
