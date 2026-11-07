//! Plugin catalogs and looking a key up in them.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::{
    format::format_message,
    locale::{fallback_chain, normalize},
};

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("`i18n/{locale}.json` is not valid JSON: {source}")]
    Json {
        locale: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("`i18n/{locale}.json` must be one flat object of text; `{key}` is not text")]
    NotText { locale: String, key: String },
    #[error("`{0}` is not a locale name (`en`, `fr`, `pt-BR`)")]
    BadLocale(String),
}

/// One plugin's text: its base locale, and for each locale its keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalogs {
    pub default: String,
    pub locales: BTreeMap<String, BTreeMap<String, String>>,
}

impl Catalogs {
    /// From `(locale, file text)` pairs, the contents of the plugin's `i18n/*.json`.
    pub fn from_files<'a>(
        default: &str,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, CatalogError> {
        let default = normalize(default).ok_or_else(|| CatalogError::BadLocale(default.to_string()))?;
        let mut locales = BTreeMap::new();
        for (name, text) in files {
            let locale = normalize(name).ok_or_else(|| CatalogError::BadLocale(name.to_string()))?;
            let value: Value = serde_json::from_str(text)
                .map_err(|source| CatalogError::Json { locale: locale.clone(), source })?;
            let Value::Object(object) = value else {
                return Err(CatalogError::NotText { locale, key: String::new() });
            };
            let mut messages = BTreeMap::new();
            for (key, value) in object {
                // `$schema` and the like are editor hints, not messages.
                if key.starts_with('$') {
                    continue;
                }
                let Value::String(text) = value else {
                    return Err(CatalogError::NotText { locale, key });
                };
                messages.insert(key, text);
            }
            locales.insert(locale, messages);
        }
        Ok(Self { default, locales })
    }

    pub fn is_empty(&self) -> bool {
        self.locales.values().all(BTreeMap::is_empty)
    }

    /// The part of this catalog that overrides plugin `target`: the keys written
    /// `<target>:<key>`, with the prefix removed. `None` when there are none.
    pub fn overrides_of(&self, target: &str) -> Option<Catalogs> {
        let prefix = format!("{target}:");
        let mut locales = BTreeMap::new();
        for (locale, messages) in &self.locales {
            let picked: BTreeMap<String, String> = messages
                .iter()
                .filter_map(|(key, text)| Some((key.strip_prefix(&prefix)?.to_string(), text.clone())))
                .collect();
            if !picked.is_empty() {
                locales.insert(locale.clone(), picked);
            }
        }
        (!locales.is_empty()).then(|| Catalogs { default: self.default.clone(), locales })
    }

    /// This catalog without the keys that override another plugin (those with a `:`).
    pub fn own(&self) -> Catalogs {
        Catalogs {
            default: self.default.clone(),
            locales: self
                .locales
                .iter()
                .map(|(locale, messages)| {
                    let kept = messages
                        .iter()
                        .filter(|(key, _)| !key.contains(':'))
                        .map(|(key, text)| (key.clone(), text.clone()))
                        .collect();
                    (locale.clone(), kept)
                })
                .collect(),
        }
    }
}

#[derive(Debug, Default)]
struct PluginText {
    own: Catalogs,
    /// Catalogs of plugins that override this one, in install order: the last wins.
    overrides: Vec<Catalogs>,
}

/// The text of every plugin installed in one organization.
#[derive(Debug, Default)]
pub struct Translator {
    plugins: BTreeMap<String, PluginText>,
}

impl Translator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `plugin`'s catalog. Keys of the form `<other>:<key>` are not its own text; they
    /// are overrides, added with [`Translator::add_override`] by whoever checked the permission.
    pub fn add_plugin(&mut self, plugin: &str, catalogs: &Catalogs) {
        self.plugins.entry(plugin.to_string()).or_default().own = catalogs.own();
    }

    /// Let `by`'s catalog replace text of `target`. The caller has checked that `by` holds the
    /// `i18n.override` capability and depends on `target`. Later calls win over earlier ones.
    pub fn add_override(&mut self, target: &str, by: &Catalogs) {
        if let Some(overrides) = by.overrides_of(target) {
            self.plugins.entry(target.to_string()).or_default().overrides.push(overrides);
        }
    }

    /// The text for `key` of `plugin`, with the locale it was found in. `chain` is the person's
    /// locales, best first (see [`fallback_chain`]); the plugin's own default comes last.
    /// In each locale an override is tried before the plugin's own text.
    pub fn lookup<'a>(&'a self, plugin: &str, key: &str, chain: &[String]) -> Option<(&'a str, String)> {
        let text = self.plugins.get(plugin)?;
        let mut tried: Vec<String> = chain.to_vec();
        for locale in fallback_chain(&[Some(text.own.default.as_str())]) {
            if !tried.contains(&locale) {
                tried.push(locale);
            }
        }
        for locale in tried {
            let from_override = text
                .overrides
                .iter()
                .rev()
                .find_map(|catalogs| catalogs.locales.get(&locale)?.get(key));
            let found = from_override.or_else(|| text.own.locales.get(&locale)?.get(key));
            if let Some(message) = found {
                return Some((message.as_str(), locale));
            }
        }
        None
    }

    /// `key` of `plugin` filled with `params`, or `None` when no catalog has the key. A message
    /// that is written wrongly is returned as written.
    pub fn translate(
        &self,
        plugin: &str,
        key: &str,
        params: &Map<String, Value>,
        chain: &[String],
    ) -> Option<String> {
        let (template, locale) = self.lookup(plugin, key, chain)?;
        Some(format_message(template, params, &locale).unwrap_or_else(|error| {
            log::warn!("message `{key}` of plugin `{plugin}` is malformed: {error}");
            template.to_string()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn catalogs(default: &str, files: &[(&str, &str)]) -> Result<Catalogs, CatalogError> {
        Catalogs::from_files(default, files.iter().copied())
    }

    fn chain(locale: &str) -> Vec<String> {
        fallback_chain(&[Some(locale)])
    }

    #[test]
    fn keys_fall_back_one_by_one() -> Result<(), CatalogError> {
        let mut translator = Translator::new();
        translator.add_plugin(
            "notes",
            &catalogs(
                "en",
                &[
                    ("en", r#"{"a": "Title", "b": "Body"}"#),
                    ("fr", r#"{"a": "Titre"}"#),
                ],
            )?,
        );
        let none = Map::new();
        assert_eq!(translator.translate("notes", "a", &none, &chain("fr-CA")).as_deref(), Some("Titre"));
        assert_eq!(
            translator.translate("notes", "b", &none, &chain("fr-CA")).as_deref(),
            Some("Body"),
            "fr has no `b`, so the plugin's default language is used"
        );
        assert_eq!(translator.translate("notes", "zzz", &none, &chain("fr")), None);
        assert_eq!(translator.translate("other", "a", &none, &chain("fr")), None);
        Ok(())
    }

    #[test]
    fn plurals_follow_the_language_the_text_was_found_in() -> Result<(), CatalogError> {
        let mut translator = Translator::new();
        translator.add_plugin(
            "notes",
            &catalogs(
                "en",
                &[
                    ("en", r#"{"n": "{c, plural, one {# note} other {# notes}}"}"#),
                    ("fr", r#"{"n": "{c, plural, one {# note} other {# notes}}"}"#),
                ],
            )?,
        );
        let zero = json!({"c": 0});
        let params = zero.as_object().cloned().unwrap_or_default();
        assert_eq!(translator.translate("notes", "n", &params, &chain("fr")).as_deref(), Some("0 note"));
        assert_eq!(translator.translate("notes", "n", &params, &chain("en")).as_deref(), Some("0 notes"));
        Ok(())
    }

    #[test]
    fn an_override_replaces_text_of_the_plugin_it_names() -> Result<(), CatalogError> {
        let mut translator = Translator::new();
        translator.add_plugin("company", &catalogs("en", &[("en", r#"{"title": "Title"}"#)])?);
        let theirs = catalogs(
            "en",
            &[("en", r#"{"company:title": "Subject", "own": "Mine"}"#)],
        )?;
        translator.add_plugin("custom", &theirs);
        translator.add_override("company", &theirs);
        let none = Map::new();
        assert_eq!(translator.translate("company", "title", &none, &chain("en")).as_deref(), Some("Subject"));
        assert_eq!(translator.translate("custom", "own", &none, &chain("en")).as_deref(), Some("Mine"));
        assert_eq!(
            translator.translate("custom", "company:title", &none, &chain("en")),
            None,
            "an override key is not the overrider's own text"
        );
        Ok(())
    }

    #[test]
    fn the_last_override_wins_and_a_better_locale_beats_an_override() -> Result<(), CatalogError> {
        let mut translator = Translator::new();
        translator.add_plugin(
            "company",
            &catalogs("en", &[("en", r#"{"t": "Title"}"#), ("fr", r#"{"t": "Titre"}"#)])?,
        );
        translator.add_override("company", &catalogs("en", &[("en", r#"{"company:t": "One"}"#)])?);
        translator.add_override("company", &catalogs("en", &[("en", r#"{"company:t": "Two"}"#)])?);
        let none = Map::new();
        assert_eq!(translator.translate("company", "t", &none, &chain("en")).as_deref(), Some("Two"));
        assert_eq!(
            translator.translate("company", "t", &none, &chain("fr")).as_deref(),
            Some("Titre"),
            "the person's language is tried before falling back to an English override"
        );
        Ok(())
    }

    #[test]
    fn bad_catalogs_are_refused() {
        assert!(matches!(
            Catalogs::from_files("en", [("en", "{")]),
            Err(CatalogError::Json { .. })
        ));
        assert!(matches!(
            Catalogs::from_files("en", [("en", r#"{"a": 1}"#)]),
            Err(CatalogError::NotText { .. })
        ));
        assert!(matches!(
            Catalogs::from_files("en", [("english", "{}")]),
            Err(CatalogError::BadLocale(_))
        ));
        assert!(matches!(Catalogs::from_files("??", []), Err(CatalogError::BadLocale(_))));
    }

    #[test]
    fn editor_hints_are_ignored() -> Result<(), CatalogError> {
        let c = catalogs("en", &[("en", r#"{"$schema": "x", "a": "A"}"#)])?;
        assert_eq!(c.locales.get("en").map(BTreeMap::len), Some(1));
        Ok(())
    }
}
