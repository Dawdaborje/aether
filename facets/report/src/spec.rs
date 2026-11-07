//! `report::query` payload: dimensions, measures, and an optional fill.

use serde_json::{Map, Value};
use thiserror::Error;

/// Rows shown on screen before a download. Organizations may override this.
pub const PREVIEW_DEFAULT: u32 = 10;
/// Most groups one report returns.
pub const MAX_GROUPS: u32 = 10_000;
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
    Quarter,
    Year,
}

impl Grain {
    fn parse(raw: &str) -> Result<Self, ReportError> {
        match raw {
            "day" => Ok(Self::Day),
            "week" => Ok(Self::Week),
            "month" => Ok(Self::Month),
            "quarter" => Ok(Self::Quarter),
            "year" => Ok(Self::Year),
            other => Err(invalid(format!("unknown date grain `{other}`"))),
        }
    }

    /// `time::format` pattern. Fixed strings, never taken from the caller.
    /// Quarter has no strftime pattern; the host derives it from the month
    /// (`%Y-%m` → `YYYY-Qn`), see `is_quarter`.
    pub fn format(self) -> &'static str {
        match self {
            Self::Day => "%Y-%m-%d",
            Self::Week => "%Y-%W",
            Self::Month | Self::Quarter => "%Y-%m",
            Self::Year => "%Y",
        }
    }

    pub fn is_quarter(self) -> bool {
        self == Self::Quarter
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
    /// Sum of `field` over the rows matching `filter` (0 when none do).
    SumIf { field: String, filter: Map<String, Value> },
    /// Average of `field` over the rows matching `filter` (null when none do).
    AvgIf { field: String, filter: Map<String, Value> },
    AvgDays { from: String, to: AvgDaysTo },
}

/// Column derived from measures of the same report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    /// `num / den`; null when `den` is zero.
    Ratio { num: String, den: String },
    /// `a - b`.
    Delta { a: String, b: String },
    /// Row value over the report total, as a fraction.
    Share(String),
    /// The measure in the previous period: the preceding row when rows are ordered by the
    /// first dimension, among rows that agree on the other dimensions.
    Prev(String),
    /// `(current - previous) / previous` against the previous period; null without one.
    Change(String),
}

impl Formula {
    fn parse(name: &str, spec: &Value) -> Result<Self, ReportError> {
        let one = spec
            .as_object()
            .filter(|o| o.len() == 1)
            .ok_or_else(|| invalid(format!("`{name}`: use {{\"ratio\": [\"a\", \"b\"]}}")))?;
        let (kind, body) = one.iter().next().unwrap();
        let pair = || -> Result<(String, String), ReportError> {
            let list = body
                .as_array()
                .filter(|l| l.len() == 2)
                .ok_or_else(|| invalid(format!("`{name}`: {kind} takes two measures")))?;
            let a = list[0].as_str().ok_or_else(|| invalid(format!("`{name}`: name a measure")))?;
            let b = list[1].as_str().ok_or_else(|| invalid(format!("`{name}`: name a measure")))?;
            ident(a, "measure")?;
            ident(b, "measure")?;
            Ok((a.to_string(), b.to_string()))
        };
        match kind.as_str() {
            "ratio" => pair().map(|(num, den)| Self::Ratio { num, den }),
            "delta" => pair().map(|(a, b)| Self::Delta { a, b }),
            "share" => {
                let m = body
                    .as_str()
                    .ok_or_else(|| invalid(format!("`{name}`: share takes a measure")))?;
                ident(m, "measure")?;
                Ok(Self::Share(m.to_string()))
            }
            "prev" | "change" => {
                let m = body
                    .as_str()
                    .ok_or_else(|| invalid(format!("`{name}`: {kind} takes a measure")))?;
                ident(m, "measure")?;
                Ok(if kind == "prev" {
                    Self::Prev(m.to_string())
                } else {
                    Self::Change(m.to_string())
                })
            }
            other => Err(invalid(format!("`{name}`: unknown formula `{other}`"))),
        }
    }

    pub fn sources(&self) -> Vec<&str> {
        match self {
            Self::Ratio { num, den } => vec![num, den],
            Self::Delta { a, b } => vec![a, b],
            Self::Share(m) | Self::Prev(m) | Self::Change(m) => vec![m],
        }
    }

    fn eval(&self, row: &Value, total: &Value) -> Value {
        let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64);
        let out = match self {
            Self::Ratio { num: n, den: d } => match (num(row, n), num(row, d)) {
                (Some(x), Some(y)) if y != 0.0 => Some(x / y),
                _ => None,
            },
            Self::Delta { a, b } => num(row, a).zip(num(row, b)).map(|(x, y)| x - y),
            Self::Share(m) => match (num(row, m), num(total, m)) {
                (Some(x), Some(t)) if t != 0.0 => Some(x / t),
                _ => None,
            },
            // Period-relative formulas need row order; `apply_formulas` fills them in.
            Self::Prev(_) | Self::Change(_) => None,
        };
        out.and_then(serde_json::Number::from_f64)
            .map_or(Value::Null, Value::Number)
    }
}

/// Add formula columns to result rows and to the total row.
pub fn apply_formulas(
    formulas: &[(String, Formula)],
    dimensions: &[Dimension],
    rows: &mut [Value],
    total: &mut Value,
) {
    if formulas.is_empty() {
        return;
    }
    let snapshot = total.clone();
    for row in rows.iter_mut() {
        for (name, formula) in formulas {
            let value = formula.eval(row, &snapshot);
            if let Some(obj) = row.as_object_mut() {
                obj.insert(name.clone(), value);
            }
        }
    }
    // Previous-period formulas: order rows by the first dimension within each combination of
    // the others, and read the neighbour's measure.
    if let Some(first) = dimensions.first() {
        let key = |row: &Value, field: &str| -> String {
            row.get(field).map(Value::to_string).unwrap_or_default()
        };
        let mut order: Vec<usize> = (0..rows.len()).collect();
        let rest: Vec<&str> = dimensions.iter().skip(1).map(|d| d.field.as_str()).collect();
        let others = |row: &Value| -> String {
            rest.iter().map(|f| key(row, f)).collect::<Vec<_>>().join("\u{1}")
        };
        order.sort_by(|a, b| {
            others(&rows[*a])
                .cmp(&others(&rows[*b]))
                .then_with(|| key(&rows[*a], &first.field).cmp(&key(&rows[*b], &first.field)))
        });
        for pair in order.windows(2) {
            let (before, at) = (pair[0], pair[1]);
            if others(&rows[before]) != others(&rows[at]) {
                continue;
            }
            for (name, formula) in formulas {
                let measure = match formula {
                    Formula::Prev(m) | Formula::Change(m) => m,
                    _ => continue,
                };
                let prior = rows[before].get(measure).and_then(Value::as_f64);
                let now = rows[at].get(measure).and_then(Value::as_f64);
                let value = match formula {
                    Formula::Prev(_) => prior,
                    _ => match (now, prior) {
                        (Some(n), Some(p)) if p != 0.0 => Some((n - p) / p),
                        _ => None,
                    },
                };
                if let (Some(v), Some(obj)) = (
                    value.and_then(serde_json::Number::from_f64),
                    rows[at].as_object_mut(),
                ) {
                    obj.insert(name.clone(), Value::Number(v));
                }
            }
        }
    }
    for (name, formula) in formulas {
        let value = formula.eval(&snapshot, &snapshot);
        if let Some(obj) = total.as_object_mut() {
            obj.insert(name.clone(), value);
        }
    }
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
    /// Derived columns computed from measures after the query (not orderable).
    pub formulas: Vec<(String, Formula)>,
    pub fill: Option<Fill>,
    /// `-name` is descending.
    pub order: Option<String>,
    pub limit: u32,
    /// When set, the host returns only the preview slice. Totals stay complete.
    pub preview: bool,
    /// With 2+ dimensions, also return one subtotal set per leading-dimension prefix.
    pub subtotals: bool,
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

        let mut formulas = Vec::new();
        if let Some(raw) = object.get("formulas") {
            let map = raw
                .as_object()
                .ok_or_else(|| invalid("formulas must be an object"))?;
            if map.len() > 16 {
                return Err(invalid("at most 16 formulas"));
            }
            for (name, spec) in map {
                ident(name, "formula")?;
                if measures.iter().any(|(m, _)| m == name) {
                    return Err(invalid(format!("`{name}` is used twice in the result")));
                }
                let formula = Formula::parse(name, spec)?;
                for source in formula.sources() {
                    if !measures.iter().any(|(m, _)| m == source) {
                        return Err(invalid(format!(
                            "`{name}`: `{source}` is not a measure of this report"
                        )));
                    }
                }
                formulas.push((name.clone(), formula));
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
            .min(MAX_GROUPS as u64) as u32;
        if limit == 0 {
            return Err(invalid("limit must be at least 1"));
        }
        let preview = object.get("preview").and_then(Value::as_bool).unwrap_or(false);
        let subtotals = object.get("subtotals").and_then(Value::as_bool).unwrap_or(false);

        Ok(Self {
            model,
            filter,
            dimensions,
            measures,
            formulas,
            fill,
            order,
            limit,
            preview,
            subtotals,
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
                Measure::CountIf(filter) => push_filter_fields(&mut names, filter),
                Measure::SumIf { field, filter } | Measure::AvgIf { field, filter } => {
                    names.push(field.as_str());
                    push_filter_fields(&mut names, filter);
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

/// Field names are the keys that are not and/or/not. Nested filters are checked again when the
/// host compiles them.
fn push_filter_fields<'a>(names: &mut Vec<&'a str>, filter: &'a Map<String, Value>) {
    for key in filter.keys() {
        if key != "and" && key != "or" && key != "not" {
            names.push(key.as_str());
        }
    }
}

/// Half-open `[start, end)` for a named period around `today`.
pub fn period_bounds(
    token: &str,
    today: chrono::NaiveDate,
) -> Result<(chrono::NaiveDate, chrono::NaiveDate), ReportError> {
    use chrono::{Datelike, Duration, NaiveDate};
    let day = |y: i32, m: u32, d: u32| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let tomorrow = today + Duration::days(1);
    let month_start = |y: i32, m0: i32| {
        // `m0` counts months from year 0, so any offset stays valid.
        day(y + m0.div_euclid(12), (m0.rem_euclid(12) + 1) as u32, 1)
    };
    let (y, m) = (today.year(), today.month0() as i32);
    let week_start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let q0 = (m / 3) * 3;
    Ok(match token {
        "today" => (today, tomorrow),
        "yesterday" => (today - Duration::days(1), today),
        "this_week" => (week_start, week_start + Duration::days(7)),
        "last_week" => (week_start - Duration::days(7), week_start),
        "this_month" => (month_start(y, m), month_start(y, m + 1)),
        "last_month" => (month_start(y, m - 1), month_start(y, m)),
        "this_quarter" => (month_start(y, q0), month_start(y, q0 + 3)),
        "last_quarter" => (month_start(y, q0 - 3), month_start(y, q0)),
        "this_year" => (day(y, 1, 1), day(y + 1, 1, 1)),
        "last_year" => (day(y - 1, 1, 1), day(y, 1, 1)),
        "last_7_days" => (today - Duration::days(6), tomorrow),
        "last_30_days" => (today - Duration::days(29), tomorrow),
        "last_90_days" => (today - Duration::days(89), tomorrow),
        other => return Err(invalid(format!("unknown period `{other}`"))),
    })
}

/// Replace `{ field: { "in_period": "this_month" } }` with `{ field: { gte, lt } }`, at any
/// depth of and/or/not. `kind` says whether a field is `date` or `datetime`.
pub fn resolve_periods(
    filter: &mut Map<String, Value>,
    today: chrono::NaiveDate,
    kind: &dyn Fn(&str) -> Option<String>,
) -> Result<(), ReportError> {
    for (key, value) in filter.iter_mut() {
        match key.as_str() {
            "and" | "or" => {
                if let Value::Array(items) = value {
                    for item in items {
                        if let Value::Object(inner) = item {
                            resolve_periods(inner, today, kind)?;
                        }
                    }
                }
            }
            "not" => {
                if let Value::Object(inner) = value {
                    resolve_periods(inner, today, kind)?;
                }
            }
            field => {
                let Value::Object(ops) = value else { continue };
                let Some(token) = ops.get("in_period") else { continue };
                let token = token
                    .as_str()
                    .ok_or_else(|| invalid("in_period takes a period name"))?;
                let (start, end) = period_bounds(token, today)?;
                let text = |d: chrono::NaiveDate| match kind(field).as_deref() {
                    Some("date") => d.format("%Y-%m-%d").to_string(),
                    _ => d.format("%Y-%m-%dT00:00:00Z").to_string(),
                };
                ops.remove("in_period");
                ops.insert("gte".into(), Value::String(text(start)));
                ops.insert("lt".into(), Value::String(text(end)));
            }
        }
    }
    Ok(())
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
                "sum_if" | "avg_if" => {
                    let object = body.as_object().ok_or_else(|| {
                        invalid(format!("`{name}`: {kind} takes {{ field, where }}"))
                    })?;
                    let field = object
                        .get("field")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid(format!("`{name}`: {kind} needs a field")))?;
                    ident(field, "field")?;
                    let filter = match object.get("where") {
                        None | Some(Value::Null) => Map::new(),
                        Some(Value::Object(map)) => map.clone(),
                        _ => return Err(invalid(format!("`{name}`: where is a filter"))),
                    };
                    let field = field.to_string();
                    Ok(if kind.as_str() == "sum_if" {
                        Measure::SumIf { field, filter }
                    } else {
                        Measure::AvgIf { field, filter }
                    })
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
    fn parses_year_and_quarter_grains() {
        let query = ReportQuery::parse(&json!({
            "model": "deal",
            "dimensions": [{ "field": "created_at", "grain": "quarter" }, { "field": "closed_at", "grain": "year" }],
            "measures": { "n": "count" }
        }))
        .unwrap();
        assert_eq!(query.dimensions[0].grain, Some(Grain::Quarter));
        assert_eq!(query.dimensions[1].grain.unwrap().format(), "%Y");
    }

    #[test]
    fn previous_period_and_periods() {
        let query = ReportQuery::parse(&json!({
            "model": "deal",
            "dimensions": [{ "field": "created_at", "grain": "month" }],
            "measures": { "n": "count" },
            "formulas": { "prev_n": { "prev": "n" }, "growth": { "change": "n" } }
        }))
        .unwrap();
        let mut rows = vec![
            json!({ "created_at": "2026-02", "n": 6.0 }),
            json!({ "created_at": "2026-01", "n": 4.0 }),
        ];
        let mut total = json!({ "n": 10.0 });
        apply_formulas(&query.formulas, &query.dimensions, &mut rows, &mut total);
        assert_eq!(rows[0]["prev_n"], json!(4.0));
        assert_eq!(rows[0]["growth"], json!(0.5));
        assert!(rows[1]["prev_n"].is_null());

        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(
            period_bounds("last_month", today).unwrap(),
            (
                chrono::NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
            )
        );
        let mut filter = json!({ "and": [{ "closed_at": { "in_period": "this_quarter" } }] });
        let map = filter.as_object_mut().unwrap();
        resolve_periods(map, today, &|_| Some("date".into())).unwrap();
        let ops = &map["and"][0]["closed_at"];
        assert_eq!(ops["gte"], "2026-01-01");
        assert_eq!(ops["lt"], "2026-04-01");
    }

    #[test]
    fn formulas_are_derived_from_measures() {
        let query = ReportQuery::parse(&json!({
            "model": "deal",
            "measures": { "n": "count", "won": { "count_if": { "status": "won" } } },
            "formulas": { "win_rate": { "ratio": ["won", "n"] }, "share": { "share": "n" } }
        }))
        .unwrap();
        let mut rows = vec![json!({ "n": 4.0, "won": 1.0 }), json!({ "n": 0.0, "won": 0.0 })];
        let mut total = json!({ "n": 4.0, "won": 1.0 });
        apply_formulas(&query.formulas, &query.dimensions, &mut rows, &mut total);
        assert_eq!(rows[0]["win_rate"], json!(0.25));
        assert_eq!(rows[0]["share"], json!(1.0));
        assert!(rows[1]["win_rate"].is_null());
        assert_eq!(total["win_rate"], json!(0.25));
        assert!(ReportQuery::parse(&json!({
            "model": "deal", "measures": { "n": "count" },
            "formulas": { "x": { "ratio": ["n", "nope"] } }
        })).is_err());
    }

    #[test]
    fn conditional_sums_and_averages_name_their_fields() {
        let query = ReportQuery::parse(&json!({
            "model": "deal",
            "measures": {
                "won_value": { "sum_if": { "field": "amount", "where": { "status": "won" } } },
                "won_avg": { "avg_if": { "field": "amount", "where": { "status": "won" } } }
            }
        }))
        .unwrap();
        assert_eq!(
            query.measures.iter().find(|(name, _)| name == "won_value").unwrap().1,
            Measure::SumIf {
                field: "amount".into(),
                filter: json!({ "status": "won" }).as_object().unwrap().clone()
            }
        );
        let fields = query.fields();
        assert!(fields.contains(&"amount") && fields.contains(&"status"));
        assert!(ReportQuery::parse(&json!({
            "model": "deal", "measures": { "x": { "sum_if": { "where": {} } } }
        }))
        .is_err());
    }

    #[test]
    fn preview_choices() {
        assert_eq!(normalize_preview_rows(Some(&json!(25))), 25);
        assert_eq!(normalize_preview_rows(Some(&json!(7))), PREVIEW_DEFAULT);
        assert_eq!(normalize_preview_rows(None), PREVIEW_DEFAULT);
    }
}
