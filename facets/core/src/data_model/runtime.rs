//! Between a plugin and the database: names to ids, and every value checked.
//!
//! Plugin code says `title`; the database stores `fld_k3v9xq2m7a`. A [`ModelSchema`] does the
//! translation both ways and refuses anything the model does not allow (an unknown field, a
//! wrong type, a missing required value), so what is stored always matches the definition.

use std::collections::HashMap;

use serde_json::{Map, Value};
use thiserror::Error;

use super::definition::{FieldDef, FieldType, ModelDef};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("model `{model}` has no field `{field}`")]
    UnknownField { model: String, field: String },
    #[error("{model}.{field}: {reason}")]
    InvalidValue {
        model: String,
        field: String,
        reason: String,
    },
    #[error("{model}.{field} is required")]
    Required { model: String, field: String },
}

#[derive(Debug, Clone)]
struct Column {
    def: FieldDef,
    id: String,
    /// For a link: the table its records are in.
    link_table: Option<String>,
    /// For a decimal: digits after the point.
    scale: Option<u32>,
}

impl Column {
    /// A value as the database keeps it: a decimal becomes a whole number of its smallest unit.
    fn stored(&self, value: &Value) -> Result<Value, String> {
        match self.scale {
            Some(scale) => super::decimal::to_scaled(value, scale).map(Value::from),
            None => Ok(value.clone()),
        }
    }
}

/// The graph edge table of a hierarchy link or many2many field. Field ids are only unique inside
/// one model, so the model's id is part of the name.
pub fn edge_table(model_id: &str, field_id: &str) -> String {
    format!("edg_{model_id}_{field_id}")
}

/// A many2many field: edges from this model's records to records of `target_table`.
#[derive(Debug, Clone)]
pub struct Relation {
    pub field: String,
    pub edge: String,
    pub target_table: String,
}

/// A hierarchy link: the parent column, and the edges (parent to child) kept in step with it.
#[derive(Debug, Clone)]
pub struct Hierarchy {
    pub field: String,
    pub column: String,
    pub edge: String,
}

/// One model as plugin code sees it: its fields by name, with the ids they are stored under.
#[derive(Debug, Clone)]
pub struct ModelSchema {
    pub name: String,
    /// The table: the model's id.
    pub table: String,
    /// The model's chatter, when it is on.
    pub chatter: Option<super::definition::ChatterDef>,
    /// Ids of the fields whose changes are recorded in the chatter.
    pub tracked: Vec<String>,
    /// Id of the field that titles a record (for notifications).
    pub title_column: Option<String>,
    columns: Vec<Column>,
    by_name: HashMap<String, usize>,
    /// The model's many2many fields.
    pub relations: Vec<Relation>,
    /// The model's hierarchy links.
    pub hierarchies: Vec<Hierarchy>,
}

impl ModelSchema {
    /// `all` is every model of the plugin, to find where links point. Hidden (deprecated)
    /// fields are not part of the schema: plugins cannot see them.
    pub fn new(model: &ModelDef, all: &[ModelDef]) -> Option<Self> {
        let table = model.model_id.clone()?;
        let mut columns = Vec::new();
        let mut relations = Vec::new();
        let mut hierarchies = Vec::new();
        for field in model.live_fields() {
            if field.kind == super::definition::FieldType::Many2many {
                let target_table = field.target.as_ref().and_then(|target| match super::definition::foreign_target(target) {
                    Some(_) => field.target_id.clone(),
                    None => all.iter().find(|other| &other.name == target).and_then(|other| other.model_id.clone()),
                });
                if let (Some(target_table), Some(id)) = (target_table, field.id.as_deref()) {
                    relations.push(Relation { field: field.name.clone(), edge: edge_table(&table, id), target_table });
                }
                continue;
            }
            let link_table = field.target.as_ref().and_then(|target| match super::definition::foreign_target(target) {
                // Another plugin's model: its id was written into the field by `--sync-models`.
                Some(_) => field.target_id.clone(),
                None => all.iter().find(|other| &other.name == target).and_then(|other| other.model_id.clone()),
            });
            let scale = (field.kind == super::definition::FieldType::Decimal)
                .then(|| field.scale.unwrap_or(super::decimal::DEFAULT_SCALE));
            if field.hierarchy {
                hierarchies.push(Hierarchy { field: field.name.clone(), column: field.id.clone()?, edge: edge_table(&table, field.id.as_deref()?) });
            }
            columns.push(Column { def: field.clone(), id: field.id.clone()?, link_table, scale });
        }
        let by_name = columns
            .iter()
            .enumerate()
            .map(|(index, column)| (column.def.name.clone(), index))
            .collect();
        let chatter = model.chatter_on().cloned();
        let tracked = match &chatter {
            Some(chatter) if chatter.track_changes => columns
                .iter()
                .filter(|column| column.def.track)
                .map(|column| column.id.clone())
                .collect(),
            _ => Vec::new(),
        };
        let title_column = model
            .title_field
            .as_ref()
            .and_then(|title| columns.iter().find(|column| &column.def.name == title))
            .map(|column| column.id.clone());
        Some(Self { name: model.name.clone(), table, chatter, tracked, title_column, columns, by_name, relations, hierarchies })
    }

    fn column(&self, name: &str) -> Result<&Column, SchemaError> {
        self.by_name
            .get(name)
            .map(|index| &self.columns[*index])
            .ok_or_else(|| SchemaError::UnknownField { model: self.name.clone(), field: name.to_string() })
    }

    fn invalid(&self, field: &str, reason: impl Into<String>) -> SchemaError {
        SchemaError::InvalidValue { model: self.name.clone(), field: field.to_string(), reason: reason.into() }
    }

    fn check(&self, column: &Column, value: &Value) -> Result<(), SchemaError> {
        check_value(&column.def, value, column.link_table.as_deref())
            .map_err(|reason| self.invalid(&column.def.name, reason))
    }

    /// What the chatter panel needs to show a tracked change: for each tracked field id, its
    /// current name, label, type and (for a choice) the options. Looked up by id, so a field
    /// renamed after the change was recorded still reads correctly.
    pub fn tracked_fields(&self) -> Vec<Value> {
        self.columns
            .iter()
            .filter(|column| self.tracked.contains(&column.id))
            .map(|column| {
                serde_json::json!({
                    "id": column.id,
                    "name": column.def.name,
                    "label": column.def.label.clone().unwrap_or_else(|| column.def.name.clone()),
                    "type": column.def.kind.as_str(),
                    "scale": column.scale,
                    "options": column.def.options.iter().map(|o| serde_json::json!({
                        "value": o.value,
                        "label": o.label.clone().unwrap_or_else(|| o.value.clone()),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect()
    }

    /// The id a number field is stored under, for adding to it in place.
    pub fn numeric_column(&self, name: &str) -> Result<&str, SchemaError> {
        let column = self.column(name)?;
        match column.def.kind {
            super::definition::FieldType::Int
            | super::definition::FieldType::Float
            | super::definition::FieldType::Decimal => Ok(&column.id),
            _ => Err(self.invalid(name, "only a whole number, number or decimal field can be incremented")),
        }
    }

    /// The many2many field called `name`.
    pub fn relation(&self, name: &str) -> Result<&Relation, SchemaError> {
        self.relations.iter().find(|relation| relation.field == name).ok_or_else(|| {
            self.invalid(name, "is not a many2many field of this model")
        })
    }

    /// The hierarchy link called `name`.
    pub fn hierarchy(&self, name: &str) -> Result<&Hierarchy, SchemaError> {
        self.hierarchies.iter().find(|hierarchy| hierarchy.field == name).ok_or_else(|| {
            self.invalid(name, "is not a hierarchy link of this model")
        })
    }

    /// Digits after the point of a decimal field; `None` for any other kind.
    pub fn scale_of(&self, name: &str) -> Result<Option<u32>, SchemaError> {
        Ok(self.column(name)?.scale)
    }

    /// An amount to add to `name`, as the database keeps it. A decimal takes text or a number
    /// (and must fit its scale); any other numeric field takes a number as it is.
    pub fn increment_by(&self, name: &str, by: &Value) -> Result<Value, SchemaError> {
        let column = self.column(name)?;
        match column.scale {
            Some(scale) => super::decimal::to_scaled(by, scale)
                .map(Value::from)
                .map_err(|reason| self.invalid(name, reason)),
            None if by.is_number() => Ok(by.clone()),
            None => Err(self.invalid(name, "`by` must be a number")),
        }
    }

    /// The id a field is stored under, for filters and ordering.
    pub fn column_id(&self, name: &str) -> Result<&str, SchemaError> {
        Ok(&self.column(name)?.id)
    }

    /// A value to compare a field with in a filter.
    pub fn filter_value(&self, name: &str, value: &Value) -> Result<Value, SchemaError> {
        let column = self.column(name)?;
        if value.is_null() {
            return Err(self.invalid(name, "cannot filter by null"));
        }
        self.check(column, value)?;
        column.stored(value).map_err(|reason| self.invalid(name, reason))
    }

    /// A new record, keyed by field id: values checked, defaults filled in, required fields present.
    /// A null is the same as leaving the field out.
    pub fn encode_create(&self, data: &Map<String, Value>) -> Result<Map<String, Value>, SchemaError> {
        let mut stored = Map::new();
        for (name, value) in data {
            let column = self.column(name)?;
            if value.is_null() {
                continue;
            }
            self.check(column, value)?;
            stored.insert(column.id.clone(), column.stored(value).map_err(|reason| self.invalid(name, reason))?);
        }
        for column in &self.columns {
            if stored.contains_key(&column.id) {
                continue;
            }
            match &column.def.default {
                Some(default) => {
                    let default = column.stored(default).map_err(|reason| self.invalid(&column.def.name, reason))?;
                    stored.insert(column.id.clone(), default);
                }
                None if column.def.required => {
                    return Err(SchemaError::Required { model: self.name.clone(), field: column.def.name.clone() });
                }
                None => {}
            }
        }
        Ok(stored)
    }

    /// A change to a record: the values to set (keyed by field id) and the ids to clear. A null
    /// clears an optional field.
    pub fn encode_update(
        &self,
        data: &Map<String, Value>,
    ) -> Result<(Map<String, Value>, Vec<String>), SchemaError> {
        let mut set = Map::new();
        let mut clear = Vec::new();
        for (name, value) in data {
            let column = self.column(name)?;
            if value.is_null() {
                if column.def.required {
                    return Err(self.invalid(name, "is required, so it cannot be cleared"));
                }
                clear.push(column.id.clone());
                continue;
            }
            self.check(column, value)?;
            set.insert(column.id.clone(), column.stored(value).map_err(|reason| self.invalid(name, reason))?);
        }
        Ok((set, clear))
    }

    /// A stored record as the plugin sees it: field names instead of ids; the record's own `id`
    /// kept; anything the model does not (or no longer) shows left out.
    pub fn decode(&self, row: &Value) -> Value {
        let Value::Object(stored) = row else {
            return row.clone();
        };
        let mut record = Map::new();
        if let Some(id) = stored.get("id") {
            record.insert("id".into(), id.clone());
        }
        for column in &self.columns {
            if let Some(value) = stored.get(&column.id) {
                let shown = match column.scale {
                    Some(scale) => super::decimal::present(value, scale),
                    None => value.clone(),
                };
                record.insert(column.def.name.clone(), shown);
            }
        }
        Value::Object(record)
    }
}

/// The schema of each model of a plugin, by model name.
pub fn schemas_of(models: &[ModelDef]) -> HashMap<String, std::sync::Arc<ModelSchema>> {
    models
        .iter()
        .filter_map(|model| {
            ModelSchema::new(model, models).map(|schema| (model.name.clone(), std::sync::Arc::new(schema)))
        })
        .collect()
}

/// Whether `value` is allowed for `field`. `link_table` is the table a link must point into,
/// when it is known.
pub fn check_value(field: &FieldDef, value: &Value, link_table: Option<&str>) -> Result<(), String> {
    let text = value.as_str();
    match field.kind {
        FieldType::String | FieldType::Text => {
            let text = text.ok_or("must be text")?;
            if let Some(max) = field.max_length
                && text.chars().count() > max as usize
            {
                return Err(format!("is longer than {max} characters"));
            }
            Ok(())
        }
        FieldType::Int => value.as_i64().map(|_| ()).ok_or_else(|| "must be a whole number".to_string()),
        FieldType::Float => {
            if value.as_f64().is_some_and(f64::is_finite) {
                Ok(())
            } else {
                Err("must be a number".into())
            }
        }
        FieldType::Decimal => super::decimal::to_scaled(value, field.scale.unwrap_or(super::decimal::DEFAULT_SCALE)).map(|_| ()),
        FieldType::Bool => value.as_bool().map(|_| ()).ok_or_else(|| "must be true or false".to_string()),
        FieldType::Date => {
            if text.is_some_and(is_date) {
                Ok(())
            } else {
                Err("must be a date like 2026-10-03".into())
            }
        }
        FieldType::Datetime => {
            if text.is_some_and(is_datetime) {
                Ok(())
            } else {
                Err("must be a timestamp like 2026-10-03T21:00:00Z".into())
            }
        }
        FieldType::Select => {
            let text = text.ok_or("must be one of the options")?;
            if field.options.iter().any(|option| option.value == text) {
                Ok(())
            } else {
                let allowed: Vec<&str> = field.options.iter().map(|o| o.value.as_str()).collect();
                Err(format!("must be one of: {}", allowed.join(", ")))
            }
        }
        FieldType::Link => {
            let text = text.ok_or("must be a record id like `table:key`")?;
            let (table, key) = text.split_once(':').ok_or("must be a record id like `table:key`")?;
            if table.is_empty() || key.is_empty() {
                return Err("must be a record id like `table:key`".into());
            }
            match link_table {
                Some(expected) if expected != table => Err(format!("must be a record of the linked model, not of `{table}`")),
                _ => Ok(()),
            }
        }
        FieldType::Json => Ok(()),
        FieldType::Many2many => Err("is a relation: change it with db::relate, not as a value".into()),
    }
}

fn digits(text: &str, count: usize) -> Option<u32> {
    (text.len() == count && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse().ok())?
}

fn is_date(text: &str) -> bool {
    let mut parts = text.split('-');
    let (Some(year), Some(month), Some(day), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let (Some(year), Some(month), Some(day)) = (digits(year, 4), digits(month, 2), digits(day, 2)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let longest = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=longest).contains(&day)
}

fn is_datetime(text: &str) -> bool {
    let Some((date, time)) = text.split_once(['T', 't']) else {
        return false;
    };
    if !is_date(date) {
        return false;
    }
    // The time, then `Z` or an offset.
    let (clock, zone_ok) = if let Some(clock) = time.strip_suffix(['Z', 'z']) {
        (clock, true)
    } else if let Some(index) = time.rfind(['+', '-']) {
        let (clock, offset) = time.split_at(index);
        let offset = &offset[1..];
        let ok = offset
            .split_once(':')
            .is_some_and(|(h, m)| digits(h, 2).is_some_and(|h| h < 24) && digits(m, 2).is_some_and(|m| m < 60));
        (clock, ok)
    } else {
        (time, false)
    };
    if !zone_ok {
        return false;
    }
    let (clock, fraction) = clock.split_once('.').map_or((clock, None), |(c, f)| (c, Some(f)));
    if fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    let mut parts = clock.split(':');
    let (Some(h), Some(m), Some(s), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    matches!(
        (digits(h, 2), digits(m, 2), digits(s, 2)),
        (Some(h), Some(m), Some(s)) if h < 24 && m < 60 && s < 61
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_model::definition::sync_ids;
    use serde_json::json;

    fn schema() -> (ModelSchema, ModelDef) {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "note",
            "fields": [
                { "name": "title", "type": "string", "required": true, "max_length": 10 },
                { "name": "body", "type": "text" },
                { "name": "pages", "type": "int" },
                { "name": "score", "type": "float" },
                { "name": "done", "type": "bool", "default": false },
                { "name": "due", "type": "date" },
                { "name": "seen", "type": "datetime" },
                { "name": "status", "type": "select", "default": "draft",
                  "options": [{ "value": "draft" }, { "value": "shared" }] },
                { "name": "parent", "type": "link", "target": "note" },
                { "name": "extra", "type": "json" },
                { "name": "old", "type": "string", "deprecated": true }
            ]
        }))
        .unwrap();
        sync_ids(&mut model);
        (ModelSchema::new(&model, std::slice::from_ref(&model)).unwrap(), model)
    }

    fn data(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn a_decimal_is_stored_in_whole_units_and_shown_as_text() -> Result<(), SchemaError> {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "pay",
            "fields": [
                { "name": "amount", "type": "decimal", "required": true },
                { "name": "days", "type": "decimal", "scale": 1, "default": "1.5" }
            ]
        }))
        .map_err(|e| SchemaError::InvalidValue { model: "pay".into(), field: String::new(), reason: e.to_string() })?;
        sync_ids(&mut model);
        let schema = ModelSchema::new(&model, std::slice::from_ref(&model))
            .ok_or_else(|| SchemaError::UnknownField { model: "pay".into(), field: String::new() })?;
        let id = |name: &str| model.fields.iter().find(|f| f.name == name).and_then(|f| f.id.clone()).unwrap_or_default();

        let stored = schema.encode_create(&data(json!({ "amount": "12.34" })))?;
        assert_eq!(stored[&id("amount")], json!(1234));
        assert_eq!(stored[&id("days")], json!(15), "the default, in whole units");
        let shown = schema.decode(&Value::Object(stored));
        assert_eq!(shown["amount"], json!("12.34"));
        assert_eq!(shown["days"], json!("1.5"));

        assert!(schema.encode_create(&data(json!({ "amount": "1.005" }))).is_err(), "too many digits");
        assert!(schema.encode_create(&data(json!({ "amount": "abc" }))).is_err());
        assert_eq!(schema.filter_value("amount", &json!("0.10"))?, json!(10));
        assert_eq!(schema.increment_by("amount", &json!("-2.5"))?, json!(-250));
        assert_eq!(schema.increment_by("days", &json!(2))?, json!(20));
        Ok(())
    }

    #[test]
    fn a_create_is_stored_under_ids_with_defaults_filled_in() {
        let (schema, model) = schema();
        let stored = schema.encode_create(&data(json!({ "title": "Plan", "pages": 3 }))).unwrap();
        let id = |name: &str| model.fields.iter().find(|f| f.name == name).unwrap().id.clone().unwrap();
        assert_eq!(stored[&id("title")], "Plan");
        assert_eq!(stored[&id("pages")], 3);
        assert_eq!(stored[&id("done")], false, "the default");
        assert_eq!(stored[&id("status")], "draft", "the default");
        assert!(!stored.contains_key("title"), "names are never stored");
        assert_eq!(schema.table, model.model_id.unwrap());
    }

    #[test]
    fn refuses_unknown_hidden_missing_and_wrong_values() {
        let (schema, _) = schema();
        let refused = |value: Value| schema.encode_create(&data(value)).unwrap_err();
        assert!(matches!(refused(json!({ "titel": "x" })), SchemaError::UnknownField { .. }));
        assert!(matches!(refused(json!({ "title": "x", "old": "y" })), SchemaError::UnknownField { .. }), "hidden fields do not exist for plugins");
        assert!(matches!(refused(json!({ "body": "no title" })), SchemaError::Required { .. }));
        assert!(matches!(refused(json!({ "title": null })), SchemaError::Required { .. }), "null is absent");
        for bad in [
            json!({ "title": "much too long a title" }),
            json!({ "title": "x", "pages": "3" }),
            json!({ "title": "x", "pages": 3.5 }),
            json!({ "title": "x", "score": "high" }),
            json!({ "title": "x", "done": "yes" }),
            json!({ "title": "x", "due": "2026-02-30" }),
            json!({ "title": "x", "seen": "yesterday" }),
            json!({ "title": "x", "status": "archived" }),
            json!({ "title": "x", "parent": "no-colon" }),
            json!({ "title": "x", "parent": "other_table:1" }),
        ] {
            assert!(matches!(refused(bad.clone()), SchemaError::InvalidValue { .. }), "{bad}");
        }
    }

    #[test]
    fn dates_times_and_links_are_checked_properly() {
        let (schema, model) = schema();
        let table = model.model_id.unwrap();
        let ok = json!({
            "title": "x", "due": "2028-02-29", "seen": "2026-10-03T21:00:00.123+02:00",
            "parent": format!("{table}:abc"), "extra": { "any": ["thing"] }, "score": 2
        });
        assert!(schema.encode_create(&data(ok)).is_ok());
        for bad_date in ["2026-13-01", "2026-00-10", "2027-02-29", "26-01-01", "2026-1-1"] {
            assert!(schema.encode_create(&data(json!({ "title": "x", "due": bad_date }))).is_err(), "{bad_date}");
        }
        for bad_time in ["2026-10-03", "2026-10-03T25:00:00Z", "2026-10-03T21:00:00", "2026-10-03T21:00Z"] {
            assert!(schema.encode_create(&data(json!({ "title": "x", "seen": bad_time }))).is_err(), "{bad_time}");
        }
    }

    #[test]
    fn an_update_sets_checked_values_and_clears_with_null() {
        let (schema, model) = schema();
        let body = model.fields.iter().find(|f| f.name == "body").unwrap().id.clone().unwrap();
        let (set, clear) = schema.encode_update(&data(json!({ "title": "New", "body": null }))).unwrap();
        assert_eq!(set.len(), 1);
        assert_eq!(clear, [body]);
        assert!(schema.encode_update(&data(json!({ "title": null }))).is_err(), "a required field cannot be cleared");
        assert!(schema.encode_update(&data(json!({ "pages": "x" }))).is_err());
        assert!(schema.encode_update(&data(json!({}))).unwrap().0.is_empty());
    }

    #[test]
    fn filters_and_ordering_use_ids_and_checked_values() {
        let (schema, model) = schema();
        let title = model.fields[0].id.clone().unwrap();
        assert_eq!(schema.column_id("title").unwrap(), title);
        assert!(schema.column_id("nope").is_err());
        assert!(schema.column_id("old").is_err());
        assert!(schema.filter_value("status", &json!("shared")).is_ok());
        assert!(schema.filter_value("status", &json!("nope")).is_err());
        assert!(schema.filter_value("title", &Value::Null).is_err());
    }

    #[test]
    fn records_come_back_with_names_not_ids_and_without_hidden_or_unknown_columns() {
        let (schema, model) = schema();
        let id = |name: &str| model.fields.iter().find(|f| f.name == name).unwrap().id.clone().unwrap();
        let row = json!({
            "id": "tbl:abc", id("title"): "Plan", id("old"): "hidden", "fld_stray": 1
        });
        assert_eq!(schema.decode(&row), json!({ "id": "tbl:abc", "title": "Plan" }));
    }

    #[test]
    fn renaming_a_field_changes_nothing_that_is_stored() {
        let (_, mut model) = schema();
        let before = ModelSchema::new(&model, std::slice::from_ref(&model)).unwrap();
        let stored = before.encode_create(&data(json!({ "title": "Plan" }))).unwrap();
        model.fields[0].name = "heading".into();
        let after = ModelSchema::new(&model, std::slice::from_ref(&model)).unwrap();
        // The same stored record, read through the renamed model.
        let row = Value::Object(stored);
        assert_eq!(after.decode(&row)["heading"], "Plan");
        assert!(after.encode_create(&data(json!({ "title": "x" }))).is_err(), "the old name is gone from the code's point of view");
    }
}

#[cfg(test)]
mod foreign_link_tests {
    use super::*;
    use crate::data_model::ModelDef;

    fn invoice() -> Result<ModelDef, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "model_id": "mdl_inv0000001", "name": "invoice",
            "fields": [
                { "id": "fld_cur0000001", "name": "currency", "type": "link", "target": "currency.currency", "target_id": "mdl_cur0000001" },
                { "id": "fld_par0000001", "name": "parent", "type": "link", "target": "invoice" }
            ]
        }))
    }

    #[test]
    fn a_link_to_another_plugins_model_must_point_into_that_models_table() -> Result<(), Box<dyn std::error::Error>> {
        let model = invoice()?;
        let schema = ModelSchema::new(&model, std::slice::from_ref(&model)).ok_or("no schema")?;
        let mut data = serde_json::Map::new();
        data.insert("currency".into(), serde_json::json!("mdl_cur0000001:abc"));
        assert!(schema.encode_create(&data).is_ok());
        data.insert("currency".into(), serde_json::json!("mdl_other00001:abc"));
        let refused = schema.encode_create(&data).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(refused.contains("linked model"), "{refused}");
        // A link inside the plugin still resolves through the plugin's own models.
        data.insert("currency".into(), serde_json::json!("mdl_cur0000001:abc"));
        data.insert("parent".into(), serde_json::json!("mdl_inv0000001:x"));
        assert!(schema.encode_create(&data).is_ok());
        Ok(())
    }
}
