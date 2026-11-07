use std::time::{SystemTime, UNIX_EPOCH};

use super::ScaffoldError;

/// Business Source License 1.1 (<https://mariadb.com/bsl11/>) with the usual
/// parameters filled in: production use needs a commercial license from the
/// author, and the work converts to Apache-2.0 four years after `year`.
pub fn bsl_license(year: i64, holder: &str) -> String {
    let change_year = year + 4;
    format!(
        "Business Source License 1.1

Parameters

Licensor:             {holder}
Licensed Work:        This plugin. The Licensed Work is (c) {year} {holder}.
Additional Use Grant: You may make use of the Licensed Work for non-production
                      purposes only (development, testing, evaluation and
                      personal use). Any production or commercial use requires
                      a separate commercial license from the Licensor.
Change Date:          {change_year}-01-01
Change License:       Apache License, Version 2.0

For information about alternative licensing arrangements for the Licensed Work,
please contact the Licensor.

Notice

The Business Source License (this document, or the \"License\") is not an Open
Source license. However, the Licensed Work will eventually be made available
under an Open Source License, as stated in this License.

License text copyright (c) 2017 MariaDB Corporation Ab, All Rights Reserved.
\"Business Source License\" is a trademark of MariaDB Corporation Ab.

-----------------------------------------------------------------------------

Business Source License 1.1

Terms

The Licensor hereby grants you the right to copy, modify, create derivative
works, redistribute, and make non-production use of the Licensed Work. The
Licensor may make an Additional Use Grant, above, permitting limited production
use.

Effective on the Change Date, or the fourth anniversary of the first publicly
available distribution of a specific version of the Licensed Work under this
License, whichever comes first, the Licensor hereby grants you rights under the
terms of the Change License, and the rights granted in the paragraph above
terminate.

If your use of the Licensed Work does not comply with the requirements
currently in effect as described in this License, you must purchase a
commercial license from the Licensor, its affiliated entities, or authorized
resellers, or you must refrain from using the Licensed Work.

All copies of the original and modified Licensed Work, and derivative works of
the Licensed Work, are subject to this License. This License applies separately
for each version of the Licensed Work and the Change Date may vary for each
version of the Licensed Work released by Licensor.

You must conspicuously display this License on each original or modified copy
of the Licensed Work. If you receive the Licensed Work in original or modified
form from a third party, the terms and conditions set forth in this License
apply to your use of that work.

Any use of the Licensed Work in violation of this License will automatically
terminate your rights under this License for the current and all other versions
of the Licensed Work.

This License does not grant you any right in any trademark or logo of Licensor
or its affiliates (provided that you may use a trademark or logo of Licensor as
expressly required by this License).

TO THE EXTENT PERMITTED BY APPLICABLE LAW, THE LICENSED WORK IS PROVIDED ON AN
\"AS IS\" BASIS. LICENSOR HEREBY DISCLAIMS ALL WARRANTIES AND CONDITIONS,
EXPRESS OR IMPLIED, INCLUDING (WITHOUT LIMITATION) WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, NON-INFRINGEMENT, AND
TITLE.
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
        let text = bsl_license(2026, "Ada Lovelace");
        assert!(text.starts_with("Business Source License 1.1"));
        assert!(text.contains("Licensor:             Ada Lovelace"));
        assert!(text.contains("Change Date:          2030-01-01"));
    }
}
