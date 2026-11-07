//! What the columns of a file mean: which field each goes to, and how a cell's text becomes the value of
//! that field (a date, a decimal, one of a field's choices ...).
//!
//! A [`Mapping`] is data: it can be guessed from the headers ([`guess`]), shown to a person, corrected and
//! sent back. A cell that cannot be turned into its field's type is a [`CellProblem`] that names the column
//! and says what was expected, and the row it is in is not written.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::data_model::{FieldDef, FieldType, ModelDef};

/// The order of day, month and year in dates written with slashes, dots or dashes. A date in the form
/// `2026-03-01` is always read as year-month-day.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateOrder {
    /// 25/12/2026
    #[default]
    Dmy,
    /// 12/25/2026
    Mdy,
    /// 2026/12/25
    Ymd,
}

/// How numbers and dates are written in the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notation {
    /// `1.234,56` instead of `1,234.56`.
    #[serde(default)]
    pub decimal_comma: bool,
    #[serde(default)]
    pub date_order: DateOrder,
}

/// The meaning of a file's columns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// Header text → field name; `null` leaves a column out.
    #[serde(default)]
    pub columns: BTreeMap<String, Option<String>>,
    /// Fields that identify a record: a row whose values here match an existing record updates it,
    /// otherwise it creates one. Without a key, every row creates a record.
    #[serde(default)]
    pub key: Vec<String>,
    /// Values for fields the file does not give, used when a record is created.
    #[serde(default)]
    pub defaults: Map<String, Value>,
    /// Field → (text in the file, lower-case → value): `{ "active": { "yes": true, "ja": true } }`.
    #[serde(default)]
    pub values: BTreeMap<String, BTreeMap<String, Value>>,
    /// Link field → a field of the model it points at, to find the record by (a name instead of an id).
    #[serde(default)]
    pub link_by: BTreeMap<String, String>,
    #[serde(default)]
    pub notation: Notation,
}

/// Why a cell could not be used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CellProblem {
    pub column: String,
    pub value: String,
    pub message: String,
}

/// Whether a field can be given a value from a file.
pub fn importable(field: &FieldDef) -> bool {
    !field.deprecated
        && field.related.is_none()
        && field.compute.is_none()
        && !matches!(field.kind, FieldType::Many2many | FieldType::Child | FieldType::Image | FieldType::File)
}

/// Letters and digits only, lower case: `"Date of Birth"` and `date_of_birth` are the same.
pub fn simple(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// How a header was matched to a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    Name,
    Label,
    Alias,
    Similar,
}

/// A guessed mapping, and what was and was not matched.
#[derive(Debug, Clone, Serialize)]
pub struct Guess {
    pub mapping: Mapping,
    pub matched: Vec<(String, String, How)>,
    /// Headers that matched no field (left out unless a person maps them).
    pub unmatched_headers: Vec<String>,
    /// Required fields no column and no default fills.
    pub missing_required: Vec<String>,
}

/// Guess a mapping from the headers: a header is matched to the field it names (by name, by label, by an
/// alias the model's `import.json` gives), and failing that to the one field whose name it closely
/// resembles. A field is never given two columns, nor a column two fields.
pub fn guess(headers: &[String], model: &ModelDef, aliases: &BTreeMap<String, Vec<String>>) -> Guess {
    let fields: Vec<&FieldDef> = model.fields.iter().filter(|f| importable(f)).collect();
    let mut taken: Vec<&str> = Vec::new();
    let mut matched = Vec::new();
    let mut columns = BTreeMap::new();
    let mut leftover: Vec<&String> = Vec::new();
    let mut pending: Vec<(&String, String)> = headers.iter().map(|h| (h, simple(h))).collect();

    // Exact names first, then labels, then aliases: the strongest claim wins the field.
    for how in [How::Name, How::Label, How::Alias] {
        let mut next = Vec::new();
        for (header, key) in pending {
            let found = fields.iter().find(|field| {
                !taken.contains(&field.name.as_str())
                    && match how {
                        How::Name => simple(&field.name) == key,
                        How::Label => field.label.as_deref().is_some_and(|l| !l.contains('%') && simple(l) == key),
                        _ => aliases.get(&field.name).is_some_and(|list| list.iter().any(|a| simple(a) == key)),
                    }
            });
            match found {
                Some(field) if !key.is_empty() => {
                    taken.push(&field.name);
                    columns.insert(header.clone(), Some(field.name.clone()));
                    matched.push((header.clone(), field.name.clone(), how));
                }
                _ => next.push((header, key)),
            }
        }
        pending = next;
    }
    // A header that begins or ends with exactly one free field's name (or the reverse) and is long enough.
    for (header, key) in pending {
        let near: Vec<&&FieldDef> = fields
            .iter()
            .filter(|field| {
                let name = simple(&field.name);
                !taken.contains(&field.name.as_str())
                    && name.len() >= 4
                    && key.len() >= 4
                    && (key.starts_with(&name) || key.ends_with(&name) || name.starts_with(&key) || name.ends_with(&key))
            })
            .collect();
        if let [field] = near.as_slice() {
            taken.push(&field.name);
            columns.insert(header.clone(), Some(field.name.clone()));
            matched.push((header.clone(), field.name.clone(), How::Similar));
        } else {
            leftover.push(header);
        }
    }
    let missing_required = fields
        .iter()
        .filter(|f| f.required && f.default.is_none() && f.sequence.is_none() && !taken.contains(&f.name.as_str()))
        .map(|f| f.name.clone())
        .collect();
    Guess {
        mapping: Mapping { columns, ..Mapping::default() },
        matched,
        unmatched_headers: leftover.into_iter().cloned().collect(),
        missing_required,
    }
}

const TRUE_WORDS: &[&str] = &["true", "yes", "y", "1", "x", "on", "t", "ja", "oui", "si", "sí", "sant", "vero", "wahr"];
const FALSE_WORDS: &[&str] = &["false", "no", "n", "0", "off", "f", "nej", "non", "falso", "falskt", "nein", "faux"];

fn digits_only(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// A decimal written the way people write it, as text with exactly `scale` digits after the point.
pub fn decimal(text: &str, scale: u32, comma: bool) -> Result<String, String> {
    let mut text: String = text.chars().filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '_' && *c != '\'').collect();
    let negative_brackets = text.starts_with('(') && text.ends_with(')');
    if negative_brackets {
        text = text[1..text.len() - 1].to_string();
    }
    let (thousands, point) = if comma { ('.', ',') } else { (',', '.') };
    text.retain(|c| c != thousands);
    let text = text.replace(point, ".");
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(&text)),
    };
    let negative = negative != negative_brackets;
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    if (whole.is_empty() && fraction.is_empty())
        || !(whole.is_empty() || digits_only(whole))
        || !(fraction.is_empty() || digits_only(fraction))
    {
        return Err("is not a number".into());
    }
    let fraction = fraction.trim_end_matches('0');
    if fraction.len() > scale as usize {
        return Err(format!("has more than {scale} digits after the point"));
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let mut out = String::new();
    let zero = whole == "0" && fraction.is_empty();
    if negative && !zero {
        out.push('-');
    }
    out.push_str(whole);
    if scale > 0 {
        out.push('.');
        out.push_str(fraction);
        out.push_str(&"0".repeat(scale as usize - fraction.len()));
    }
    Ok(out)
}

/// Days since 1899-12-30, which is how a spreadsheet counts.
fn from_serial(days: f64) -> Option<NaiveDateTime> {
    let base = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_time(NaiveTime::MIN);
    let seconds = (days * 86_400.0).round() as i64;
    base.checked_add_signed(chrono::Duration::seconds(seconds))
}

fn parse_date(text: &str, order: DateOrder) -> Result<NaiveDate, String> {
    let text = text.trim();
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Ok(date);
    }
    if digits_only(text) && text.len() <= 6 {
        return from_serial(text.parse().map_err(|_| "is not a date")?)
            .map(|d| d.date())
            .filter(|d| (1900..=2200).contains(&d.year()))
            .ok_or_else(|| "is not a date".to_string());
    }
    let formats: &[&str] = match order {
        DateOrder::Dmy => &["%d/%m/%Y", "%d.%m.%Y", "%d-%m-%Y"],
        DateOrder::Mdy => &["%m/%d/%Y", "%m.%d.%Y", "%m-%d-%Y"],
        DateOrder::Ymd => &["%Y/%m/%d", "%Y.%m.%d"],
    };
    formats
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(text, format).ok())
        .ok_or_else(|| format!("is not a date (expected 2026-03-01{})", match order {
            DateOrder::Dmy => " or 01/03/2026 as day/month/year",
            DateOrder::Mdy => " or 03/01/2026 as month/day/year",
            DateOrder::Ymd => " or 2026/03/01",
        }))
}

fn parse_datetime(text: &str, order: DateOrder) -> Result<String, String> {
    let text = text.trim();
    if let Ok(moment) = chrono::DateTime::parse_from_rfc3339(text) {
        return Ok(moment.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }
    let numeric = text.replace('.', "");
    if digits_only(&numeric) && text.matches('.').count() <= 1 && text.len() <= 14 && !text.contains('-') {
        if let Some(moment) = text.parse::<f64>().ok().and_then(from_serial) {
            return Ok(moment.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        }
    }
    // A date and a time, split at the first space or T; no time means midnight. The time is taken as UTC.
    let (date_part, time_part) = match text.split_once(['T', ' ']) {
        Some((d, t)) if !t.trim().is_empty() => (d, Some(t.trim())),
        _ => (text, None),
    };
    let date = parse_date(date_part, order).map_err(|_| "is not a date and time (expected 2026-03-01 14:30 or 2026-03-01T14:30:00Z)".to_string())?;
    let time = match time_part {
        None => NaiveTime::MIN,
        Some(t) => ["%H:%M:%S", "%H:%M"]
            .iter()
            .find_map(|format| NaiveTime::parse_from_str(t.trim_end_matches('Z'), format).ok())
            .ok_or_else(|| "has a time that is not like 14:30 or 14:30:00".to_string())?,
    };
    Ok(date.and_time(time).format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// The value of a cell for `field`. `None` for an empty cell (it is simply not given). `map` is the
/// mapping's own translation of texts for this field.
pub fn coerce(
    field: &FieldDef,
    text: &str,
    notation: &Notation,
    map: Option<&BTreeMap<String, Value>>,
) -> Result<Option<Value>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Some(value) = map.and_then(|m| m.get(&trimmed.to_lowercase())) {
        return Ok(Some(value.clone()));
    }
    let value = match field.kind {
        FieldType::String | FieldType::Text => Value::String(trimmed.to_string()),
        FieldType::Int => {
            let cleaned: String = trimmed
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '_' && *c != if notation.decimal_comma { '.' } else { ',' })
                .collect();
            let number: f64 = cleaned.replace(',', ".").parse().map_err(|_| "is not a whole number".to_string())?;
            if number.fract() != 0.0 || number.abs() >= 9e15 {
                return Err("is not a whole number".into());
            }
            Value::from(number as i64)
        }
        FieldType::Float => {
            let cleaned: String = trimmed.chars().filter(|c| !c.is_whitespace() && *c != '\u{a0}').collect();
            let normal = if notation.decimal_comma { cleaned.replace('.', "").replace(',', ".") } else { cleaned.replace(',', "") };
            let number: f64 = normal.parse().map_err(|_| "is not a number".to_string())?;
            serde_json::Number::from_f64(number).map(Value::Number).ok_or("is not a number")?
        }
        FieldType::Decimal => Value::String(decimal(trimmed, field.scale.unwrap_or(2), notation.decimal_comma)?),
        FieldType::Bool => {
            let word = trimmed.to_lowercase();
            if TRUE_WORDS.contains(&word.as_str()) {
                Value::Bool(true)
            } else if FALSE_WORDS.contains(&word.as_str()) {
                Value::Bool(false)
            } else {
                return Err("is not yes or no (use yes, no, true, false, 1 or 0)".into());
            }
        }
        FieldType::Date => Value::String(parse_date(trimmed, notation.date_order)?.format("%Y-%m-%d").to_string()),
        FieldType::Datetime => Value::String(parse_datetime(trimmed, notation.date_order)?),
        FieldType::Select => {
            let wanted = simple(trimmed);
            let found = field
                .options
                .iter()
                .find(|o| o.value == trimmed)
                .or_else(|| field.options.iter().find(|o| o.value.eq_ignore_ascii_case(trimmed)))
                .or_else(|| {
                    field
                        .options
                        .iter()
                        .find(|o| simple(&o.value) == wanted || o.label.as_deref().is_some_and(|l| !l.contains('%') && simple(l) == wanted))
                });
            match found {
                Some(option) => Value::String(option.value.clone()),
                None => {
                    let allowed: Vec<&str> = field.options.iter().take(8).map(|o| o.value.as_str()).collect();
                    return Err(format!("is not one of: {}{}", allowed.join(", "), if field.options.len() > 8 { ", ..." } else { "" }));
                }
            }
        }
        FieldType::Link => {
            match trimmed.split_once(':') {
                Some((table, key)) if !table.is_empty() && !key.is_empty() => Value::String(trimmed.to_string()),
                _ => return Err("is not a record id like `table:key` (map a name column with `link_by` to look records up by name)".into()),
            }
        }
        FieldType::Json => serde_json::from_str(trimmed).map_err(|e| format!("is not valid JSON ({e})"))?,
        FieldType::Many2many | FieldType::Child | FieldType::Image | FieldType::File => {
            return Err("cannot be imported from a file".into());
        }
    };
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model() -> ModelDef {
        serde_json::from_value(json!({
            "name": "lead",
            "fields": [
                { "name": "title", "label": "Deal title", "type": "string", "required": true },
                { "name": "email", "type": "string" },
                { "name": "amount", "type": "decimal", "scale": 2 },
                { "name": "status", "type": "select", "options": [{ "value": "new", "label": "New lead" }, { "value": "won" }, { "value": "lost" }] },
                { "name": "closed_on", "type": "date" },
                { "name": "active", "type": "bool" },
                { "name": "seats", "type": "int" },
                { "name": "score", "type": "float" },
                { "name": "owner", "type": "link", "target": "party" },
                { "name": "total", "type": "decimal", "compute": "amount * seats" },
                { "name": "skills", "type": "many2many", "target": "skill" },
                { "name": "old_note", "type": "string", "deprecated": true }
            ]
        }))
        .expect("a model")
    }

    fn headers(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn field(name: &str) -> FieldDef {
        model().fields.into_iter().find(|f| f.name == name).expect("a field")
    }

    fn cell(name: &str, text: &str) -> Result<Option<Value>, String> {
        coerce(&field(name), text, &Notation::default(), None)
    }

    #[test]
    fn headers_are_matched_by_name_then_label_then_alias() {
        let aliases = BTreeMap::from([("email".to_string(), vec!["E-mail address".to_string()])]);
        let g = guess(&headers(&["Deal Title", "AMOUNT", "E-Mail Address", "closed on", "Closed_On2", "Notes"]), &model(), &aliases);
        let to = |h: &str| g.mapping.columns.get(h).cloned().flatten();
        assert_eq!(to("Deal Title").as_deref(), Some("title"), "by label");
        assert_eq!(to("AMOUNT").as_deref(), Some("amount"), "by name, any case");
        assert_eq!(to("E-Mail Address").as_deref(), Some("email"), "by alias");
        assert_eq!(to("closed on").as_deref(), Some("closed_on"), "spaces and underscores do not matter");
        assert!(g.unmatched_headers.contains(&"Notes".to_string()));
        assert!(g.matched.iter().any(|(h, f, how)| h == "AMOUNT" && f == "amount" && *how == How::Name));
        assert!(g.matched.iter().any(|(h, _, how)| h == "E-Mail Address" && *how == How::Alias));
    }

    #[test]
    fn a_field_gets_one_column_and_what_cannot_be_imported_is_never_offered() {
        let g = guess(&headers(&["amount", "Amount", "total", "skills", "old_note"]), &model(), &BTreeMap::new());
        let amounts = g.mapping.columns.values().filter(|f| f.as_deref() == Some("amount")).count();
        assert_eq!(amounts, 1, "the first column wins; the second is left over");
        assert!(g.unmatched_headers.contains(&"Amount".to_string()));
        for never in ["total", "skills", "old_note"] {
            assert!(!g.mapping.columns.values().any(|f| f.as_deref() == Some(never)), "{never}: calculated, a relation, or retired");
        }
        assert!(importable(&field("title")) && !importable(&field("total")) && !importable(&field("skills")) && !importable(&field("old_note")));
    }

    #[test]
    fn a_header_that_closely_resembles_one_free_field_is_guessed_and_says_so() {
        let g = guess(&headers(&["Customer email", "status code"]), &model(), &BTreeMap::new());
        assert!(g.matched.iter().any(|(h, f, how)| h == "Customer email" && f == "email" && *how == How::Similar));
        assert!(g.matched.iter().any(|(h, f, how)| h == "status code" && f == "status" && *how == How::Similar));
        // Too short to guess from: left for a person.
        let g = guess(&headers(&["id"]), &model(), &BTreeMap::new());
        assert_eq!(g.unmatched_headers, vec!["id"]);
    }

    #[test]
    fn required_fields_nothing_fills_are_reported() {
        let g = guess(&headers(&["email"]), &model(), &BTreeMap::new());
        assert_eq!(g.missing_required, vec!["title"]);
        let g = guess(&headers(&["title"]), &model(), &BTreeMap::new());
        assert!(g.missing_required.is_empty());
    }

    #[test]
    fn decimals_are_read_the_way_people_write_them() {
        assert_eq!(decimal("1234.5", 2, false).as_deref(), Ok("1234.50"));
        assert_eq!(decimal("1,234.56", 2, false).as_deref(), Ok("1234.56"));
        assert_eq!(decimal("1.234,56", 2, true).as_deref(), Ok("1234.56"));
        assert_eq!(decimal("(12.5)", 2, false).as_deref(), Ok("-12.50"));
        assert_eq!(decimal("-0.00", 2, false).as_deref(), Ok("0.00"), "no negative zero");
        assert_eq!(decimal("7", 0, false).as_deref(), Ok("7"));
        assert_eq!(decimal("0.500", 2, false).as_deref(), Ok("0.50"), "trailing zeros are not digits");
        assert_eq!(decimal(" 1 000.25 ", 2, false).as_deref(), Ok("1000.25"));
        assert!(decimal("1.234", 2, false).unwrap_err().contains("more than 2"));
        for bad in ["abc", "", "-", "1.2.3", "1e5", "--1"] {
            assert!(decimal(bad, 2, false).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn each_type_reads_its_cells_or_says_what_it_expected() {
        assert_eq!(cell("title", "  Big deal ").unwrap(), Some(json!("Big deal")));
        assert_eq!(cell("title", "   ").unwrap(), None, "an empty cell is not given");
        assert_eq!(cell("seats", "1,000").unwrap(), Some(json!(1000)));
        assert_eq!(cell("seats", "12.0").unwrap(), Some(json!(12)));
        assert!(cell("seats", "12.5").unwrap_err().contains("whole number"));
        assert_eq!(cell("score", "3.25").unwrap(), Some(json!(3.25)));
        assert_eq!(cell("amount", "10.5").unwrap(), Some(json!("10.50")));
        assert_eq!(cell("active", "Yes").unwrap(), Some(json!(true)));
        assert_eq!(cell("active", "NEJ").unwrap(), Some(json!(false)));
        assert!(cell("active", "maybe").unwrap_err().contains("yes or no"));
        assert_eq!(cell("owner", "party:ann").unwrap(), Some(json!("party:ann")));
        assert!(cell("owner", "Ann").unwrap_err().contains("link_by"));
        assert_eq!(cell("title", "x").unwrap(), Some(json!("x")));
    }

    #[test]
    fn a_choice_matches_by_value_by_case_or_by_label_and_lists_the_options_otherwise() {
        assert_eq!(cell("status", "won").unwrap(), Some(json!("won")));
        assert_eq!(cell("status", "WON").unwrap(), Some(json!("won")));
        assert_eq!(cell("status", "New Lead").unwrap(), Some(json!("new")), "by label");
        let error = cell("status", "pending").unwrap_err();
        assert!(error.contains("new, won, lost"), "{error}");
    }

    #[test]
    fn dates_follow_the_order_the_file_uses() {
        assert_eq!(cell("closed_on", "2026-03-01").unwrap(), Some(json!("2026-03-01")));
        assert_eq!(cell("closed_on", "01/03/2026").unwrap(), Some(json!("2026-03-01")), "day/month by default");
        assert_eq!(cell("closed_on", "25.12.2026").unwrap(), Some(json!("2026-12-25")));
        let us = Notation { date_order: DateOrder::Mdy, ..Notation::default() };
        assert_eq!(coerce(&field("closed_on"), "03/01/2026", &us, None).unwrap(), Some(json!("2026-03-01")));
        assert_eq!(cell("closed_on", "45000").unwrap(), Some(json!("2023-03-15")), "a spreadsheet's date number");
        assert!(cell("closed_on", "31/02/2026").unwrap_err().contains("not a date"));
        assert!(cell("closed_on", "soon").unwrap_err().contains("2026-03-01"));
    }

    #[test]
    fn a_moment_is_read_as_utc_and_a_date_alone_is_midnight() {
        let datetime = FieldDef { kind: FieldType::Datetime, ..field("closed_on") };
        let read = |t: &str| coerce(&datetime, t, &Notation::default(), None).unwrap().unwrap();
        assert_eq!(read("2026-03-01 14:30"), json!("2026-03-01T14:30:00Z"));
        assert_eq!(read("2026-03-01T14:30:15Z"), json!("2026-03-01T14:30:15Z"));
        assert_eq!(read("2026-03-01T16:30:00+02:00"), json!("2026-03-01T14:30:00Z"), "a zone is honoured");
        assert_eq!(read("01/03/2026 09:05"), json!("2026-03-01T09:05:00Z"));
        assert_eq!(read("2026-03-01"), json!("2026-03-01T00:00:00Z"));
        assert!(coerce(&datetime, "2026-03-01 25:99", &Notation::default(), None).unwrap_err().contains("time"));
    }

    #[test]
    fn the_mappings_own_translation_wins() {
        let map = BTreeMap::from([("signed".to_string(), json!("won")), ("tbd".to_string(), json!(false))]);
        assert_eq!(coerce(&field("status"), " Signed ", &Notation::default(), Some(&map)).unwrap(), Some(json!("won")));
        assert_eq!(coerce(&field("active"), "TBD", &Notation::default(), Some(&map)).unwrap(), Some(json!(false)));
        assert!(coerce(&field("status"), "other", &Notation::default(), Some(&map)).is_err(), "what it does not list is read as usual");
    }

    #[test]
    fn a_mapping_is_plain_data_that_round_trips_and_refuses_unknown_keys() {
        let mapping: Mapping = serde_json::from_value(json!({
            "columns": { "Deal title": "title", "Ignore me": null },
            "key": ["email"],
            "defaults": { "status": "new" },
            "notation": { "decimal_comma": true, "date_order": "mdy" }
        }))
        .expect("a mapping");
        assert_eq!(mapping.columns["Ignore me"], None);
        assert!(mapping.notation.decimal_comma && mapping.notation.date_order == DateOrder::Mdy);
        assert_eq!(serde_json::from_value::<Mapping>(serde_json::to_value(&mapping).unwrap()).unwrap(), mapping);
        assert!(serde_json::from_value::<Mapping>(json!({ "colums": {} })).is_err());
    }
}
