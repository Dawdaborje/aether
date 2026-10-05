//! The message syntax: the subset of ICU MessageFormat that plugin catalogs use.
//!
//! ```text
//! Hello, {name}
//! {count, plural, =0 {No notes} one {# note} other {# notes}}
//! {role, select, admin {Administrator} other {Member}}
//! ```
//!
//! `{name}` is replaced by the parameter. `plural` picks a branch by the language's plural
//! rules (an exact `=N` branch wins) and `#` inside it is the number. `select` picks the branch
//! named like the parameter's value. Both need an `other` branch. A parameter that was not
//! given is left in the text as `{name}`, so a missing value is visible, not an error.

use serde_json::{Map, Value};
use thiserror::Error;

use crate::locale::language;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FormatError {
    #[error("a `{{` is never closed")]
    Unclosed,
    #[error("a `}}` has no matching `{{`")]
    Unopened,
    #[error("empty argument name")]
    EmptyName,
    #[error("unknown argument type `{0}`; use `number`, `plural` or `select`")]
    UnknownType(String),
    #[error("`{0}` needs an `other` branch")]
    MissingOther(String),
    #[error("malformed branch in `{0}`")]
    BadBranch(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Text(String),
    /// `#` inside a plural branch.
    Number,
    Arg(String),
    Plural { name: String, branches: Vec<(Selector, Vec<Node>)> },
    Select { name: String, branches: Vec<(String, Vec<Node>)> },
}

#[derive(Debug, Clone, PartialEq)]
enum Selector {
    Exact(f64),
    Category(String),
}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { chars: text.chars().peekable() }
    }

    /// Nodes up to the end of the text, or to the `}` closing a branch when `nested`.
    fn nodes(&mut self, nested: bool) -> Result<Vec<Node>, FormatError> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        loop {
            let Some(c) = self.chars.next() else {
                if nested {
                    return Err(FormatError::Unclosed);
                }
                push_text(&mut nodes, &mut text);
                return Ok(nodes);
            };
            match c {
                '}' if nested => {
                    push_text(&mut nodes, &mut text);
                    return Ok(nodes);
                }
                '}' => return Err(FormatError::Unopened),
                '{' => {
                    push_text(&mut nodes, &mut text);
                    nodes.push(self.argument()?);
                }
                '#' if nested => {
                    push_text(&mut nodes, &mut text);
                    nodes.push(Node::Number);
                }
                other => text.push(other),
            }
        }
    }

    fn word(&mut self) -> String {
        self.skip_space();
        let mut word = String::new();
        while let Some(&c) = self.chars.peek() {
            if c == ',' || c == '}' || c == '{' || c.is_whitespace() {
                break;
            }
            word.push(c);
            self.chars.next();
        }
        word
    }

    fn skip_space(&mut self) {
        while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
            self.chars.next();
        }
    }

    /// After a `{`: `name}` or `name, type[, branches]}`.
    fn argument(&mut self) -> Result<Node, FormatError> {
        let name = self.word();
        if name.is_empty() {
            return Err(FormatError::EmptyName);
        }
        self.skip_space();
        match self.chars.next() {
            Some('}') => Ok(Node::Arg(name)),
            Some(',') => {
                let kind = self.word();
                self.skip_space();
                match kind.as_str() {
                    "number" => match self.chars.next() {
                        Some('}') => Ok(Node::Arg(name)),
                        _ => Err(FormatError::Unclosed),
                    },
                    "plural" | "select" => {
                        if self.chars.next() != Some(',') {
                            return Err(FormatError::BadBranch(name));
                        }
                        let branches = self.branches(&name)?;
                        self.build(name, &kind, branches)
                    }
                    other => Err(FormatError::UnknownType(other.to_string())),
                }
            }
            _ => Err(FormatError::Unclosed),
        }
    }

    /// `selector {text}` repeated, up to and including the closing `}` of the argument.
    fn branches(&mut self, name: &str) -> Result<Vec<(String, Vec<Node>)>, FormatError> {
        let mut branches = Vec::new();
        loop {
            self.skip_space();
            match self.chars.peek() {
                Some('}') => {
                    self.chars.next();
                    return Ok(branches);
                }
                None => return Err(FormatError::Unclosed),
                _ => {}
            }
            let selector = self.word();
            self.skip_space();
            if selector.is_empty() || self.chars.next() != Some('{') {
                return Err(FormatError::BadBranch(name.to_string()));
            }
            branches.push((selector, self.nodes(true)?));
        }
    }

    fn build(
        &mut self,
        name: String,
        kind: &str,
        branches: Vec<(String, Vec<Node>)>,
    ) -> Result<Node, FormatError> {
        if !branches.iter().any(|(selector, _)| selector == "other") {
            return Err(FormatError::MissingOther(name));
        }
        if kind == "select" {
            return Ok(Node::Select { name, branches });
        }
        let mut typed = Vec::with_capacity(branches.len());
        for (selector, nodes) in branches {
            let selector = match selector.strip_prefix('=') {
                Some(number) => Selector::Exact(
                    number.parse().map_err(|_| FormatError::BadBranch(name.clone()))?,
                ),
                None => Selector::Category(selector),
            };
            typed.push((selector, nodes));
        }
        Ok(Node::Plural { name, branches: typed })
    }
}

/// Move the pending literal text into `nodes`.
fn push_text(nodes: &mut Vec<Node>, text: &mut String) {
    if !text.is_empty() {
        nodes.push(Node::Text(std::mem::take(text)));
    }
}

/// The plural category of `n` in `locale`: `zero`, `one`, `two`, `few`, `many` or `other`.
/// Covers English-like languages (the default), French and Portuguese, the East Slavic
/// languages, Polish, Arabic, and the languages without plurals (Chinese, Japanese, Korean,
/// Vietnamese, Thai, Indonesian, Malay).
pub fn plural_category(locale: &str, n: f64) -> &'static str {
    let whole = n.fract() == 0.0;
    let i = n.abs().trunc();
    let mod10 = i % 10.0;
    let mod100 = i % 100.0;
    match language(locale) {
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" => "other",
        "fr" | "pt" => if i == 0.0 || i == 1.0 { "one" } else { "other" },
        "ru" | "uk" | "be" => {
            if !whole {
                "other"
            } else if mod10 == 1.0 && mod100 != 11.0 {
                "one"
            } else if (2.0..=4.0).contains(&mod10) && !(12.0..=14.0).contains(&mod100) {
                "few"
            } else {
                "many"
            }
        }
        "pl" => {
            if !whole {
                "other"
            } else if i == 1.0 {
                "one"
            } else if (2.0..=4.0).contains(&mod10) && !(12.0..=14.0).contains(&mod100) {
                "few"
            } else {
                "many"
            }
        }
        "ar" => {
            if !whole {
                "other"
            } else if i == 0.0 {
                "zero"
            } else if i == 1.0 {
                "one"
            } else if i == 2.0 {
                "two"
            } else if (3.0..=10.0).contains(&mod100) {
                "few"
            } else if (11.0..=99.0).contains(&mod100) {
                "many"
            } else {
                "other"
            }
        }
        _ => if n == 1.0 { "one" } else { "other" },
    }
}

fn show_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn show(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.as_f64().map_or_else(|| number.to_string(), show_number),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn number_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn render(nodes: &[Node], params: &Map<String, Value>, locale: &str, number: Option<f64>, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Number => out.push_str(&number.map(show_number).unwrap_or_default()),
            Node::Arg(name) => match params.get(name) {
                Some(value) => out.push_str(&show(value)),
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            },
            Node::Plural { name, branches } => {
                let Some(n) = params.get(name).and_then(number_of) else {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                    continue;
                };
                let category = plural_category(locale, n);
                let chosen = branches
                    .iter()
                    .find(|(selector, _)| matches!(selector, Selector::Exact(x) if *x == n))
                    .or_else(|| {
                        branches
                            .iter()
                            .find(|(selector, _)| matches!(selector, Selector::Category(c) if c == category))
                    })
                    .or_else(|| {
                        branches
                            .iter()
                            .find(|(selector, _)| matches!(selector, Selector::Category(c) if c == "other"))
                    });
                if let Some((_, nodes)) = chosen {
                    render(nodes, params, locale, Some(n), out);
                }
            }
            Node::Select { name, branches } => {
                let value = params.get(name).map(show);
                let chosen = branches
                    .iter()
                    .find(|(selector, _)| Some(selector) == value.as_ref())
                    .or_else(|| branches.iter().find(|(selector, _)| selector == "other"));
                if let Some((_, nodes)) = chosen {
                    render(nodes, params, locale, number, out);
                }
            }
        }
    }
}

/// Fill `template` with `params`, using `locale`'s plural rules.
pub fn format_message(
    template: &str,
    params: &Map<String, Value>,
    locale: &str,
) -> Result<String, FormatError> {
    // Nearly every message is plain text; skip the parser for those.
    if !template.contains(['{', '}']) {
        return Ok(template.to_string());
    }
    let nodes = Parser::new(template).nodes(false)?;
    let mut out = String::with_capacity(template.len());
    render(&nodes, params, locale, None, &mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    fn fmt(template: &str, value: Value, locale: &str) -> Result<String, FormatError> {
        format_message(template, &params(value), locale)
    }

    #[test]
    fn plain_text_and_parameters() -> Result<(), FormatError> {
        assert_eq!(fmt("Hello", json!({}), "en")?, "Hello");
        assert_eq!(fmt("Hello, {name}!", json!({"name": "Ada"}), "en")?, "Hello, Ada!");
        assert_eq!(fmt("{n} items", json!({"n": 3}), "en")?, "3 items");
        assert_eq!(fmt("{n, number}", json!({"n": 2.5}), "en")?, "2.5");
        Ok(())
    }

    #[test]
    fn a_missing_parameter_stays_visible() -> Result<(), FormatError> {
        assert_eq!(fmt("Hi {name}", json!({}), "en")?, "Hi {name}");
        Ok(())
    }

    #[test]
    fn plural_picks_by_language_rules() -> Result<(), FormatError> {
        let t = "{count, plural, =0 {No notes} one {# note} other {# notes}}";
        assert_eq!(fmt(t, json!({"count": 0}), "en")?, "No notes");
        assert_eq!(fmt(t, json!({"count": 1}), "en")?, "1 note");
        assert_eq!(fmt(t, json!({"count": 5}), "en")?, "5 notes");
        let fr = "{count, plural, one {# note} other {# notes}}";
        assert_eq!(fmt(fr, json!({"count": 0}), "fr")?, "0 note", "French counts 0 as one");
        assert_eq!(fmt(fr, json!({"count": 2}), "fr")?, "2 notes");
        Ok(())
    }

    #[test]
    fn slavic_and_arabic_categories() {
        assert_eq!(plural_category("ru", 21.0), "one");
        assert_eq!(plural_category("ru", 3.0), "few");
        assert_eq!(plural_category("ru", 12.0), "many");
        assert_eq!(plural_category("pl", 22.0), "few");
        assert_eq!(plural_category("ar", 0.0), "zero");
        assert_eq!(plural_category("ar", 7.0), "few");
        assert_eq!(plural_category("ja", 1.0), "other");
    }

    #[test]
    fn select_picks_a_branch_or_other() -> Result<(), FormatError> {
        let t = "{role, select, admin {Administrator} other {Member}}";
        assert_eq!(fmt(t, json!({"role": "admin"}), "en")?, "Administrator");
        assert_eq!(fmt(t, json!({"role": "x"}), "en")?, "Member");
        assert_eq!(fmt(t, json!({}), "en")?, "Member");
        Ok(())
    }

    #[test]
    fn branches_can_hold_arguments() -> Result<(), FormatError> {
        let t = "{n, plural, one {{name} has # note} other {{name} has # notes}}";
        assert_eq!(fmt(t, json!({"n": 2, "name": "Ada"}), "en")?, "Ada has 2 notes");
        Ok(())
    }

    #[test]
    fn broken_messages_are_errors() {
        assert_eq!(fmt("{name", json!({}), "en"), Err(FormatError::Unclosed));
        assert_eq!(fmt("name}", json!({}), "en"), Err(FormatError::Unopened));
        assert_eq!(fmt("{}", json!({}), "en"), Err(FormatError::EmptyName));
        assert!(matches!(fmt("{n, plural, one {x}}", json!({}), "en"), Err(FormatError::MissingOther(_))));
        assert!(matches!(fmt("{n, date}", json!({}), "en"), Err(FormatError::UnknownType(_))));
    }
}
