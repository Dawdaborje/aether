//! Locale names and the order in which they are tried.

/// A locale written the standard way: language lowercase, region uppercase, `-` between
/// (`fr_ca` and `FR-ca` both become `fr-CA`). `None` when it does not look like a locale.
pub fn normalize(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 35 {
        return None;
    }
    let mut parts = raw.split(['-', '_']);
    let language = parts.next()?;
    if !(2..=3).contains(&language.len()) || !language.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = language.to_ascii_lowercase();
    for part in parts {
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        out.push('-');
        // A region (`CA`, `419`) is uppercase; a script (`Latn`) is title case.
        if part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()) {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
            }
            out.push_str(&chars.as_str().to_ascii_lowercase());
        } else {
            out.push_str(&part.to_ascii_uppercase());
        }
    }
    Some(out)
}

/// The language part: `fr-CA` is `fr`.
pub fn language(locale: &str) -> &str {
    locale.split('-').next().unwrap_or(locale)
}

/// Locales to try, best first. Each source (the person, the organization, the server, the
/// plugin's own default) contributes itself and then its bare language, so `fr-CA` is
/// followed by `fr`. Sources that are unset or malformed are skipped; repeats are dropped.
pub fn fallback_chain(sources: &[Option<&str>]) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    for source in sources.iter().flatten() {
        let Some(locale) = normalize(source) else { continue };
        let base = language(&locale).to_string();
        for candidate in [locale, base] {
            if !chain.contains(&candidate) {
                chain.push(candidate);
            }
        }
    }
    chain
}

/// The languages most preferred in an `Accept-Language` header, best first
/// (`fr-CA,fr;q=0.8,en;q=0.5`). Entries with `q=0` and `*` are left out.
pub fn parse_accept_language(header: &str) -> Vec<String> {
    let mut entries: Vec<(f32, usize, String)> = Vec::new();
    for (position, part) in header.split(',').enumerate() {
        let mut pieces = part.split(';');
        let Some(tag) = pieces.next().map(str::trim) else { continue };
        if tag == "*" {
            continue;
        }
        let Some(locale) = normalize(tag) else { continue };
        let weight = pieces
            .filter_map(|piece| piece.trim().strip_prefix("q="))
            .find_map(|value| value.trim().parse::<f32>().ok())
            .unwrap_or(1.0);
        if weight > 0.0 {
            entries.push((weight, position, locale));
        }
    }
    entries.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    entries.into_iter().map(|(_, _, locale)| locale).collect()
}

/// Whether the language is written right to left.
pub fn is_rtl(locale: &str) -> bool {
    matches!(language(locale), "ar" | "he" | "fa" | "ur" | "ps" | "sd" | "yi" | "dv" | "ug")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_and_separator() {
        assert_eq!(normalize("fr_ca").as_deref(), Some("fr-CA"));
        assert_eq!(normalize("EN").as_deref(), Some("en"));
        assert_eq!(normalize("zh-hans-cn").as_deref(), Some("zh-Hans-CN"));
        assert_eq!(normalize("es-419").as_deref(), Some("es-419"));
    }

    #[test]
    fn rejects_things_that_are_not_locales() {
        for bad in ["", "e", "english", "fr--CA", "fr-", "../x", "fr CA"] {
            assert_eq!(normalize(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_chain_goes_from_specific_to_general_without_repeats() {
        let chain = fallback_chain(&[Some("fr-CA"), None, Some("fr"), Some("de"), Some("en")]);
        assert_eq!(chain, ["fr-CA", "fr", "de", "en"]);
    }

    #[test]
    fn malformed_sources_are_skipped() {
        assert_eq!(fallback_chain(&[Some("!!"), Some("en")]), ["en"]);
    }

    #[test]
    fn accept_language_is_ordered_by_weight() {
        let list = parse_accept_language("en;q=0.5, fr-CA, fr;q=0.8, *;q=0.1, de;q=0");
        assert_eq!(list, ["fr-CA", "fr", "en"]);
    }

    #[test]
    fn right_to_left_languages() {
        assert!(is_rtl("ar-EG"));
        assert!(is_rtl("he"));
        assert!(!is_rtl("fr"));
    }
}
