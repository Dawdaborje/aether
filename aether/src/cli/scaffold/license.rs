use std::time::{SystemTime, UNIX_EPOCH};

use super::ScaffoldError;

pub fn mit_license(year: i64, holder: &str) -> String {
    format!(
        "MIT License

Copyright (c) {year} {holder}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"
    )
}

pub fn current_year() -> Result<i64, ScaffoldError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ScaffoldError::Clock)?
        .as_secs();
    // Seconds since the epoch fit comfortably in i64 for any realistic clock.
    Ok(year_from_unix_seconds(i64::try_from(seconds).unwrap_or(i64::MAX)))
}

/// Gregorian year (UTC) of a Unix timestamp, via Hinnant's `civil_from_days`.
fn year_from_unix_seconds(seconds: i64) -> i64 {
    let z = seconds.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    year_of_era + era * 400 + i64::from(month <= 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_the_utc_year() {
        assert_eq!(year_from_unix_seconds(0), 1970);
        assert_eq!(year_from_unix_seconds(1_767_225_599), 2025);
        assert_eq!(year_from_unix_seconds(1_767_225_600), 2026);
        assert_eq!(year_from_unix_seconds(951_782_400), 2000); // 2000-02-29
    }

    #[test]
    fn license_names_year_and_holder() {
        let text = mit_license(2026, "Ada Lovelace");
        assert!(text.starts_with("MIT License"));
        assert!(text.contains("Copyright (c) 2026 Ada Lovelace"));
    }
}
