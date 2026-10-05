//! Reading records: filters, counts and aggregates, turned into SurrealQL.
//!
//! Plugins never write SurrealQL. They describe what they want as a **filter**, and this module
//! is the one place that turns it into a `WHERE` clause. Column names always come from the
//! model (a field name is looked up in the [`ModelSchema`] and replaced by its stored id), and
//! values are always bound, never spliced into the text, so a filter cannot change the shape of
//! the query.
//!
//! A filter is a JSON object. Each key is either a field name or one of `and`, `or`, `not`:
//!
//! ```json
//! { "status": "active", "salary": { "gte": 1000, "lt": 5000 },
//!   "or": [ { "dept": "hr" }, { "dept": { "in": ["it", "ops"] } } ],
//!   "not": { "manager": { "null": true } } }
//! ```
//!
//! * `"field": value` is an equality test; `"field": { op: value, … }` takes one or more of
//!   `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `nin` (also true for a record with no value), `like` (case-insensitive substring) and
//!   `null` (`true`: the field is empty; `false`: it has a value).
//! * Every key of one object must hold, so an object is an AND. `and` / `or` take a list of
//!   filters, `not` takes one.
//!
//! The old flat form (`{"field": value, …}`) is a filter of equalities, so existing plugins keep
//! working unchanged.

use std::collections::HashSet;

use serde_json::{Map, Value};
use thiserror::Error;

use super::runtime::{ModelSchema, SchemaError};

/// The deepest `and` / `or` / `not` nesting accepted.
const MAX_DEPTH: usize = 8;
/// The most comparisons in one filter, so a request cannot build an enormous statement.
const MAX_TERMS: usize = 64;
/// The most values in an `in` / `nin` list.
const MAX_LIST: usize = 500;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QueryError {
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error("invalid filter: {0}")]
    Invalid(String),
}

fn invalid(reason: impl Into<String>) -> QueryError {
    QueryError::Invalid(reason.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Nin,
    Like,
    Null,
}

impl Op {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "gt" => Self::Gt,
            "gte" => Self::Gte,
            "lt" => Self::Lt,
            "lte" => Self::Lte,
            "in" => Self::In,
            "nin" => Self::Nin,
            "like" => Self::Like,
            "null" => Self::Null,
            _ => return None,
        })
    }
}

/// A parsed filter: a tree of comparisons joined by and / or / not.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
    Cmp { field: String, op: Op, value: Value },
    /// Matches nothing. A rule that depends on something the caller does not have (an employee
    /// record, say) reduces to this.
    Never,
}

impl Filter {
    /// Read a filter from its JSON form. An empty object matches everything.
    pub fn parse(object: &Map<String, Value>) -> Result<Self, QueryError> {
        let mut terms = 0;
        parse_object(object, 0, &mut terms)
    }

    /// Whether this filter matches every record.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::And(parts) if parts.is_empty())
    }
}

fn parse_object(object: &Map<String, Value>, depth: usize, terms: &mut usize) -> Result<Filter, QueryError> {
    if depth > MAX_DEPTH {
        return Err(invalid(format!("nested more than {MAX_DEPTH} levels deep")));
    }
    let mut parts = Vec::new();
    for (key, value) in object {
        match key.as_str() {
            "and" | "or" => {
                let list = value
                    .as_array()
                    .ok_or_else(|| invalid(format!("`{key}` takes a list of filters")))?;
                let mut inner = Vec::new();
                for item in list {
                    let item = item
                        .as_object()
                        .ok_or_else(|| invalid(format!("`{key}` takes a list of filters")))?;
                    inner.push(parse_object(item, depth + 1, terms)?);
                }
                parts.push(if key == "and" { Filter::And(inner) } else { Filter::Or(inner) });
            }
            "not" => {
                let inner = value.as_object().ok_or_else(|| invalid("`not` takes one filter"))?;
                parts.push(Filter::Not(Box::new(parse_object(inner, depth + 1, terms)?)));
            }
            field => parse_field(field, value, terms, &mut parts)?,
        }
    }
    Ok(Filter::And(parts))
}

fn parse_field(field: &str, value: &Value, terms: &mut usize, parts: &mut Vec<Filter>) -> Result<(), QueryError> {
    let mut push = |op: Op, value: Value| -> Result<(), QueryError> {
        *terms += 1;
        if *terms > MAX_TERMS {
            return Err(invalid(format!("more than {MAX_TERMS} comparisons")));
        }
        parts.push(Filter::Cmp { field: field.to_string(), op, value });
        Ok(())
    };
    match value {
        // `{ "gte": 1, "lt": 5 }` is a set of operators; any other object is not a value a
        // field can hold, so it is a mistake rather than something to compare with.
        Value::Object(ops) => {
            if ops.is_empty() {
                return Err(invalid(format!("`{field}` has an empty operator list")));
            }
            for (name, operand) in ops {
                let op = Op::parse(name).ok_or_else(|| invalid(format!("`{field}`: unknown operator `{name}`")))?;
                push(op, operand.clone())?;
            }
            Ok(())
        }
        other => push(Op::Eq, other.clone()),
    }
}

/// How field names and values are looked up while compiling. [`ModelSchema`] is the real one;
/// [`Unchecked`] passes names through for tables that have no model definition.
pub trait Columns {
    fn column(&self, field: &str) -> Result<String, QueryError>;
    fn value(&self, field: &str, value: &Value) -> Result<Value, QueryError>;
    /// Digits after the point when `field` is a decimal (stored as whole units).
    fn scale(&self, _field: &str) -> Option<u32> {
        None
    }
}

impl Columns for ModelSchema {
    fn column(&self, field: &str) -> Result<String, QueryError> {
        let column = self.column_id(field)?.to_string();
        validate_ident(&column)?;
        Ok(column)
    }

    fn value(&self, field: &str, value: &Value) -> Result<Value, QueryError> {
        Ok(self.filter_value(field, value)?)
    }

    fn scale(&self, field: &str) -> Option<u32> {
        self.scale_of(field).ok().flatten()
    }
}

/// Field names used as they are, for system tables without a model.
pub struct Unchecked;

impl Columns for Unchecked {
    fn column(&self, field: &str) -> Result<String, QueryError> {
        validate_ident(field)?;
        Ok(field.to_string())
    }

    fn value(&self, _field: &str, value: &Value) -> Result<Value, QueryError> {
        Ok(value.clone())
    }
}

fn validate_ident(name: &str) -> Result<(), QueryError> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(invalid(format!("invalid identifier `{name}`")));
    }
    Ok(())
}

/// A statement fragment and the values it binds.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Compiled {
    pub sql: String,
    pub binds: Vec<(String, Value)>,
}

impl Filter {
    /// A filter that matches every record.
    pub fn always() -> Self {
        Self::And(Vec::new())
    }

    /// Both this and `other` must hold.
    pub fn and(self, other: Filter) -> Filter {
        match (self, other) {
            (Filter::And(a), Filter::And(b)) if a.is_empty() => Filter::And(b),
            (Filter::And(a), other) if a.is_empty() => other,
            (this, Filter::And(b)) if b.is_empty() => this,
            (Filter::And(mut a), Filter::And(b)) => {
                a.extend(b);
                Filter::And(a)
            }
            (a, b) => Filter::And(vec![a, b]),
        }
    }

    /// Either this or `other` holds.
    pub fn or(self, other: Filter) -> Filter {
        match (self, other) {
            (Filter::Never, other) | (other, Filter::Never) => other,
            (a, b) if a.is_empty() || b.is_empty() => Filter::always(),
            (Filter::Or(mut a), Filter::Or(b)) => {
                a.extend(b);
                Filter::Or(a)
            }
            (a, b) => Filter::Or(vec![a, b]),
        }
    }

    /// The names of the fields the filter compares.
    pub fn fields(&self) -> Vec<&str> {
        match self {
            Filter::And(parts) | Filter::Or(parts) => parts.iter().flat_map(Filter::fields).collect(),
            Filter::Not(inner) => inner.fields(),
            Filter::Cmp { field, .. } => vec![field.as_str()],
            Filter::Never => Vec::new(),
        }
    }
}

/// Turn a filter into a `WHERE` condition (without the keyword); an empty string matches all.
/// Bind names start with `prefix`, so several compiled pieces can share one statement.
pub fn compile_filter(filter: &Filter, columns: &dyn Columns, prefix: &str) -> Result<Compiled, QueryError> {
    let mut out = Compiled::default();
    if filter.is_empty() {
        return Ok(out);
    }
    out.sql = compile_node(filter, columns, prefix, &mut out.binds)?;
    Ok(out)
}

fn bind(binds: &mut Vec<(String, Value)>, prefix: &str, value: Value) -> String {
    let name = format!("{prefix}{}", binds.len());
    binds.push((name.clone(), value));
    format!("${name}")
}

fn compile_node(
    node: &Filter,
    columns: &dyn Columns,
    prefix: &str,
    binds: &mut Vec<(String, Value)>,
) -> Result<String, QueryError> {
    match node {
        Filter::And(parts) | Filter::Or(parts) => {
            let (joiner, empty) = if matches!(node, Filter::And(_)) { (" AND ", "true") } else { (" OR ", "false") };
            let mut pieces = Vec::new();
            for part in parts {
                pieces.push(compile_node(part, columns, prefix, binds)?);
            }
            Ok(match pieces.len() {
                0 => empty.to_string(),
                1 => pieces.remove(0),
                _ => format!("({})", pieces.join(joiner)),
            })
        }
        Filter::Not(inner) => Ok(format!("!({})", compile_node(inner, columns, prefix, binds)?)),
        Filter::Cmp { field, op, value } => compile_cmp(field, *op, value, columns, prefix, binds),
        Filter::Never => Ok("false".to_string()),
    }
}

fn compile_cmp(
    field: &str,
    op: Op,
    value: &Value,
    columns: &dyn Columns,
    prefix: &str,
    binds: &mut Vec<(String, Value)>,
) -> Result<String, QueryError> {
    let column = columns.column(field)?;
    match op {
        Op::Null => {
            let wants_empty = value
                .as_bool()
                .ok_or_else(|| invalid(format!("`{field}`: `null` takes true or false")))?;
            Ok(format!("{column} {} NONE", if wants_empty { "=" } else { "!=" }))
        }
        Op::In | Op::Nin => {
            let list = value
                .as_array()
                .ok_or_else(|| invalid(format!("`{field}`: `in` takes a list")))?;
            if list.len() > MAX_LIST {
                return Err(invalid(format!("`{field}`: more than {MAX_LIST} values in a list")));
            }
            let checked = list
                .iter()
                .map(|item| columns.value(field, item))
                .collect::<Result<Vec<_>, _>>()?;
            let placeholder = bind(binds, prefix, Value::Array(checked));
            Ok(if op == Op::In {
                format!("{column} IN {placeholder}")
            } else {
                format!("{column} NOT IN {placeholder}")
            })
        }
        Op::Like => {
            let text = value
                .as_str()
                .ok_or_else(|| invalid(format!("`{field}`: `like` takes text")))?;
            let placeholder = bind(binds, prefix, Value::String(text.to_string()));
            Ok(format!(
                "string::contains(string::lowercase(<string> {column}), string::lowercase({placeholder}))"
            ))
        }
        Op::Eq | Op::Ne | Op::Gt | Op::Gte | Op::Lt | Op::Lte => {
            let checked = columns.value(field, value)?;
            let placeholder = bind(binds, prefix, checked);
            let symbol = match op {
                Op::Eq => "=",
                Op::Ne => "!=",
                Op::Gt => ">",
                Op::Gte => ">=",
                Op::Lt => "<",
                _ => "<=",
            };
            Ok(format!("{column} {symbol} {placeholder}"))
        }
    }
}

/// One figure an aggregate computes over each group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agg {
    Count,
    Sum(String),
    Avg(String),
    Min(String),
    Max(String),
}

/// A grouped summary: which fields to group by, and which figures to compute per group.
/// With no `group_by` the whole filtered set is one group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aggregate {
    pub group_by: Vec<String>,
    /// Result name and figure, in the order asked.
    pub figures: Vec<(String, Agg)>,
}

impl Aggregate {
    /// Read `{ "group_by": ["dept"], "aggs": { "n": "count", "total": { "sum": "salary" } } }`.
    pub fn parse(group_by: &[String], aggs: &Map<String, Value>) -> Result<Self, QueryError> {
        if aggs.is_empty() {
            return Err(invalid("an aggregate needs at least one figure"));
        }
        let mut figures = Vec::new();
        for (name, spec) in aggs {
            validate_ident(name)?;
            let agg = match spec {
                Value::String(kind) if kind == "count" => Agg::Count,
                Value::Object(one) if one.len() == 1 => {
                    let (kind, field) = one.iter().next().ok_or_else(|| invalid("empty figure"))?;
                    let field = field
                        .as_str()
                        .ok_or_else(|| invalid(format!("`{name}`: name a field")))?
                        .to_string();
                    match kind.as_str() {
                        "sum" => Agg::Sum(field),
                        "avg" => Agg::Avg(field),
                        "min" => Agg::Min(field),
                        "max" => Agg::Max(field),
                        other => return Err(invalid(format!("`{name}`: unknown figure `{other}`"))),
                    }
                }
                _ => return Err(invalid(format!("`{name}`: use \"count\" or {{\"sum\": \"field\"}}"))),
            };
            figures.push((name.clone(), agg));
        }
        // A result name used twice (as a group field and as a figure) would overwrite a column.
        let mut seen = HashSet::new();
        for name in group_by.iter().chain(figures.iter().map(|(name, _)| name)) {
            if !seen.insert(name.as_str()) {
                return Err(invalid(format!("`{name}` is used twice in the result")));
            }
        }
        Ok(Self { group_by: group_by.to_vec(), figures })
    }

    /// The names of the fields the summary looks at: grouped and summed.
    pub fn fields(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.group_by.iter().map(String::as_str).collect();
        for (_, agg) in &self.figures {
            match agg {
                Agg::Count => {}
                Agg::Sum(f) | Agg::Avg(f) | Agg::Min(f) | Agg::Max(f) => names.push(f.as_str()),
            }
        }
        names
    }

    /// Which result columns hold decimals, with their scale: a sum, minimum, maximum or average
    /// of a decimal field, which the database computes in whole units.
    pub fn decimal_results(&self, columns: &dyn Columns) -> Vec<(String, u32)> {
        let mut found = Vec::new();
        for (name, agg) in &self.figures {
            let field = match agg {
                Agg::Sum(field) | Agg::Avg(field) | Agg::Min(field) | Agg::Max(field) => field,
                Agg::Count => continue,
            };
            if let Some(scale) = columns.scale(field) {
                found.push((name.clone(), scale));
            }
        }
        // A group-by field that is a decimal comes back in whole units too.
        for field in &self.group_by {
            if let Some(scale) = columns.scale(field) {
                found.push((field.clone(), scale));
            }
        }
        found
    }

    /// The `SELECT` list and `GROUP` clause for this summary.
    pub fn compile(&self, columns: &dyn Columns) -> Result<(String, String), QueryError> {
        let mut select = Vec::new();
        let mut group = Vec::new();
        for field in &self.group_by {
            validate_ident(field)?;
            let column = columns.column(field)?;
            select.push(format!("{column} AS {field}"));
            group.push(field.clone());
        }
        for (name, agg) in &self.figures {
            select.push(match agg {
                Agg::Count => format!("count() AS {name}"),
                Agg::Sum(field) => format!("math::sum({}) AS {name}", columns.column(field)?),
                Agg::Avg(field) => format!("math::mean({}) AS {name}", columns.column(field)?),
                Agg::Min(field) => format!("math::min({}) AS {name}", columns.column(field)?),
                Agg::Max(field) => format!("math::max({}) AS {name}", columns.column(field)?),
            });
        }
        let group_clause = if group.is_empty() { "GROUP ALL".to_string() } else { format!("GROUP BY {}", group.join(", ")) };
        Ok((select.join(", "), group_clause))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn filter(value: Value) -> Result<Filter, QueryError> {
        match value {
            Value::Object(map) => Filter::parse(&map),
            _ => Err(invalid("not an object")),
        }
    }

    fn sql(value: Value) -> Result<Compiled, QueryError> {
        compile_filter(&filter(value)?, &Unchecked, "f")
    }

    #[test]
    fn flat_filter_is_a_set_of_equalities() -> Result<(), QueryError> {
        let compiled = sql(json!({ "status": "active", "dept": "hr" }))?;
        assert!(compiled.sql.contains("dept = $f"));
        assert!(compiled.sql.contains(" AND "));
        assert!(compiled.sql.contains("status = $f"));
        assert_eq!(compiled.binds.len(), 2);
        Ok(())
    }

    #[test]
    fn empty_filter_matches_everything() -> Result<(), QueryError> {
        assert!(sql(json!({}))?.sql.is_empty());
        Ok(())
    }

    #[test]
    fn operators_or_and_not() -> Result<(), QueryError> {
        let compiled = sql(json!({
            "salary": { "gte": 10 },
            "or": [{ "dept": "hr" }, { "dept": { "in": ["it", "ops"] } }],
            "not": { "boss": { "null": true } }
        }))?;
        assert!(compiled.sql.contains("salary >= $f"));
        assert!(compiled.sql.contains(" OR "));
        assert!(compiled.sql.contains("dept IN $f"));
        assert!(compiled.sql.contains("!(boss = NONE)"));
        Ok(())
    }

    #[test]
    fn values_are_bound_not_spliced() -> Result<(), QueryError> {
        let compiled = sql(json!({ "name": "x' OR true --" }))?;
        assert!(!compiled.sql.contains("true"));
        assert_eq!(compiled.binds[0].1, json!("x' OR true --"));
        Ok(())
    }

    #[test]
    fn hostile_field_names_are_refused() {
        assert!(sql(json!({ "name = 1; DELETE x; --": 1 })).is_err());
        assert!(sql(json!({ "a b": 1 })).is_err());
    }

    #[test]
    fn unknown_operator_and_bad_shapes_are_refused() {
        assert!(sql(json!({ "a": { "xx": 1 } })).is_err());
        assert!(sql(json!({ "a": {} })).is_err());
        assert!(sql(json!({ "a": { "in": 3 } })).is_err());
        assert!(sql(json!({ "a": { "null": "yes" } })).is_err());
        assert!(sql(json!({ "or": { "a": 1 } })).is_err());
        assert!(sql(json!({ "not": [1] })).is_err());
    }

    #[test]
    fn nesting_and_size_are_bounded() {
        let mut deep = json!({ "a": 1 });
        for _ in 0..(MAX_DEPTH + 2) {
            deep = json!({ "not": deep });
        }
        assert!(sql(deep).is_err());
        let many: Vec<Value> = (0..MAX_TERMS + 1).map(|i| json!({ "a": i })).collect();
        assert!(sql(json!({ "or": many })).is_err());
        let long: Vec<Value> = (0..MAX_LIST + 1).map(Value::from).collect();
        assert!(sql(json!({ "a": { "in": long } })).is_err());
    }

    #[test]
    fn like_is_case_insensitive_and_bound() -> Result<(), QueryError> {
        let compiled = sql(json!({ "name": { "like": "Jo" } }))?;
        assert!(compiled.sql.starts_with("string::contains(string::lowercase("));
        assert_eq!(compiled.binds[0].1, json!("Jo"));
        Ok(())
    }

    #[test]
    fn aggregate_builds_select_and_group() -> Result<(), QueryError> {
        let aggs = json!({ "n": "count", "total": { "sum": "salary" } });
        let aggs = aggs.as_object().ok_or_else(|| invalid("object"))?;
        let aggregate = Aggregate::parse(&["dept".to_string()], aggs)?;
        let (select, group) = aggregate.compile(&Unchecked)?;
        assert_eq!(select, "dept AS dept, count() AS n, math::sum(salary) AS total");
        assert_eq!(group, "GROUP BY dept");
        let all = Aggregate::parse(&[], aggs)?;
        assert_eq!(all.compile(&Unchecked)?.1, "GROUP ALL");
        Ok(())
    }

    #[test]
    fn aggregate_refuses_bad_specs() {
        let empty = Map::new();
        assert!(Aggregate::parse(&[], &empty).is_err());
        let clash = json!({ "dept": "count" });
        assert!(Aggregate::parse(&["dept".to_string()], clash.as_object().unwrap_or(&empty)).is_err());
        let unknown = json!({ "x": { "median": "salary" } });
        assert!(Aggregate::parse(&[], unknown.as_object().unwrap_or(&empty)).is_err());
        let bad_name = json!({ "x; DROP": "count" });
        assert!(Aggregate::parse(&[], bad_name.as_object().unwrap_or(&empty)).is_err());
    }
}
