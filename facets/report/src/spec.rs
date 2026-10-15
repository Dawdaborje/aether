//! `report::query` payload: dimensions, measures, and an optional fill.

use serde_json::{Map, Value};
use thiserror::Error;

/// Rows shown on screen before a download. Organizations may override this.
pub const PREVIEW_DEFAULT: u32 = 10;
pub const PREVIEW_CHOICES: &[u32] = &[5, 10, 25, 50];

/// Snap a setting value onto an allowed preview length.
pub fn normalize_preview_rows(value: Option<&Value>) -> u32 {
    let n = match value {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as u32,
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    };
    if PREVIEW_CHOICES.contains(&n) {
        n
    } else {
        PREVIEW_DEFAULT
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReportError {
    #[error("{0}")]
    Invalid(String),
}

fn invalid(reason: impl Into<String>) -> ReportError {
    ReportError::Invalid(reason.into())
}

fn ident(name: &str, what: &str) -> Result<(), ReportError> {
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Ok(())
    } else {
        Err(invalid(format!("{what} `{name}` is not a field name")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    Day,
    Week,
    Month,
}

impl Grain {
    fn parse(raw: &str) -> Result<Self, ReportError> {
        match raw {
            "day" => Ok(Self::Day),
            "week" => Ok(Self::Week),
            "month" => Ok(Self::Month),
            other => Err(invalid(format!("unknown date grain `{other}`"))),
        }
    }

    /// `time::format` pattern. Fixed strings, never taken from the caller.
    pub fn format(self) -> &'static str {
        match self {
            Self::Day => "%Y-%m-%d",
            Self::Week => "%Y-%W",
            Self::Month => "%Y-%m",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    pub field: String,
    pub grain: Option<Grain>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Measure {
    Count,
    Sum(String),
    Avg(String),
    Min(String),
    Max(String),
    /// Count of rows matching this filter (same language as `db::find`).
    CountIf(Map<String, Value>),
    AvgDays { from: String, to: AvgDaysTo },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvgDaysTo {
    Field(String),
    Now,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub model: String,
    pub key: String,
    pub label: String,
    pub filter: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportQuery {
    pub model: String,
    pub filter: Map<String, Value>,
    pub dimensions: Vec<Dimension>,
    /// Result name and measure, in request order.
    pub measures: Vec<(String, Measure)>,
    pub fill: Option<Fill>,
    /// `-name` is descending.
    pub order: Option<String>,
    pub limit: u32,
    /// When set, the host returns only the preview slice. Totals stay complete.
    pub preview: bool,
}

impl ReportQuery {
    pub fn parse(value: &Value) -> Result<Self, ReportError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("report query must be an object"))?;
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("model is required"))?
            .to_string();
        if model.contains('.') {
            let (plugin, name) = model.split_once('.').unwrap();
            ident(plugin, "plugin")?;
            ident(name, "model")?;
        } else {
            ident(&model, "model")?;
        }

        let filter = match object.get("filter") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            _ => return Err(invalid("filter must be an object")),
        };

        let mut dimensions = Vec::new();
        if let Some(raw) = object.get("dimensions") {
            let list = raw
                .as_array()
                .ok_or_else(|| invalid("dimensions must be a list"))?;
            if list.len() > 8 {
                return Err(invalid("at most 8 dimensions"));
            }
            for item in list {
                dimensions.push(parse_dimension(item)?);
            }
        }

        let measures_value = object
            .get("measures")
            .or_else(|| object.get("aggs"))
            .ok_or_else(|| invalid("measures are required"))?;
        let measures_object = measures_value
            .as_object()
            .ok_or_else(|| invalid("measures must be an object"))?;
        if measures_object.is_empty() {
            return Err(invalid("a report needs at least one measure"));
        }
        if measures_object.len() > 16 {
            return Err(invalid("at most 16 measures"));
        }
        let mut measures = Vec::new();
        for (name, spec) in measures_object {
            ident(name, "measure")?;
            measures.push((name.clone(), parse_measure(name, spec)?));
        }

        let mut seen = std::collections::HashSet::new();
        for dim in &dimensions {
            if !seen.insert(dim.field.as_str()) {
                return Err(invalid(format!("`{}` is used twice in the result", dim.field)));
            }
        }
        for (name, _) in &measures {
            if !seen.insert(name.as_str()) {
                return Err(invalid(format!("`{name}` is used twice in the result")));
            }
        }

        let fill = match object.get("fill") {
            None | Some(Value::Null) => None,
            Some(value) => Some(parse_fill(value)?),
            };

        let order = object
            .get("order")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if let Some(order) = &order {
            let name = order.strip_prefix('-').unwrap_or(order);
            ident(name, "order")?;
            if !seen.contains(name) {
                return Err(invalid(format!("order `{name}` is not a result column")));
            }
        }

        let limit = object
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(1000)
            .min(1000) as u32;
        if limit == 0 {
            return Err(invalid("limit must be at least 1"));
        }
        let preview = object.get("preview").and_then(Value::as_bool).unwrap_or(false);

        Ok(Self {
            model,
            filter,
            dimensions,
            measures,
            fill,
            order,
            limit,
            preview,
        })
    }

    /// Fields the summary reads on the fact model (not the fill model).
    pub fn fields(&self) -> Vec<&str> {
        let mut names = Vec::new();
        for dim in &self.dimensions {
            names.push(dim.field.as_str());
        }
        for (_, measure) in &self.measures {
            match measure {
                Measure::Count => {}
                Measure::Sum(f) | Measure::Avg(f) | Measure::Min(f) | Measure::Max(f) => {
                    names.push(f.as_str())
                }
                Measure::CountIf(filter) => {
                    // Field names are the keys that are not and/or/not. Nested filters are
                    // checked again when the host compiles them.
                    for key in filter.keys() {
                        if key != "and" && key != "or" && key != "not" {
                            names.push(key.as_str());
                        }
                    }
                }
                Measure::AvgDays { from, to } => {
                    names.push(from.as_str());
                    if let AvgDaysTo::Field(field) = to {
                        names.push(field.as_str());
                    }
                }
            }
        }
        names
    }
}

fn parse_dimension(value: &Value) -> Result<Dimension, ReportError> {
    match value {
        Value::String(field) => {
            ident(field, "dimension")?;
            Ok(Dimension { field: field.clone(), grain: None })
        }
        Value::Object(object) => {
            let field = object
                .get("field")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("a dimension needs a field"))?;
            ident(field, "dimension")?;
            let grain = match object.get("grain").and_then(Value::as_str) {
                None => None,
                Some(raw) => Some(Grain::parse(raw)?),
            };
            Ok(Dimension { field: field.to_string(), grain })
        }
        _ => Err(invalid("a dimension is a field name or { field, grain }")),
    }
}

fn parse_measure(name: &str, spec: &Value) -> Result<Measure, ReportError> {
    match spec {
        Value::String(kind) if kind == "count" => Ok(Measure::Count),
        Value::Object(one) if one.len() == 1 => {
            let (kind, body) = one.iter().next().unwrap();
            match kind.as_str() {
                "sum" | "avg" | "min" | "max" => {
                    let field = body
                        .as_str()
                        .ok_or_else(|| invalid(format!("`{name}`: name a field")))?;
                    ident(field, "field")?;
                    Ok(match kind.as_str() {
                        "sum" => Measure::Sum(field.to_string()),
                        "avg" => Measure::Avg(field.to_string()),
                        "min" => Measure::Min(field.to_string()),
                        "max" => Measure::Max(field.to_string()),
                        _ => unreachable!(),
                    })
                }
                "count_if" => {
                    let filter = body
                        .as_object()
                        .cloned()
                        .ok_or_else(|| invalid(format!("`{name}`: count_if takes a filter")))?;
                    Ok(Measure::CountIf(filter))
                }
                "avg_days" => {
                    let object = body
                        .as_object()
                        .ok_or_else(|| invalid(format!("`{name}`: avg_days takes {{ from, to }}")))?;
                    let from = object
                        .get("from")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid(format!("`{name}`: avg_days needs from")))?;
                    ident(from, "field")?;
                    let to_raw = object
                        .get("to")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid(format!("`{name}`: avg_days needs to")))?;
                    let to = if to_raw == "now" {
                        AvgDaysTo::Now
                    } else {
                        ident(to_raw, "field")?;
                        AvgDaysTo::Field(to_raw.to_string())
                    };
                    Ok(Measure::AvgDays { from: from.to_string(), to })
                }
                other => Err(invalid(format!("`{name}`: unknown measure `{other}`"))),
            }
        }
        _ => Err(invalid(format!(
            "`{name}`: use \"count\" or {{\"sum\": \"field\"}}"
        ))),
    }
}

fn parse_fill(value: &Value) -> Result<Fill, ReportError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("fill must be an object"))?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("fill.model is required"))?;
    if model.contains('.') {
        let (plugin, name) = model.split_once('.').unwrap();
        ident(plugin, "plugin")?;
        ident(name, "model")?;
    } else {
        ident(model, "model")?;
    }
    let key = object.get("key").and_then(Value::as_str).unwrap_or("id");
    ident(key, "fill.key")?;
    let label = object
        .get("label")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("fill.label is required"))?;
    ident(label, "fill.label")?;
    let filter = match object.get("filter") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        _ => return Err(invalid("fill.filter must be an object")),
    };
    Ok(Fill {
        model: model.to_string(),
        key: key.to_string(),
        label: label.to_string(),
        filter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_funnel() {
        let query = ReportQuery::parse(&json!({
            "model": "crm_deal.deal",
            "filter": { "pipeline": "crm_pipeline:1" },
            "dimensions": ["stage"],
            "measures": { "n": "count" },
            "fill": { "model": "crm.crm_stage", "key": "id", "label": "name", "filter": {} },
            "preview": true
        }))
        .unwrap();
        assert_eq!(query.model, "crm_deal.deal");
        assert_eq!(query.dimensions.len(), 1);
        assert!(query.preview);
        assert!(query.fill.is_some());
    }

    #[test]
    fn refuses_a_bad_measure() {
        let err = ReportQuery::parse(&json!({
            "model": "deal",
            "measures": { "n": { "median": "amount" } }
        }))
        .unwrap_err();
        assert!(err.to_string().contains("median"));
    }

    #[test]
    fn preview_choices() {
        assert_eq!(normalize_preview_rows(Some(&json!(25))), 25);
        assert_eq!(normalize_preview_rows(Some(&json!(7))), PREVIEW_DEFAULT);
        assert_eq!(normalize_preview_rows(None), PREVIEW_DEFAULT);
    }
}
