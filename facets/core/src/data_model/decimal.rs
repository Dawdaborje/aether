//! Exact decimal numbers for money, pay, days of leave and the like.
//!
//! A `decimal` field has a **scale**, the number of digits after the point (2 for money). The
//! database stores the value as a whole number of the smallest unit (`12.34` with scale 2 is
//! `1234`). That keeps sums, averages, comparisons and ordering exact and done by the database
//! itself, with no floating point anywhere. Plugins send and receive decimals as text
//! (`"12.34"`) so no digit is lost in transit; a JSON number is accepted as input too.

use serde_json::Value;

/// The most digits after the point.
pub const MAX_SCALE: u32 = 9;
/// The scale of a decimal field that does not name one.
pub const DEFAULT_SCALE: u32 = 2;

/// Largest magnitude kept: 18 digits fit in an `i64` with room to add two such numbers.
const MAX_DIGITS: usize = 17;

fn power(scale: u32) -> i64 {
    10_i64.pow(scale)
}

/// The stored whole number for `value` (text or a JSON number) at `scale`. A value with more
/// digits after the point than the scale is refused unless the extra digits are all zero:
/// rounding is the plugin's decision, never the database's.
pub fn to_scaled(value: &Value, scale: u32) -> Result<i64, String> {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return Err("must be a decimal number like 12.34".into()),
    };
    parse(&text, scale)
}

fn parse(text: &str, scale: u32) -> Result<i64, String> {
    let bad = || "must be a decimal number like 12.34".to_string();
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (unsigned, ""),
    };
    if (whole.is_empty() && fraction.is_empty())
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(bad());
    }
    let kept = fraction.get(..(scale as usize).min(fraction.len())).unwrap_or("");
    let extra = fraction.get(kept.len()..).unwrap_or("");
    if extra.bytes().any(|b| b != b'0') {
        return Err(format!("has more than {scale} digit(s) after the point"));
    }
    let whole = whole.trim_start_matches('0');
    let padded = format!("{kept:0<width$}", width = scale as usize);
    let digits = format!("{whole}{padded}");
    if digits.len() > MAX_DIGITS {
        return Err("is too large".into());
    }
    let magnitude: i64 = if digits.is_empty() { 0 } else { digits.parse().map_err(|_| bad())? };
    Ok(if negative { -magnitude } else { magnitude })
}

/// The text form of a stored whole number: always exactly `scale` digits after the point.
pub fn from_scaled(stored: i64, scale: u32) -> String {
    if scale == 0 {
        return stored.to_string();
    }
    let unit = power(scale);
    let sign = if stored < 0 { "-" } else { "" };
    let magnitude = stored.unsigned_abs();
    let (whole, fraction) = (magnitude / unit as u64, magnitude % unit as u64);
    format!("{sign}{whole}.{fraction:0width$}", width = scale as usize)
}

/// A stored value as a plugin sees it. Anything that is not a whole number (an average, for
/// instance) is rounded to the scale first.
pub fn present(stored: &Value, scale: u32) -> Value {
    if let Some(whole) = stored.as_i64() {
        return Value::String(from_scaled(whole, scale));
    }
    match stored.as_f64() {
        Some(number) if number.is_finite() => Value::String(from_scaled(number.round() as i64, scale)),
        _ => stored.clone(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn text_and_numbers_become_whole_units() {
        assert_eq!(to_scaled(&json!("12.34"), 2), Ok(1234));
        assert_eq!(to_scaled(&json!("12.3"), 2), Ok(1230));
        assert_eq!(to_scaled(&json!("12"), 2), Ok(1200));
        assert_eq!(to_scaled(&json!(12.5), 2), Ok(1250));
        assert_eq!(to_scaled(&json!(7), 0), Ok(7));
        assert_eq!(to_scaled(&json!("-0.05"), 2), Ok(-5));
        assert_eq!(to_scaled(&json!(".5"), 2), Ok(50));
        assert_eq!(to_scaled(&json!("12.3400"), 2), Ok(1234));
    }

    #[test]
    fn extra_digits_are_refused_not_rounded() {
        assert!(to_scaled(&json!("12.345"), 2).is_err());
        assert!(to_scaled(&json!("0.1"), 0).is_err());
    }

    #[test]
    fn junk_is_refused() {
        for bad in ["", ".", "-", "1,5", "1.2.3", "abc", "1e5", " "] {
            assert!(to_scaled(&json!(bad), 2).is_err(), "{bad}");
        }
        assert!(to_scaled(&json!(true), 2).is_err());
        assert!(to_scaled(&json!(null), 2).is_err());
        assert!(to_scaled(&json!("123456789012345678"), 2).is_err());
    }

    #[test]
    fn stored_values_print_with_exactly_the_scale() {
        assert_eq!(from_scaled(1234, 2), "12.34");
        assert_eq!(from_scaled(5, 2), "0.05");
        assert_eq!(from_scaled(-5, 2), "-0.05");
        assert_eq!(from_scaled(1200, 2), "12.00");
        assert_eq!(from_scaled(7, 0), "7");
        assert_eq!(from_scaled(0, 3), "0.000");
    }

    #[test]
    fn round_trip_is_exact() {
        for text in ["0.00", "1.01", "99999.99", "-123.45"] {
            let scaled = to_scaled(&json!(text), 2).unwrap_or_default();
            assert_eq!(from_scaled(scaled, 2), text);
        }
    }

    #[test]
    fn averages_are_rounded_to_the_scale_for_display() {
        assert_eq!(present(&json!(1234), 2), json!("12.34"));
        assert_eq!(present(&json!(1234.6), 2), json!("12.35"));
        assert_eq!(present(&json!("x"), 2), json!("x"));
    }
}
