//! The model file format (`models/<name>.json`) and the rules a model must follow.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Where a plugin's model files are, relative to the package.
pub const MODEL_DIR: &str = "models";

const ID_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";
const ID_LENGTH: usize = 10;

pub const MODEL_PREFIX: &str = "mdl";
pub const FIELD_PREFIX: &str = "fld";
pub const OPTION_PREFIX: &str = "opt";
pub const INDEX_PREFIX: &str = "idx";

/// A fresh id such as `fld_k3v9xq2m7a`. Ids only have to be unique inside one model (field
/// and option ids) or one catalog (model ids), so a short random one is plenty.
pub fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; ID_LENGTH];
    rand::rng().fill(&mut bytes);
    let tail: String = bytes
        .iter()
        .map(|byte| char::from(ID_ALPHABET[usize::from(*byte) % ID_ALPHABET.len()]))
        .collect();
    format!("{prefix}_{tail}")
}

fn is_id(prefix: &str, text: &str) -> bool {
    text.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
        .is_some_and(|tail| {
            (6..=32).contains(&tail.len())
                && tail.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// `plugin.model`: a model of another plugin, as a link names it.
pub fn foreign_target(target: &str) -> Option<(&str, &str)> {
    target.split_once('.').filter(|(plugin, model)| is_name(plugin) && is_name(model))
}

/// A name plugin code and pages use: lowercase letters, digits and `_`, starting with a letter.
pub fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && text.len() <= 48
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Short text.
    String,
    /// Long text.
    Text,
    Int,
    Float,
    /// An exact decimal with a fixed number of digits after the point (`scale`): money, pay,
    /// days of leave. Sent and received as text; see [`super::decimal`].
    Decimal,
    Bool,
    /// `YYYY-MM-DD`.
    Date,
    /// An RFC 3339 timestamp.
    Datetime,
    /// One of the field's `options`.
    Select,
    /// The id (`table:key`) of a record of another model of the plugin: its `target`.
    Link,
    /// Several records of the `target` model, kept as graph edges rather than in a column: a
    /// person's skills, a group's members. Changed with `db::relate` / `db::unrelate`, read
    /// with `db::related`.
    Many2many,
    /// The rows of another model of the plugin that point back at this record (`inverse`): the
    /// lines of an invoice. No column. Written as a list of rows with `db::create` and
    /// `db::update`, read with `expand`; see [`super::runtime::ChildField`].
    Child,
    /// Any JSON.
    Json,
}

impl FieldType {
    /// Whether records hold a value for the field. A many2many is edges and a child is rows of
    /// another model.
    pub fn has_column(self) -> bool {
        !matches!(self, Self::Many2many | Self::Child)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Text => "text",
            Self::Int => "int",
            Self::Float => "float",
            Self::Decimal => "decimal",
            Self::Bool => "bool",
            Self::Date => "date",
            Self::Datetime => "datetime",
            Self::Select => "select",
            Self::Link => "link",
            Self::Many2many => "many2many",
            Self::Child => "child",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexKind {
    Plain,
    Unique,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    /// Stable, so an option can be renamed without touching stored data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// What is stored and what code compares against.
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// An index over several fields together (for one field, use the field's own `index`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDef {
    /// Stable, so fields can be renamed without rebuilding the index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Field names, in the order the index sorts by: put the field you always filter on first.
    pub fields: Vec<String>,
    /// No two records may have the same values in all of these fields.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    /// Stable identity of the column. The database stores the value under this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// What code and pages call the field. Free to change.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub kind: FieldType,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// For a `link` to the model's own kind: the link is a parent pointer and the records form a
    /// tree (org units, reporting lines). The kernel keeps a graph edge for every parent link, so
    /// `db::tree` can walk ancestors and descendants of any depth in one query, and refuses a
    /// parent that does not exist or would make a loop.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hierarchy: bool,
    /// For `decimal`: digits after the point (default 2, at most 9). Fixed once records exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<IndexKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    /// For `link`: the model it points at. A bare name (`note`) is a model of the same plugin;
    /// `plugin.model` (`currency.currency`) is a model of another plugin, which this plugin must
    /// list as a dependency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// For a link to another plugin's model: that model's id (`mdl_…`). `aether --sync-models`
    /// fills it in from the catalog, like every other id, so loading a plugin never has to look
    /// anything up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Hidden from plugins and pages; the stored data is kept.
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    /// Record changes to this field in the record's chatter. Needs the model's chatter on.
    #[serde(default, skip_serializing_if = "is_false")]
    pub track: bool,
    /// For `int`, `float` and `decimal`: the smallest value allowed (a number, or text for a decimal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<Value>,
    /// For `int`, `float` and `decimal`: the largest value allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<Value>,
    /// For `string` and `text`: the fewest characters allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    /// For `string` and `text`: a regular expression the whole value must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// For `string`: a naming series. A record created without a value gets the next number of
    /// the series, allocated inside the create's transaction so numbers are never skipped or repeated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<SequenceDef>,
    /// A copy of a field of the record a link points at: `"customer.name"` is the `name` of the
    /// record in this model's `customer` link (a link to a model of the same plugin). Filled by the
    /// kernel on every write of the record and read-only for plugins. It is a copy: a change to the
    /// linked record shows here when this record is next written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related: Option<String>,
    /// A number calculated from the record's own whole-number and decimal fields, such as
    /// `"qty * price"` (see [`super::compute`]). Stored, so it can be filtered and indexed; recalculated
    /// on every write of the record and read-only for plugins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute: Option<String>,
    /// For a `child` field: the link of the target model that points back at this model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
    /// For a `child` field: an `int` field of the target model that the kernel sets to each row's
    /// position (1, 2, 3 …) when rows are written, and the rows are read in that order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

/// When a naming series starts again from 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SequenceReset {
    #[default]
    Never,
    Yearly,
    Monthly,
}

/// A naming series such as `INV-{YYYY}-{#####}`: literal text, `{YYYY}`, `{YY}`, `{MM}` (the date the
/// record is created, UTC) and one `{#####}` for the zero-padded number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceDef {
    pub pattern: String,
    #[serde(default)]
    pub reset: SequenceReset,
}

/// A rule every record of the model must satisfy after it is written, such as "the end date is
/// not before the start date". `require` is a filter (see the queries doc); a write that leaves
/// the record not matching it is refused with `message`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckDef {
    pub require: serde_json::Map<String, Value>,
    pub message: String,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// How a model is shown. Presentation only: changing it never touches stored data.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDef {
    /// Field names of the list's columns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form: Vec<FormSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<SortKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormSection {
    pub section: String,
    /// Columns of field names.
    pub columns: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortKey {
    pub field: String,
    #[serde(default)]
    pub dir: Option<String>,
}

/// Whether anonymous visitors may use a record's chatter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisitorChatter {
    /// Visitors never see it. The default.
    #[default]
    None,
    /// Visitors can read messages, on records they can read.
    Read,
    /// Visitors can read and post messages (never notes).
    ReadWrite,
}

fn yes() -> bool {
    true
}

/// The conversation and history panel of a record. A model has none unless it asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatterDef {
    #[serde(default)]
    pub enabled: bool,
    /// Messages, which notify the record's followers.
    #[serde(default = "yes")]
    pub messages: bool,
    /// Internal notes, which notify nobody (except people mentioned).
    #[serde(default = "yes")]
    pub notes: bool,
    #[serde(default = "yes")]
    pub followers: bool,
    /// Record the changes to fields marked `track`.
    #[serde(default = "yes")]
    pub track_changes: bool,
    #[serde(default)]
    pub visitors: VisitorChatter,
}

impl Default for ChatterDef {
    fn default() -> Self {
        Self {
            enabled: false,
            messages: true,
            notes: true,
            followers: true,
            track_changes: true,
            visitors: VisitorChatter::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDef {
    /// Stable identity of the model; its records live in the table of this name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Name of the field that titles a record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_field: Option<String>,
    pub fields: Vec<FieldDef>,
    /// Indexes over several fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexes: Vec<IndexDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewDef>,
    /// The record's conversation and history. Absent means off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chatter: Option<ChatterDef>,
    /// Rules every record must satisfy after a write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckDef>,
}

#[derive(Debug, Error)]
pub enum ModelFileError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path}: the model is named `{name}`, so the file must be `{name}.json`")]
    WrongFileName { path: PathBuf, name: String },
    #[error("{path}: {problems}", problems = .problems.join("; "))]
    Invalid { path: PathBuf, problems: Vec<String> },
    #[error("{problems}", problems = .0.join("; "))]
    InvalidSet(Vec<String>),
}

impl ModelDef {
    /// The model's chatter settings when it has chatter switched on.
    pub fn chatter_on(&self) -> Option<&ChatterDef> {
        self.chatter.as_ref().filter(|chatter| chatter.enabled)
    }

    /// The fields that are in use (not hidden).
    pub fn live_fields(&self) -> impl Iterator<Item = &FieldDef> {
        self.fields.iter().filter(|field| !field.deprecated)
    }

    /// Everything wrong with this definition on its own. Ids must already be assigned
    /// (see [`sync_ids`]); a missing one is a problem.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let model = &self.name;
        if !is_name(&self.name) {
            problems.push(format!(
                "model name `{model}` must be lowercase letters, digits and `_`, starting with a letter"
            ));
        }
        match self.model_id.as_deref() {
            None => problems.push(format!("model `{model}` has no `model_id`; run `aether --sync-models`")),
            Some(id) if !is_id(MODEL_PREFIX, id) => {
                problems.push(format!("model `{model}`: `model_id` `{id}` is not like `mdl_k3v9xq2m7a`"));
            }
            Some(_) => {}
        }

        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for field in &self.fields {
            let name = &field.name;
            if !is_name(name) || name == "id" {
                problems.push(format!(
                    "{model}.{name}: a field name is lowercase letters, digits and `_`, starting with a letter, and is not `id`"
                ));
            }
            if !names.insert(name.as_str()) {
                problems.push(format!("{model}: two fields are called `{name}`"));
            }
            match field.id.as_deref() {
                None => problems.push(format!("{model}.{name} has no `id`; run `aether --sync-models`")),
                Some(id) if !is_id(FIELD_PREFIX, id) => {
                    problems.push(format!("{model}.{name}: id `{id}` is not like `fld_k3v9xq2m7a`"));
                }
                Some(id) => {
                    if !ids.insert(id) {
                        problems.push(format!("{model}.{name}: id `{id}` is used by another field"));
                    }
                }
            }
            field_problems(model, field, &mut problems);
        }

        for (position, field) in self.fields.iter().enumerate() {
            if let Some(compute) = &field.compute
                && let Ok(expr) = super::compute::Expr::parse(compute)
            {
                for rollup in expr.rollups() {
                    match self.live_fields().find(|f| f.name == rollup.child) {
                        Some(f) if f.kind == FieldType::Child => {}
                        _ => problems.push(format!("{model}.{}: `compute` rolls up `{}`, which is not a child field of this model", field.name, rollup.child)),
                    }
                }
                for used in expr.fields() {
                    match self.fields.iter().position(|f| !f.deprecated && f.name == used) {
                        None => problems.push(format!("{model}.{}: `compute` uses `{used}`, which is not a field", field.name)),
                        Some(at) => {
                            let input = &self.fields[at];
                            if !matches!(input.kind, FieldType::Int | FieldType::Decimal) {
                                problems.push(format!("{model}.{}: `compute` uses `{used}`, which is not a whole-number or decimal field", field.name));
                            } else if input.compute.is_some() && at >= position {
                                problems.push(format!("{model}.{}: `compute` uses the calculated field `{used}`, which must be declared before it", field.name));
                            }
                        }
                    }
                }
            }
            if let Some(related) = &field.related
                && let Some((link, _)) = related.split_once('.')
            {
                match self.live_fields().find(|f| f.name == link) {
                    Some(l) if l.kind == FieldType::Link && l.target.as_deref().is_some_and(|t| foreign_target(t).is_none()) => {}
                    _ => problems.push(format!("{model}.{}: `related` needs `{link}` to be a link to a model of this plugin", field.name)),
                }
            }
        }

        let mut index_ids = BTreeSet::new();
        for (position, index) in self.indexes.iter().enumerate() {
            let label = format!("{model}: index #{}", position + 1);
            match index.id.as_deref() {
                None => problems.push(format!("{label} has no `id`")),
                Some(id) if !is_id(INDEX_PREFIX, id) => problems.push(format!("{label}: id `{id}` is not like `idx_k3v9xq2m7a`")),
                Some(id) => {
                    if !index_ids.insert(id) {
                        problems.push(format!("{label}: id `{id}` is used twice"));
                    }
                }
            }
            if !(2..=8).contains(&index.fields.len()) {
                problems.push(format!("{label} needs 2 to 8 fields (one field takes `index` on the field itself)"));
            }
            let mut seen = BTreeSet::new();
            for name in &index.fields {
                match self.live_fields().find(|field| &field.name == name) {
                    None => problems.push(format!("{label}: `{name}` is not a field")),
                    Some(field) if matches!(field.kind, FieldType::Json | FieldType::Text) => {
                        problems.push(format!("{label}: json and text fields cannot be indexed (`{name}`)"));
                    }
                    Some(_) => {}
                }
                if !seen.insert(name.as_str()) {
                    problems.push(format!("{label}: `{name}` is listed twice"));
                }
            }
        }

        if self.checks.len() > MAX_CHECKS {
            problems.push(format!("{model}: at most {MAX_CHECKS} checks"));
        }
        for (position, check) in self.checks.iter().enumerate() {
            let label = format!("{model}: check #{}", position + 1);
            if check.message.trim().is_empty() || check.message.chars().count() > 200 {
                problems.push(format!("{label}: `message` is 1 to 200 characters"));
            }
            match super::query::Filter::parse(&check.require) {
                Err(error) => problems.push(format!("{label}: {error}")),
                Ok(filter) => {
                    if let Err(error) = super::query::compile_filter(&filter, &FieldNames(&self.fields), "c") {
                        problems.push(format!("{label}: {error}"));
                    }
                }
            }
        }
        if let Some(chatter) = &self.chatter {
            if chatter.visitors != VisitorChatter::None && !chatter.enabled {
                problems.push(format!("{model}: `chatter.visitors` needs `chatter.enabled`"));
            }
            if chatter.visitors == VisitorChatter::ReadWrite && !chatter.messages {
                problems.push(format!("{model}: visitors post messages, so `visitors: read_write` needs `messages`"));
            }
        }
        if let Some(tracked) = self.fields.iter().find(|field| field.track && field.kind == FieldType::Json) {
            problems.push(format!("{model}.{}: json fields cannot be tracked", tracked.name));
        }
        if self.fields.iter().any(|field| field.track) && !self.chatter_on().is_some_and(|c| c.track_changes) {
            problems.push(format!("{model}: a field is marked `track` but the model's chatter (with `track_changes`) is not on"));
        }
        if let Some(title) = &self.title_field
            && !self.live_fields().any(|field| &field.name == title)
        {
            problems.push(format!("{model}: `title_field` `{title}` is not a field"));
        }
        if let Some(view) = &self.view {
            let known = |name: &str| self.live_fields().any(|field| field.name == name);
            let referenced = view
                .list
                .iter()
                .chain(view.form.iter().flat_map(|s| s.columns.iter().flatten()))
                .chain(view.sort.iter().map(|key| &key.field));
            for name in referenced {
                if !known(name) {
                    problems.push(format!("{model}: the view names `{name}`, which is not a field"));
                }
            }
        }
        problems
    }
}

/// Most checks one model may have.
pub const MAX_CHECKS: usize = 20;

/// Field names of a model, to validate a filter before any schema exists.
struct FieldNames<'a>(&'a [FieldDef]);

impl super::query::Columns for FieldNames<'_> {
    fn column(&self, field: &str) -> Result<String, super::query::QueryError> {
        if self.0.iter().any(|f| !f.deprecated && f.name == field && f.kind.has_column()) {
            Ok(field.to_string())
        } else {
            Err(super::query::QueryError::Invalid(format!("`{field}` is not a field of this model")))
        }
    }

    fn value(&self, _field: &str, value: &Value) -> Result<Value, super::query::QueryError> {
        Ok(value.clone())
    }
}

fn field_problems(model: &str, field: &FieldDef, problems: &mut Vec<String>) {
    let name = &field.name;
    let numeric = matches!(field.kind, FieldType::Int | FieldType::Float | FieldType::Decimal);
    let texty = matches!(field.kind, FieldType::String | FieldType::Text);
    if (field.min.is_some() || field.max.is_some()) && !numeric {
        problems.push(format!("{model}.{name}: `min` and `max` are only for int, float and decimal fields"));
    }
    if numeric {
        let scale = field.scale.unwrap_or(super::decimal::DEFAULT_SCALE);
        let mut bounds = Vec::new();
        for (label, bound) in [("min", &field.min), ("max", &field.max)] {
            let Some(bound) = bound else { continue };
            match number_of(field.kind, bound, scale) {
                Ok(number) => bounds.push(number),
                Err(reason) => problems.push(format!("{model}.{name}: `{label}` {reason}")),
            }
        }
        if let [low, high] = bounds[..]
            && low > high
        {
            problems.push(format!("{model}.{name}: `min` is above `max`"));
        }
    }
    if (field.min_length.is_some() || field.pattern.is_some()) && !texty {
        problems.push(format!("{model}.{name}: `min_length` and `pattern` are only for string and text fields"));
    }
    if let (Some(min), Some(max)) = (field.min_length, field.max_length)
        && min > max
    {
        problems.push(format!("{model}.{name}: `min_length` is above `max_length`"));
    }
    if let Some(pattern) = &field.pattern
        && let Err(reason) = super::runtime::compile_pattern(pattern)
    {
        problems.push(format!("{model}.{name}: `pattern` {reason}"));
    }
    if field.related.is_some() || field.compute.is_some() {
        if field.related.is_some() && field.compute.is_some() {
            problems.push(format!("{model}.{name}: a field is `related` or `compute`, not both"));
        }
        if field.required || field.default.is_some() || field.sequence.is_some() {
            problems.push(format!("{model}.{name}: a calculated field cannot be `required`, have a `default` or a `sequence`"));
        }
    }
    if let Some(compute) = &field.compute {
        if !matches!(field.kind, FieldType::Int | FieldType::Decimal) {
            problems.push(format!("{model}.{name}: only int and decimal fields can be calculated"));
        }
        if let Err(reason) = super::compute::Expr::parse(compute) {
            problems.push(format!("{model}.{name}: `compute` {reason}"));
        }
    }
    if let Some(related) = &field.related {
        if matches!(field.kind, FieldType::Link | FieldType::Many2many | FieldType::Child | FieldType::Json | FieldType::Select) {
            problems.push(format!("{model}.{name}: a `related` field cannot be a link, select, many2many or json field"));
        }
        if related.split_once('.').is_none_or(|(link, target)| !is_name(link) || !is_name(target)) {
            problems.push(format!("{model}.{name}: `related` is `link_field.field`, such as `customer.name`"));
        }
    }
    if let Some(sequence) = &field.sequence {
        if field.kind != FieldType::String {
            problems.push(format!("{model}.{name}: only string fields can have a `sequence`"));
        }
        if let Err(reason) = super::sequence::Pattern::parse(&sequence.pattern) {
            problems.push(format!("{model}.{name}: sequence `pattern` {reason}"));
        }
        if field.default.is_some() {
            problems.push(format!("{model}.{name}: a sequence field has no `default`"));
        }
    }
    match field.kind {
        FieldType::Select => {
            if field.options.is_empty() {
                problems.push(format!("{model}.{name}: a select field needs `options`"));
            }
            let mut values = BTreeSet::new();
            let mut ids = BTreeSet::new();
            for option in &field.options {
                if option.value.trim().is_empty() || !values.insert(option.value.as_str()) {
                    problems.push(format!("{model}.{name}: option values must be unique and not empty"));
                }
                match option.id.as_deref() {
                    None => problems.push(format!("{model}.{name}: option `{}` has no `id`", option.value)),
                    Some(id) if !is_id(OPTION_PREFIX, id) => {
                        problems.push(format!("{model}.{name}: option id `{id}` is not like `opt_k3v9xq2m7a`"));
                    }
                    Some(id) => {
                        if !ids.insert(id) {
                            problems.push(format!("{model}.{name}: option id `{id}` is used twice"));
                        }
                    }
                }
            }
        }
        _ if !field.options.is_empty() => {
            problems.push(format!("{model}.{name}: only select fields have `options`"));
        }
        _ => {}
    }
    if field.hierarchy {
        if field.kind != FieldType::Link {
            problems.push(format!("{model}.{name}: only a link can be a `hierarchy`"));
        } else if field.target.as_deref() != Some(model) {
            problems.push(format!("{model}.{name}: a `hierarchy` link must point at its own model (`{model}`)"));
        }
        if field.required {
            problems.push(format!("{model}.{name}: a `hierarchy` link cannot be required (the root has no parent)"));
        }
    }
    if field.kind == FieldType::Child {
        if field.required || field.default.is_some() || field.index.is_some() || field.track {
            problems.push(format!("{model}.{name}: a child field has no column, so it cannot be required, indexed, tracked or have a default"));
        }
        match field.target.as_deref() {
            None => problems.push(format!("{model}.{name}: a child field needs a `target` model")),
            Some(target) if foreign_target(target).is_some() => {
                problems.push(format!("{model}.{name}: the rows of a child field are a model of this plugin, not `{target}`"));
            }
            Some(_) => {}
        }
        match field.inverse.as_deref() {
            Some(inverse) if is_name(inverse) => {}
            _ => problems.push(format!("{model}.{name}: a child field needs `inverse`, the link of the rows that points back")),
        }
        if field.order.as_deref().is_some_and(|order| !is_name(order)) {
            problems.push(format!("{model}.{name}: `order` is the name of an int field of the rows"));
        }
    } else if field.inverse.is_some() || field.order.is_some() {
        problems.push(format!("{model}.{name}: only child fields have `inverse` and `order`"));
    }
    if field.kind == FieldType::Many2many {
        if field.required || field.default.is_some() || field.index.is_some() || field.track {
            problems.push(format!("{model}.{name}: a many2many field has no column, so it cannot be required, indexed, tracked or have a default"));
        }
        if field.target.is_none() {
            problems.push(format!("{model}.{name}: a many2many field needs a `target` model"));
        }
    }
    match (field.kind, &field.target) {
        (FieldType::Link | FieldType::Many2many | FieldType::Child, None) if field.kind != FieldType::Link => {}
        (FieldType::Link, None) => problems.push(format!("{model}.{name}: a link field needs a `target` model")),
        (FieldType::Child, Some(target)) if !is_name(target) && foreign_target(target).is_none() => {
            problems.push(format!("{model}.{name}: `target` must be a model name"));
        }
        (FieldType::Child, Some(_)) => {}
        (FieldType::Link | FieldType::Many2many, Some(target)) if foreign_target(target).is_some() => match &field.target_id {
            None => problems.push(format!(
                "{model}.{name}: a link to `{target}` needs its `target_id`; run `aether --sync-models` (the other plugin must be loaded first)"
            )),
            Some(id) if !is_id(MODEL_PREFIX, id) => {
                problems.push(format!("{model}.{name}: `target_id` `{id}` is not like `mdl_k3v9xq2m7a`"));
            }
            Some(_) => {}
        },
        (FieldType::Link | FieldType::Many2many, Some(target)) if !is_name(target) => {
            problems.push(format!("{model}.{name}: `target` must be a model name, or `plugin.model` for another plugin's"));
        }
        (FieldType::Link | FieldType::Many2many, Some(_)) => {
            if field.target_id.is_some() {
                problems.push(format!("{model}.{name}: only a link to another plugin's model has a `target_id`"));
            }
        }
        (_, Some(_)) => problems.push(format!("{model}.{name}: only link fields have a `target`")),
        _ => {}
    }
    if field.max_length.is_some() && !matches!(field.kind, FieldType::String | FieldType::Text) {
        problems.push(format!("{model}.{name}: `max_length` is only for string and text fields"));
    }
    if field.scale.is_some() && field.kind != FieldType::Decimal {
        problems.push(format!("{model}.{name}: `scale` is only for decimal fields"));
    }
    if field.scale.is_some_and(|scale| scale > super::decimal::MAX_SCALE) {
        problems.push(format!("{model}.{name}: `scale` is at most {}", super::decimal::MAX_SCALE));
    }
    if field.max_length == Some(0) {
        problems.push(format!("{model}.{name}: `max_length` must be at least 1"));
    }
    if field.index.is_some() && matches!(field.kind, FieldType::Json | FieldType::Text) {
        problems.push(format!("{model}.{name}: json and text fields cannot be indexed"));
    }
    if field.deprecated && field.required {
        problems.push(format!("{model}.{name}: a hidden (deprecated) field cannot be required"));
    }
    if let Some(default) = &field.default
        && let Err(reason) = super::runtime::check_value(field, default, None)
    {
        problems.push(format!("{model}.{name}: the default is not valid: {reason}"));
    }
}

/// Give every model, field and select option that has no id one. Returns how many were assigned.
pub fn sync_ids(model: &mut ModelDef) -> usize {
    let mut assigned = 0;
    if model.model_id.is_none() {
        model.model_id = Some(new_id(MODEL_PREFIX));
        assigned += 1;
    }
    for field in &mut model.fields {
        if field.id.is_none() {
            field.id = Some(new_id(FIELD_PREFIX));
            assigned += 1;
        }
        for option in &mut field.options {
            if option.id.is_none() {
                option.id = Some(new_id(OPTION_PREFIX));
                assigned += 1;
            }
        }
    }
    for index in &mut model.indexes {
        if index.id.is_none() {
            index.id = Some(new_id(INDEX_PREFIX));
            assigned += 1;
        }
    }
    assigned
}

/// What is wrong across a plugin's models together: names and ids used twice, links to a
/// model that is not there.
pub fn validate_set(models: &[ModelDef]) -> Result<(), ModelFileError> {
    let mut problems = Vec::new();
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for model in models {
        if !names.insert(model.name.as_str()) {
            problems.push(format!("two models are called `{}`", model.name));
        }
        if let Some(id) = &model.model_id
            && !ids.insert(id.as_str())
        {
            problems.push(format!("model id `{id}` is used by two models"));
        }
    }
    for model in models {
        for field in model.live_fields() {
            if let Some(target) = &field.target
                && foreign_target(target).is_none()
                && !names.contains(target.as_str())
            {
                problems.push(format!("{}.{}: links to `{target}`, which is not a model of this plugin", model.name, field.name));
            }
            if field.kind == FieldType::Child
                && let Some(target) = field.target.as_deref()
                && let Some(rows) = models.iter().find(|m| m.name == target)
            {
                let label = format!("{}.{}", model.name, field.name);
                match field.inverse.as_deref().and_then(|name| rows.live_fields().find(|f| f.name == name)) {
                    Some(back) if back.kind == FieldType::Link && !back.hierarchy && back.target.as_deref() == Some(model.name.as_str()) => {}
                    _ => problems.push(format!(
                        "{label}: `inverse` must be a link of `{target}` that points at `{}`",
                        model.name
                    )),
                }
                if let Some(order) = field.order.as_deref() {
                    match rows.live_fields().find(|f| f.name == order) {
                        Some(f) if f.kind == FieldType::Int && f.related.is_none() && f.compute.is_none() => {}
                        _ => problems.push(format!("{label}: `order` must be a plain int field of `{target}`")),
                    }
                }
            }
            if let Some(compute) = field.compute.as_deref().and_then(|c| super::compute::Expr::parse(c).ok()) {
                for rollup in compute.rollups() {
                    let rows = model
                        .live_fields()
                        .find(|f| f.name == rollup.child)
                        .and_then(|f| f.target.as_deref())
                        .and_then(|target| models.iter().find(|m| m.name == target));
                    if let (Some(rows), Some(source)) = (rows, rollup.source.as_deref()) {
                        match rows.live_fields().find(|f| f.name == source) {
                            Some(f) if matches!(f.kind, FieldType::Int | FieldType::Decimal) => {}
                            _ => problems.push(format!(
                                "{}.{}: `compute` adds up `{}.{source}`, which is not a whole-number or decimal field",
                                model.name, field.name, rollup.child
                            )),
                        }
                    }
                }
            }
            if let Some((link, source)) = field.related.as_deref().and_then(|r| r.split_once('.'))
                && let Some(target) = model.live_fields().find(|f| f.name == link).and_then(|f| f.target.as_deref())
                && let Some(other) = models.iter().find(|m| m.name == target)
            {
                match other.live_fields().find(|f| f.name == source) {
                    None => problems.push(format!("{}.{}: `related` names `{source}`, which `{target}` does not have", model.name, field.name)),
                    Some(src) if src.kind != field.kind || src.scale != field.scale => problems.push(format!(
                        "{}.{}: `related` copies `{target}.{source}`, so it must be the same type (and decimal scale)",
                        model.name, field.name
                    )),
                    Some(_) => {}
                }
            }
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(ModelFileError::InvalidSet(problems)) }
}

/// Read and check every `models/*.json` of a package, in name order. Missing ids are an
/// error here: they are assigned by `aether --sync-models`, never silently.
pub fn read_models(package_dir: &Path) -> Result<Vec<(PathBuf, ModelDef)>, ModelFileError> {
    let directory = package_dir.join(MODEL_DIR);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ModelFileError::Io { path: directory, source }),
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut models = Vec::with_capacity(paths.len());
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|source| ModelFileError::Io { path: path.clone(), source })?;
        let model = parse_model(&text, &path)?;
        models.push((path, model));
    }
    validate_set(&models.iter().map(|(_, model)| model.clone()).collect::<Vec<_>>())?;
    Ok(models)
}

/// What [`sync_package`] did to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    pub path: PathBuf,
    pub model: String,
    /// Ids written into the file (0: it already had them all).
    pub assigned: usize,
}

/// Give every model, field and select option in a package's `models/*.json` that has no id one,
/// and write the file back. Ids are never changed once set. A hand-written model starts without
/// ids; this is how it gets them, so that renaming a field later cannot be mistaken for
/// deleting it and adding another.
pub fn sync_package(package_dir: &Path) -> Result<Vec<Synced>, ModelFileError> {
    sync_package_with(package_dir, &HashMap::new())
}

/// The other plugins' models (`plugin.model`) the package's links point at.
pub fn foreign_targets(package_dir: &Path) -> Result<BTreeSet<String>, ModelFileError> {
    let directory = package_dir.join(MODEL_DIR);
    let mut found = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(&directory) else { return Ok(found) };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|source| ModelFileError::Io { path: path.clone(), source })?;
        let model: ModelDef = serde_json::from_str(&text).map_err(|source| ModelFileError::Parse { path: path.clone(), source })?;
        for field in &model.fields {
            if let Some(target) = field.target.as_deref().filter(|target| foreign_target(target).is_some()) {
                found.insert(target.to_string());
            }
        }
    }
    Ok(found)
}

/// [`sync_package`], also filling in the `target_id` of links to other plugins' models from `foreign`
/// (`plugin.model` to model id, from the catalog). A link whose model is not in `foreign` is left as
/// it is and reported by validation.
pub fn sync_package_with(package_dir: &Path, foreign: &HashMap<String, String>) -> Result<Vec<Synced>, ModelFileError> {
    let directory = package_dir.join(MODEL_DIR);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map_err(|source| ModelFileError::Io { path: directory.clone(), source })?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut synced = Vec::new();
    let mut models = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .map_err(|source| ModelFileError::Io { path: path.clone(), source })?;
        let mut model: ModelDef = serde_json::from_str(&text)
            .map_err(|source| ModelFileError::Parse { path: path.clone(), source })?;
        let mut assigned = sync_ids(&mut model);
        for field in &mut model.fields {
            if field.target_id.is_none()
                && let Some(id) = field.target.as_ref().and_then(|target| foreign.get(target))
            {
                field.target_id = Some(id.clone());
                assigned += 1;
            }
        }
        if assigned > 0 {
            let mut written = serde_json::to_string_pretty(&model)
                .map_err(|source| ModelFileError::Parse { path: path.clone(), source })?;
            written.push('\n');
            std::fs::write(&path, written).map_err(|source| ModelFileError::Io { path: path.clone(), source })?;
        }
        synced.push(Synced { path: path.clone(), model: model.name.clone(), assigned });
        models.push((path, model));
    }
    // Whatever is still wrong (a bad name, a link to nothing) is reported now, not at load time.
    for (path, model) in &models {
        let problems = model.problems();
        if !problems.is_empty() {
            return Err(ModelFileError::Invalid { path: path.clone(), problems });
        }
    }
    validate_set(&models.iter().map(|(_, model)| model.clone()).collect::<Vec<_>>())?;
    Ok(synced)
}

/// Parse one model file's text and check it (including that its ids are assigned).
pub fn parse_model(text: &str, path: &Path) -> Result<ModelDef, ModelFileError> {
    let model: ModelDef = serde_json::from_str(text)
        .map_err(|source| ModelFileError::Parse { path: path.to_path_buf(), source })?;
    if path.file_stem().and_then(|s| s.to_str()) != Some(model.name.as_str()) {
        return Err(ModelFileError::WrongFileName { path: path.to_path_buf(), name: model.name });
    }
    let problems = model.problems();
    if problems.is_empty() {
        Ok(model)
    } else {
        Err(ModelFileError::Invalid { path: path.to_path_buf(), problems })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn note() -> ModelDef {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "note",
            "label": "Note",
            "title_field": "title",
            "fields": [
                { "name": "title", "type": "string", "required": true, "max_length": 200, "index": "plain" },
                { "name": "body", "type": "text" },
                { "name": "status", "type": "select", "default": "draft",
                  "options": [{ "value": "draft" }, { "value": "shared" }] },
                { "name": "parent", "type": "link", "target": "note" }
            ],
            "view": { "list": ["title", "status"], "sort": [{ "field": "title" }] }
        }))
        .unwrap();
        sync_ids(&mut model);
        model
    }

    #[test]
    fn ids_look_right_and_are_assigned_once() {
        let mut model = note();
        assert!(is_id(MODEL_PREFIX, model.model_id.as_deref().unwrap()));
        assert!(model.fields.iter().all(|f| is_id(FIELD_PREFIX, f.id.as_deref().unwrap())));
        assert!(model.fields[2].options.iter().all(|o| is_id(OPTION_PREFIX, o.id.as_deref().unwrap())));
        assert_eq!(sync_ids(&mut model), 0, "nothing left to assign");
        assert!(model.problems().is_empty(), "{:?}", model.problems());
    }

    #[test]
    fn a_model_without_ids_is_refused_until_they_are_assigned() {
        let mut model = note();
        model.fields[0].id = None;
        model.model_id = None;
        let problems = model.problems().join("; ");
        assert!(problems.contains("no `model_id`") && problems.contains("note.title has no `id`"), "{problems}");
        sync_ids(&mut model);
        assert!(model.problems().is_empty());
    }

    #[test]
    fn rules_are_enforced() {
        let mut model = note();
        model.fields[0].name = "Title".into();
        model.fields[1].name = "id".into();
        model.fields[2].options.clear();
        model.fields[3].target = None;
        let problems = model.problems().join("; ");
        for expected in ["note.Title", "not `id`", "needs `options`", "needs a `target`"] {
            assert!(problems.contains(expected), "{expected} in {problems}");
        }

        let mut duplicate = note();
        duplicate.fields[1].name = "title".into();
        let second_id = duplicate.fields[0].id.clone();
        duplicate.fields[1].id = second_id;
        let problems = duplicate.problems().join("; ");
        assert!(problems.contains("two fields are called `title`") && problems.contains("used by another field"), "{problems}");

        let mut bad_default = note();
        bad_default.fields[2].default = Some(json!("archived"));
        assert!(bad_default.problems().join(";").contains("default is not valid"));
        let mut bad_view = note();
        bad_view.view.as_mut().unwrap().list.push("nope".into());
        assert!(bad_view.problems().join(";").contains("`nope`"));
    }

    #[test]
    fn a_set_checks_names_ids_and_links() {
        let one = note();
        let mut two = note();
        two.name = "other".into();
        two.model_id = one.model_id.clone();
        assert!(validate_set(&[one.clone(), two.clone()]).is_err(), "the same model_id twice");
        two.model_id = Some(new_id(MODEL_PREFIX));
        two.fields[3].target = Some("ghost".into());
        let error = validate_set(&[one, two]).unwrap_err().to_string();
        assert!(error.contains("`ghost`"), "{error}");
    }

    #[test]
    fn files_are_read_checked_and_named_after_their_model() {
        let directory = tempfile::tempdir().unwrap();
        let models = directory.path().join(MODEL_DIR);
        std::fs::create_dir_all(&models).unwrap();
        let model = note();
        std::fs::write(models.join("note.json"), serde_json::to_string_pretty(&model).unwrap()).unwrap();
        let read = read_models(directory.path()).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].1, model);

        std::fs::write(models.join("wrong.json"), serde_json::to_string(&model).unwrap()).unwrap();
        assert!(matches!(read_models(directory.path()), Err(ModelFileError::WrongFileName { .. })));
        std::fs::remove_file(models.join("wrong.json")).unwrap();
        // No models folder at all is fine: a plugin may have no data.
        let empty = tempfile::tempdir().unwrap();
        assert!(read_models(empty.path()).unwrap().is_empty());
    }

    #[test]
    fn chatter_is_off_unless_the_model_asks_and_its_rules_are_checked() {
        let mut model = note();
        assert!(model.chatter_on().is_none(), "off by default");
        model.chatter = Some(ChatterDef::default());
        assert!(model.chatter_on().is_none(), "present but not enabled is still off");
        model.chatter = Some(ChatterDef { enabled: true, ..ChatterDef::default() });
        assert!(model.chatter_on().is_some());
        assert!(model.problems().is_empty(), "{:?}", model.problems());

        // Tracking needs chatter; visitors need it too; json cannot be tracked.
        let mut tracked = note();
        tracked.fields[0].track = true;
        assert!(tracked.problems().join(";").contains("track"), "chatter is off");
        tracked.chatter = Some(ChatterDef { enabled: true, ..ChatterDef::default() });
        assert!(tracked.problems().is_empty());
        tracked.chatter = Some(ChatterDef { enabled: true, track_changes: false, ..ChatterDef::default() });
        assert!(!tracked.problems().is_empty(), "track_changes is off");

        let mut visitors = note();
        visitors.chatter = Some(ChatterDef { visitors: VisitorChatter::Read, ..ChatterDef::default() });
        assert!(visitors.problems().join(";").contains("visitors"), "not enabled");
        visitors.chatter = Some(ChatterDef { enabled: true, messages: false, visitors: VisitorChatter::ReadWrite, ..ChatterDef::default() });
        assert!(!visitors.problems().is_empty(), "visitors post messages");

        let mut json_field = note();
        json_field.fields.push(serde_json::from_value(json!({ "id": "fld_jjjjjjjjjj", "name": "extra", "type": "json", "track": true })).unwrap());
        json_field.chatter = Some(ChatterDef { enabled: true, ..ChatterDef::default() });
        assert!(json_field.problems().join(";").contains("json fields cannot be tracked"));
    }

    #[test]
    fn syncing_assigns_ids_once_writes_them_back_and_never_changes_them() {
        let directory = tempfile::tempdir().unwrap();
        let models = directory.path().join(MODEL_DIR);
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(
            models.join("note.json"),
            r#"{ "name": "note", "fields": [{ "name": "title", "type": "string" }] }"#,
        )
        .unwrap();

        let first = sync_package(directory.path()).unwrap();
        assert_eq!(first[0].assigned, 2, "the model and its field");
        let text = std::fs::read_to_string(models.join("note.json")).unwrap();
        let written: ModelDef = serde_json::from_str(&text).unwrap();
        assert!(written.problems().is_empty());

        // Run again, and after renaming the field by hand: the ids stay.
        assert_eq!(sync_package(directory.path()).unwrap()[0].assigned, 0);
        let renamed = text.replace("\"title\"", "\"heading\"");
        std::fs::write(models.join("note.json"), renamed).unwrap();
        sync_package(directory.path()).unwrap();
        let after: ModelDef = serde_json::from_str(&std::fs::read_to_string(models.join("note.json")).unwrap()).unwrap();
        assert_eq!(after.fields[0].name, "heading");
        assert_eq!(after.fields[0].id, written.fields[0].id, "a rename keeps the column");
        assert_eq!(after.model_id, written.model_id);
    }

    #[test]
    fn syncing_reports_what_else_is_wrong() {
        let directory = tempfile::tempdir().unwrap();
        let models = directory.path().join(MODEL_DIR);
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("note.json"), r#"{ "name": "note", "fields": [{ "name": "Bad Name", "type": "string" }] }"#).unwrap();
        assert!(matches!(sync_package(directory.path()), Err(ModelFileError::Invalid { .. })));
    }

    #[test]
    fn unknown_keys_in_a_model_file_are_an_error() {
        let result: Result<ModelDef, _> = serde_json::from_value(json!({
            "name": "x", "fields": [], "colour": "red"
        }));
        assert!(result.is_err(), "typos must not be silently ignored");
    }

    // ---- links to another plugin's model

    fn invoice_with_foreign_link(target_id: Option<&str>) -> ModelDef {
        let mut model: ModelDef = serde_json::from_value(serde_json::json!({
            "name": "invoice",
            "fields": [{ "name": "currency", "type": "link", "target": "currency.currency" }]
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        sync_ids(&mut model);
        model.fields[0].target_id = target_id.map(str::to_string);
        model
    }

    #[test]
    fn a_link_to_another_plugins_model_needs_its_id() {
        let problems = invoice_with_foreign_link(None).problems();
        assert!(problems.iter().any(|p| p.contains("needs its `target_id`") && p.contains("--sync-models")), "{problems:?}");
        let problems = invoice_with_foreign_link(Some("not an id")).problems();
        assert!(problems.iter().any(|p| p.contains("is not like")), "{problems:?}");
        assert!(invoice_with_foreign_link(Some("mdl_k3v9xq2m7a")).problems().is_empty());
        // A set does not look for `currency.currency` among its own models.
        let model = invoice_with_foreign_link(Some("mdl_k3v9xq2m7a"));
        assert!(validate_set(&[model]).is_ok());
    }

    #[test]
    fn only_a_foreign_link_has_a_target_id_and_a_bad_target_is_refused() {
        let mut own: ModelDef = serde_json::from_value(serde_json::json!({
            "name": "note", "fields": [{ "name": "parent", "type": "link", "target": "note", "target_id": "mdl_k3v9xq2m7a" }]
        }))
        .unwrap_or_else(|error| panic!("{error}"));
        sync_ids(&mut own);
        assert!(own.problems().iter().any(|p| p.contains("only a link to another plugin's model has a `target_id`")));
        for target in ["Currency.currency", "a.b.c", "currency.", ".currency", "cur-rency.x"] {
            let mut model = invoice_with_foreign_link(Some("mdl_k3v9xq2m7a"));
            model.fields[0].target = Some(target.to_string());
            assert!(!model.problems().is_empty(), "{target}");
        }
    }

    #[test]
    fn syncing_fills_a_foreign_links_id_from_what_the_catalog_knows() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join(MODEL_DIR))?;
        let file = dir.path().join(MODEL_DIR).join("invoice.json");
        std::fs::write(
            &file,
            serde_json::json!({ "name": "invoice", "fields": [
                { "name": "currency", "type": "link", "target": "currency.currency" },
                { "name": "owner", "type": "link", "target": "party.party" }
            ] })
            .to_string(),
        )?;
        let wanted = foreign_targets(dir.path())?;
        assert_eq!(wanted.into_iter().collect::<Vec<_>>(), ["currency.currency", "party.party"]);

        // Only one is known: the other is reported, not guessed.
        let known: HashMap<String, String> = [("currency.currency".to_string(), "mdl_k3v9xq2m7a".to_string())].into();
        let refused = sync_package_with(dir.path(), &known).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(refused.contains("party.party") && refused.contains("--sync-models"), "{refused}");
        let written: ModelDef = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
        assert_eq!(written.fields[0].target_id.as_deref(), Some("mdl_k3v9xq2m7a"));

        let all: HashMap<String, String> = [
            ("currency.currency".to_string(), "mdl_k3v9xq2m7a".to_string()),
            ("party.party".to_string(), "mdl_p4rty0000a".to_string()),
        ]
        .into();
        sync_package_with(dir.path(), &all)?;
        let written: ModelDef = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
        // The id that was already there is never changed.
        assert_eq!(written.fields[0].target_id.as_deref(), Some("mdl_k3v9xq2m7a"));
        assert_eq!(written.fields[1].target_id.as_deref(), Some("mdl_p4rty0000a"));
        Ok(())
    }

    #[test]
    fn multi_field_indexes_are_checked() {
        let mut model: ModelDef = serde_json::from_value(serde_json::json!({
            "name": "pos", "fields": [
                { "name": "a", "type": "string" }, { "name": "b", "type": "int" },
                { "name": "t", "type": "text" }, { "name": "j", "type": "json" }
            ],
            "indexes": [ { "fields": ["a", "b"], "unique": true } ]
        })).unwrap_or_else(|e| panic!("{e}"));
        assert!(model.problems().iter().any(|p| p.contains("no `id`")));
        sync_ids(&mut model);
        assert!(model.problems().is_empty(), "{:?}", model.problems());
        model.indexes[0].fields = vec!["a".into()];
        assert!(model.problems().iter().any(|p| p.contains("2 to 8")));
        model.indexes[0].fields = vec!["a".into(), "a".into()];
        assert!(model.problems().iter().any(|p| p.contains("twice")));
        model.indexes[0].fields = vec!["a".into(), "zzz".into()];
        assert!(model.problems().iter().any(|p| p.contains("not a field")));
        model.indexes[0].fields = vec!["a".into(), "t".into()];
        assert!(model.problems().iter().any(|p| p.contains("cannot be indexed")));
    }

    #[test]
    fn decimal_scale_is_checked() {
        let mut model: ModelDef = serde_json::from_value(serde_json::json!({
            "name": "pay", "fields": [ { "name": "a", "type": "decimal", "scale": 12 }, { "name": "b", "type": "int", "scale": 2 } ]
        })).unwrap_or_else(|e| panic!("{e}"));
        sync_ids(&mut model);
        let problems = model.problems();
        assert!(problems.iter().any(|p| p.contains("at most")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("only for decimal")), "{problems:?}");
    }
}

/// A bound as a comparable number: a whole number, a float, or a decimal's scaled whole number
/// (as a float, enough to compare two bounds).
fn number_of(kind: FieldType, bound: &Value, scale: u32) -> Result<f64, String> {
    match kind {
        FieldType::Int => bound.as_i64().map(|n| n as f64).ok_or_else(|| "must be a whole number".to_string()),
        FieldType::Float => bound.as_f64().filter(|n| n.is_finite()).ok_or_else(|| "must be a number".to_string()),
        _ => super::decimal::to_scaled(bound, scale).map(|n| n as f64),
    }
}
