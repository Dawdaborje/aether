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
}

impl ModelSchema {
    /// `all` is every model of the plugin, to find where links point. Hidden (deprecated)
    /// fields are not part of the schema: plugins cannot see them.
    pub fn new(model: &ModelDef, all: &[ModelDef]) -> Option<Self> {
        let table = model.model_id.clone()?;
        let mut columns = Vec::new();
        for field in model.live_fields() {
            let link_table = field.target.as_ref().and_then(|target| {
                all.iter().find(|other| &other.name == target).and_then(|other| other.model_id.clone())
            });
            columns.push(Column { def: field.clone(), id: field.id.clone()?, link_table });
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
        Some(Self { name: model.name.clone(), table, chatter, tracked, title_column, columns, by_name })
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
            super::definition::FieldType::Int | super::definition::FieldType::Float => Ok(&column.id),
            _ => Err(self.invalid(name, "only a whole number or number field can be incremented")),
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
        Ok(value.clone())
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
            stored.insert(column.id.clone(), value.clone());
        }
        for column in &self.columns {
            if stored.contains_key(&column.id) {
                continue;
            }
            match &column.def.default {
                Some(default) => {
                    stored.insert(column.id.clone(), default.clone());
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
            set.insert(column.id.clone(), value.clone());
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
                record.insert(column.def.name.clone(), value.clone());
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
