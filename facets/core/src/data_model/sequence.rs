//! Naming series: `INV-{YYYY}-{#####}` and the statements that allocate the next number.
//!
//! The counter lives in the kernel table `aether_sequence`, one row per series and period. It is
//! incremented in the same transaction as the create, so a create that fails gives its number
//! back and numbers are never skipped or handed out twice.

use super::definition::SequenceReset;

/// The table that holds the counters.
pub const SEQUENCE_TABLE: &str = "aether_sequence";

/// Longest pattern, in characters.
const MAX_PATTERN: usize = 48;
/// Most digits in the number part.
const MAX_WIDTH: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Year4,
    Year2,
    Month,
    Number(usize),
}

/// A parsed naming series pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    parts: Vec<Part>,
}

/// A piece of the value, with the date already filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    /// The counter, zero-padded to at least this many digits.
    Number(usize),
}

impl Pattern {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.is_empty() || text.chars().count() > MAX_PATTERN {
            return Err(format!("must be 1 to {MAX_PATTERN} characters"));
        }
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    let mut token = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(c) => token.push(c),
                            None => return Err("has a `{` without a closing `}`".into()),
                        }
                    }
                    if !literal.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut literal)));
                    }
                    parts.push(match token.as_str() {
                        "YYYY" => Part::Year4,
                        "YY" => Part::Year2,
                        "MM" => Part::Month,
                        t if !t.is_empty() && t.chars().all(|c| c == '#') && t.len() <= MAX_WIDTH => Part::Number(t.len()),
                        other => return Err(format!("does not know `{{{other}}}`: use YYYY, YY, MM or #####")),
                    });
                }
                '}' => return Err("has a `}` without an opening `{`".into()),
                c if c.is_ascii_alphanumeric() || " -_/.:".contains(c) => literal.push(c),
                c => return Err(format!("may only contain letters, digits and ` -_/.:` as text, not `{c}`")),
            }
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        if parts.iter().filter(|part| matches!(part, Part::Number(_))).count() != 1 {
            return Err("needs exactly one number part such as `{#####}`".into());
        }
        Ok(Self { parts })
    }

    /// The pieces of the value for a record created in `year`/`month`.
    pub fn segments(&self, year: i32, month: u32) -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        for part in &self.parts {
            let segment = match part {
                Part::Text(text) => Segment::Text(text.clone()),
                Part::Year4 => Segment::Text(format!("{year:04}")),
                Part::Year2 => Segment::Text(format!("{:02}", year.rem_euclid(100))),
                Part::Month => Segment::Text(format!("{month:02}")),
                Part::Number(width) => Segment::Number(*width),
            };
            match (out.last_mut(), segment) {
                (Some(Segment::Text(last)), Segment::Text(next)) => last.push_str(&next),
                (_, segment) => out.push(segment),
            }
        }
        out
    }

    /// The value for counter `n`, for tests and documentation.
    pub fn render(&self, n: i64, year: i32, month: u32) -> String {
        self.segments(year, month)
            .into_iter()
            .map(|segment| match segment {
                Segment::Text(text) => text,
                Segment::Number(width) => format!("{n:0width$}"),
            })
            .collect()
    }
}

/// The counter row for a series: one per model, field and reset period.
pub fn counter_key(table: &str, column: &str, reset: SequenceReset, year: i32, month: u32) -> String {
    match reset {
        SequenceReset::Never => format!("{table}_{column}"),
        SequenceReset::Yearly => format!("{table}_{column}_{year:04}"),
        SequenceReset::Monthly => format!("{table}_{column}_{year:04}{month:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_date_and_a_padded_number() -> Result<(), String> {
        let pattern = Pattern::parse("INV-{YYYY}-{#####}")?;
        assert_eq!(pattern.render(42, 2026, 10), "INV-2026-00042");
        let pattern = Pattern::parse("{YY}{MM}/{###}")?;
        assert_eq!(pattern.render(1234, 2026, 3), "2603/1234");
        Ok(())
    }

    #[test]
    fn adjacent_text_is_joined_so_few_values_are_bound() -> Result<(), String> {
        let segments = Pattern::parse("A-{YYYY}-{MM}-{##}")?.segments(2026, 10);
        assert_eq!(segments, vec![Segment::Text("A-2026-10-".into()), Segment::Number(2)]);
        Ok(())
    }

    #[test]
    fn refuses_a_pattern_without_exactly_one_number() {
        assert!(Pattern::parse("INV-{YYYY}").is_err());
        assert!(Pattern::parse("{##}-{##}").is_err());
        assert!(Pattern::parse("INV-{####").is_err());
        assert!(Pattern::parse("INV-{NOPE}-{##}").is_err());
        assert!(Pattern::parse("it's-{##}").is_err());
    }

    #[test]
    fn a_series_restarts_per_period_only_when_asked() {
        let never = counter_key("mdl_a", "fld_b", SequenceReset::Never, 2026, 10);
        let yearly = counter_key("mdl_a", "fld_b", SequenceReset::Yearly, 2026, 10);
        let monthly = counter_key("mdl_a", "fld_b", SequenceReset::Monthly, 2026, 10);
        assert_eq!((never.as_str(), yearly.as_str(), monthly.as_str()), ("mdl_a_fld_b", "mdl_a_fld_b_2026", "mdl_a_fld_b_202610"));
    }
}
