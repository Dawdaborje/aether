//! Between a plugin and the database: names to ids, and every value checked.
//!
//! Plugin code says `title`; the database stores `fld_k3v9xq2m7a`. A [`ModelSchema`] does the
//! translation both ways and refuses anything the model does not allow (an unknown field, a
//! wrong type, a missing required value), so what is stored always matches the definition.

use std::collections::HashMap;

use serde_json::{Map, Value};
use thiserror::Error;

use super::definition::{FieldDef, FieldType, ModelDef};
use super::query::Filter;
use super::sequence::Pattern;

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
    /// For text: the expression the whole value must match.
    pattern: Option<regex::Regex>,
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

/// A plain (non-hierarchy) link: the column it is stored in, and the table it must point into.
/// The kernel checks the target record exists whenever the link is written.
#[derive(Debug, Clone)]
pub struct LinkCheck {
    pub field: String,
    pub column: String,
    pub table: String,
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
    /// The model's other links, whose targets must exist.
    pub links: Vec<LinkCheck>,
    /// The model's naming series.
    pub sequences: Vec<SequenceField>,
    /// The rules every record must satisfy after a write.
    pub checks: Vec<ModelCheck>,
    /// The fields the kernel calculates, in the order they are filled.
    pub derived: Vec<DerivedField>,
    /// The model's child fields.
    pub children: Vec<ChildField>,
    /// Models whose totals count rows of this one.
    pub parents: Vec<ParentLink>,
}

/// A `child` field: rows of another model that point back at this model's records.
#[derive(Debug, Clone)]
pub struct ChildField {
    pub field: String,
    /// The rows' model, by name, and its table.
    pub model: String,
    pub table: String,
    /// The link of the rows that points back, by name and by stored column.
    pub inverse_field: String,
    pub inverse_column: String,
    /// The `int` field the kernel numbers the rows with, by name and column.
    pub order: Option<(String, String)>,
    /// The rows' whole-number and decimal fields (by name), for totals over the rows.
    pub numbers: HashMap<String, super::compute::Operand>,
}

/// A model that has this one as a `child` field and totals over its rows: when a row is written
/// on its own, the parent's totals are brought up to date.
#[derive(Debug, Clone)]
pub struct ParentLink {
    pub schema: Box<ModelSchema>,
    /// The row's column that holds the parent's id.
    pub inverse_column: String,
}

/// How a calculated field gets its value.
#[derive(Debug, Clone)]
pub enum Derivation {
    /// A copy of `source` (a column of the linked model's table) of the record `link` points at.
    Related { link: String, source: String },
    /// An expression over the record's own number columns, in whole units of `scale` digits.
    Compute { expr: super::compute::Expr, scale: u32, operands: HashMap<String, super::compute::Operand> },
}

/// A field the kernel fills in on every write.
#[derive(Debug, Clone)]
pub struct DerivedField {
    pub field: String,
    pub column: String,
    pub how: Derivation,
}

/// A field that numbers its records.
#[derive(Debug, Clone)]
pub struct SequenceField {
    pub field: String,
    pub column: String,
    pub pattern: Pattern,
    pub reset: super::definition::SequenceReset,
}

/// One of the model's `checks`, parsed.
#[derive(Debug, Clone)]
pub struct ModelCheck {
    pub filter: Filter,
    pub message: String,
}

impl ModelSchema {
    /// `all` is every model of the plugin, to find where links point. Hidden (deprecated)
    /// fields are not part of the schema: plugins cannot see them.
    pub fn new(model: &ModelDef, all: &[ModelDef]) -> Option<Self> {
        let table = model.model_id.clone()?;
        let mut columns = Vec::new();
        let mut relations = Vec::new();
        let mut hierarchies = Vec::new();
        let mut links = Vec::new();
        let mut sequences = Vec::new();
        let mut children = Vec::new();
        for field in model.live_fields() {
            if field.kind == super::definition::FieldType::Child {
                let rows = all.iter().find(|other| Some(&other.name) == field.target.as_ref())?;
                let inverse = rows.live_fields().find(|f| Some(&f.name) == field.inverse.as_ref())?;
                let order = match &field.order {
                    Some(name) => {
                        let f = rows.live_fields().find(|f| &f.name == name)?;
                        Some((f.name.clone(), f.id.clone()?))
                    }
                    None => None,
                };
                let mut numbers = HashMap::new();
                for f in rows.live_fields() {
                    let scale = match f.kind {
                        super::definition::FieldType::Int => 0,
                        super::definition::FieldType::Decimal => f.scale.unwrap_or(super::decimal::DEFAULT_SCALE),
                        _ => continue,
                    };
                    numbers.insert(f.name.clone(), super::compute::Operand { column: f.id.clone()?, scale });
                }
                children.push(ChildField {
                    field: field.name.clone(),
                    model: rows.name.clone(),
                    table: rows.model_id.clone()?,
                    inverse_field: inverse.name.clone(),
                    inverse_column: inverse.id.clone()?,
                    order,
                    numbers,
                });
                continue;
            }
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
            if field.kind == super::definition::FieldType::Link && !field.hierarchy {
                if let Some(table) = &link_table {
                    links.push(LinkCheck { field: field.name.clone(), column: field.id.clone()?, table: table.clone() });
                }
            }
            let pattern = match &field.pattern {
                Some(text) => Some(compile_pattern(text).ok()?),
                None => None,
            };
            if let Some(sequence) = &field.sequence {
                sequences.push(SequenceField {
                    field: field.name.clone(),
                    column: field.id.clone()?,
                    pattern: Pattern::parse(&sequence.pattern).ok()?,
                    reset: sequence.reset,
                });
            }
            columns.push(Column { def: field.clone(), id: field.id.clone()?, link_table, scale, pattern });
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
        let mut derived = Vec::new();
        for field in model.live_fields() {
            if let Some(related) = &field.related {
                let (link_name, source_name) = related.split_once('.')?;
                let link = model.live_fields().find(|f| f.name == link_name)?;
                let target = all.iter().find(|m| Some(&m.name) == link.target.as_ref())?;
                let source = target.live_fields().find(|f| f.name == source_name)?;
                derived.push(DerivedField {
                    field: field.name.clone(),
                    column: field.id.clone()?,
                    how: Derivation::Related { link: link.id.clone()?, source: source.id.clone()? },
                });
            }
        }
        for field in model.live_fields() {
            if let Some(compute) = &field.compute {
                let expr = super::compute::Expr::parse(compute).ok()?;
                let mut operands = HashMap::new();
                for name in expr.fields() {
                    let input = model.live_fields().find(|f| f.name == name)?;
                    let scale = match input.kind {
                        super::definition::FieldType::Int => 0,
                        _ => input.scale.unwrap_or(super::decimal::DEFAULT_SCALE),
                    };
                    operands.insert(name, super::compute::Operand { column: input.id.clone()?, scale });
                }
                let scale = match field.kind {
                    super::definition::FieldType::Int => 0,
                    _ => field.scale.unwrap_or(super::decimal::DEFAULT_SCALE),
                };
                derived.push(DerivedField { field: field.name.clone(), column: field.id.clone()?, how: Derivation::Compute { expr, scale, operands } });
            }
        }
        let mut checks = Vec::new();
        for check in &model.checks {
            checks.push(ModelCheck { filter: Filter::parse(&check.require).ok()?, message: check.message.clone() });
        }
        Some(Self {
            name: model.name.clone(),
            table,
            chatter,
            tracked,
            title_column,
            columns,
            by_name,
            relations,
            hierarchies,
            links,
            sequences,
            checks,
            derived,
            children,
            parents: Vec::new(),
        })
    }

    fn column(&self, name: &str) -> Result<&Column, SchemaError> {
        self.by_name
            .get(name)
            .map(|index| &self.columns[*index])
            .ok_or_else(|| SchemaError::UnknownField { model: self.name.clone(), field: name.to_string() })
    }

    fn refuse_if_calculated(&self, column: &Column) -> Result<(), SchemaError> {
        match (&column.def.related, &column.def.compute) {
            (Some(related), _) => Err(self.invalid(&column.def.name, format!("is copied from `{related}` by the kernel, so it cannot be written"))),
            (_, Some(_)) => Err(self.invalid(&column.def.name, "is calculated by the kernel, so it cannot be written")),
            _ => Ok(()),
        }
    }

    fn invalid(&self, field: &str, reason: impl Into<String>) -> SchemaError {
        SchemaError::InvalidValue { model: self.name.clone(), field: field.to_string(), reason: reason.into() }
    }

    fn check(&self, column: &Column, value: &Value) -> Result<(), SchemaError> {
        check_value(&column.def, value, column.link_table.as_deref())
            .map_err(|reason| self.invalid(&column.def.name, reason))?;
        if let (Some(pattern), Some(text)) = (&column.pattern, value.as_str())
            && !pattern.is_match(text)
        {
            return Err(self.invalid(&column.def.name, "is not in the expected format"));
        }
        Ok(())
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

    /// The child field called `name`.
    pub fn child(&self, name: &str) -> Option<&ChildField> {
        self.children.iter().find(|child| child.field == name)
    }

    /// A new record like [`Self::encode_create`], where the link `inverse` (by name) is filled in
    /// by the kernel afterwards, so it may be missing from `data`.
    pub fn encode_create_row(&self, data: &Map<String, Value>, inverse: &str) -> Result<Map<String, Value>, SchemaError> {
        self.encode_create_inner(data, Some(inverse))
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
        self.encode_create_inner(data, None)
    }

    fn encode_create_inner(&self, data: &Map<String, Value>, filled_later: Option<&str>) -> Result<Map<String, Value>, SchemaError> {
        let mut stored = Map::new();
        for (name, value) in data {
            let column = self.column(name)?;
            self.refuse_if_calculated(column)?;
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
                // A numbered field gets its value from the series when the record is stored.
                None if column.def.required && column.def.sequence.is_none() && filled_later != Some(column.def.name.as_str()) => {
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
            self.refuse_if_calculated(column)?;
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
    let mut schemas: Vec<ModelSchema> = models.iter().filter_map(|model| ModelSchema::new(model, models)).collect();
    // A model that totals over its child rows must be told when a row is written on its own.
    let parents: Vec<(String, ParentLink)> = schemas
        .iter()
        .flat_map(|parent| {
            parent.children.iter().filter_map(move |child| {
                let totals = parent.derived.iter().any(|d| matches!(&d.how, Derivation::Compute { expr, .. }
                    if expr.rollups().iter().any(|r| r.child == child.field)));
                totals.then(|| (child.model.clone(), ParentLink { schema: Box::new(parent.clone()), inverse_column: child.inverse_column.clone() }))
            })
        })
        .collect();
    for (rows_model, link) in parents {
        if let Some(rows) = schemas.iter_mut().find(|schema| schema.name == rows_model) {
            rows.parents.push(link);
        }
    }
    schemas.into_iter().map(|schema| (schema.name.clone(), std::sync::Arc::new(schema))).collect()
}

/// Whether `value` is allowed for `field`. `link_table` is the table a link must point into,
/// when it is known.
pub fn check_value(field: &FieldDef, value: &Value, link_table: Option<&str>) -> Result<(), String> {
    check_type(field, value, link_table)?;
    check_limits(field, value)
}

/// A pattern as a size-limited expression, so a model file cannot make a write expensive. The
/// whole value must match.
pub fn compile_pattern(pattern: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(&format!("^(?:{pattern})$"))
        .size_limit(1 << 18)
        .build()
        .map_err(|error| format!("is not a valid expression: {error}"))
}

/// `min`, `max` and `min_length`, for a value that already has the right type.
fn check_limits(field: &FieldDef, value: &Value) -> Result<(), String> {
    use std::cmp::Ordering;
    let bound = |limit: &Option<Value>, compare: &dyn Fn(Ordering) -> bool, what: &str| -> Result<(), String> {
        let Some(limit) = limit else { return Ok(()) };
        let ordering = match field.kind {
            FieldType::Int => value.as_i64().zip(limit.as_i64()).map(|(a, b)| a.cmp(&b)),
            FieldType::Float => value.as_f64().zip(limit.as_f64()).and_then(|(a, b)| a.partial_cmp(&b)),
            FieldType::Decimal => {
                let scale = field.scale.unwrap_or(super::decimal::DEFAULT_SCALE);
                super::decimal::to_scaled(value, scale).ok().zip(super::decimal::to_scaled(limit, scale).ok()).map(|(a, b)| a.cmp(&b))
            }
            _ => None,
        };
        match ordering {
            Some(ordering) if !compare(ordering) => Err(format!("must be {what} {}", limit_text(limit))),
            _ => Ok(()),
        }
    };
    bound(&field.min, &|o| o != Ordering::Less, "at least")?;
    bound(&field.max, &|o| o != Ordering::Greater, "at most")?;
    if let (Some(min), Some(text)) = (field.min_length, value.as_str())
        && text.chars().count() < min as usize
    {
        return Err(format!("is shorter than {min} characters"));
    }
    Ok(())
}

fn limit_text(limit: &Value) -> String {
    match limit {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn check_type(field: &FieldDef, value: &Value, link_table: Option<&str>) -> Result<(), String> {
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
        FieldType::Child => Err("is a list of rows: write it with db::create or db::update".into()),
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

    #[test]
    fn plain_links_are_listed_for_the_existence_check_but_hierarchy_links_are_not() -> Result<(), Box<dyn std::error::Error>> {
        let mut model = invoice()?;
        for field in &mut model.fields {
            if field.name == "parent" {
                field.hierarchy = true;
            }
        }
        let schema = ModelSchema::new(&model, std::slice::from_ref(&model)).ok_or("no schema")?;
        let links: Vec<(&str, &str)> = schema.links.iter().map(|l| (l.field.as_str(), l.table.as_str())).collect();
        assert_eq!(links, vec![("currency", "mdl_cur0000001")]);
        Ok(())
    }

    fn limited() -> Result<ModelDef, serde_json::Error> {
        serde_json::from_value(serde_json::json!({
            "model_id": "mdl_lim0000001", "name": "limited",
            "fields": [
                { "id": "fld_qty0000001", "name": "qty", "type": "int", "min": 1, "max": 10 },
                { "id": "fld_prc0000001", "name": "price", "type": "decimal", "scale": 2, "min": "0.50", "max": "99.99" },
                { "id": "fld_code000001", "name": "code", "type": "string", "pattern": "[A-Z]{3}-[0-9]+", "min_length": 5 },
                { "id": "fld_num0000001", "name": "number", "type": "string", "required": true,
                  "sequence": { "pattern": "N-{#####}" } }
            ]
        }))
    }

    #[test]
    fn numbers_and_text_stay_inside_their_limits() -> Result<(), Box<dyn std::error::Error>> {
        let model = limited()?;
        assert!(model.problems().is_empty(), "{:?}", model.problems());
        let schema = ModelSchema::new(&model, std::slice::from_ref(&model)).ok_or("no schema")?;
        let refused = |field: &str, value: serde_json::Value| -> String {
            let mut data = serde_json::Map::new();
            data.insert(field.into(), value);
            schema.encode_update(&data).err().map(|e| e.to_string()).unwrap_or_default()
        };
        let ok = |field: &str, value: serde_json::Value| -> bool {
            let mut data = serde_json::Map::new();
            data.insert(field.into(), value);
            schema.encode_update(&data).is_ok()
        };
        assert!(ok("qty", serde_json::json!(1)) && ok("qty", serde_json::json!(10)));
        assert!(refused("qty", serde_json::json!(0)).contains("at least 1"));
        assert!(refused("qty", serde_json::json!(11)).contains("at most 10"));
        assert!(ok("price", serde_json::json!("0.50")) && ok("price", serde_json::json!(99.99)));
        assert!(refused("price", serde_json::json!("0.49")).contains("at least 0.50"));
        assert!(refused("price", serde_json::json!("100")).contains("at most 99.99"));
        assert!(ok("code", serde_json::json!("ABC-12")));
        assert!(refused("code", serde_json::json!("abc-12")).contains("expected format"));
        // Longer than the minimum but not the pattern, and the whole value must match.
        assert!(refused("code", serde_json::json!("ABC-12x")).contains("expected format"));
        assert!(refused("code", serde_json::json!("A-1")).contains("shorter than 5"));
        Ok(())
    }

    #[test]
    fn a_numbered_field_may_be_left_out_of_a_create() -> Result<(), Box<dyn std::error::Error>> {
        let model = limited()?;
        let schema = ModelSchema::new(&model, std::slice::from_ref(&model)).ok_or("no schema")?;
        assert_eq!(schema.sequences.len(), 1);
        assert!(schema.encode_create(&serde_json::Map::new()).is_ok(), "the series supplies `number`");
        Ok(())
    }

    #[test]
    fn nonsense_limits_are_refused_when_the_model_loads() -> Result<(), Box<dyn std::error::Error>> {
        let mut model = limited()?;
        for field in &mut model.fields {
            match field.name.as_str() {
                "qty" => field.min = Some(serde_json::json!(20)),
                "code" => field.pattern = Some("(".into()),
                "number" => field.sequence = Some(crate::data_model::definition::SequenceDef { pattern: "NO-NUMBER".into(), reset: Default::default() }),
                _ => {}
            }
        }
        let problems = model.problems().join("; ");
        assert!(problems.contains("`min` is above `max`"), "{problems}");
        assert!(problems.contains("`pattern`"), "{problems}");
        assert!(problems.contains("exactly one number"), "{problems}");
        Ok(())
    }

    #[test]
    fn a_check_naming_a_missing_field_is_refused_when_the_model_loads() -> Result<(), Box<dyn std::error::Error>> {
        let mut model = limited()?;
        model.checks = serde_json::from_value(serde_json::json!([
            { "require": { "or": [ { "qty": { "null": true } }, { "qty": { "gte": { "field": "nope" } } } ] }, "message": "bad" }
        ]))?;
        let problems = model.problems().join("; ");
        assert!(problems.contains("nope"), "{problems}");
        model.checks = serde_json::from_value(serde_json::json!([
            { "require": { "or": [ { "qty": { "null": true } }, { "qty": { "gte": 5 } } ] }, "message": "qty is at least 5" }
        ]))?;
        assert!(model.problems().is_empty(), "{:?}", model.problems());
        Ok(())
    }
}
