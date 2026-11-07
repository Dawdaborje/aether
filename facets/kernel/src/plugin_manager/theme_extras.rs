//! The smaller token groups of a theme file: `elevation`, `motion`, `focus` and `apps`.
//! Each is checked here and stored under `tokens` as written; the web app turns them into
//! CSS variables (see "Elevation and motion", "Focus" and "Per-app accent" in
//! docs/architecture/themes.md).

use serde_json::Value;

use super::themes::{is_identifier, is_safe_token_value};

/// Steps of the shadow scale (`shadow-sm`, `shadow-lg`, …).
pub const ELEVATION_STEPS: &[&str] = &["2xs", "xs", "sm", "md", "lg", "xl", "2xl"];

/// The motion tokens a theme may set.
pub const MOTION_KEYS: &[&str] = &[
    "duration-short",
    "duration-medium",
    "easing-standard",
    "easing-emphasized",
];

const EASING_WORDS: &[&str] = &["linear", "ease", "ease-in", "ease-out", "ease-in-out"];

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a serde_json::Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{what} must be an object"))
}

/// A shadow list: `none`, or comma-separated `<x> <y> <blur> <spread>? <color>` layers. Only
/// numbers with `px`/`rem`, colours, `inset` and the functions colours use are allowed.
fn is_shadow(value: &str) -> bool {
    if value == "none" {
        return true;
    }
    is_safe_token_value(value)
        && !value.is_empty()
        && !value.contains("url(")
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    ' ' | ',' | '.' | '-' | '+' | '(' | ')' | '/' | '%' | '#' | '_'
                )
        })
}

/// `elevation` is `{ <step>: <shadow list> }`.
pub fn validate_elevation(value: &Value) -> Result<(), String> {
    for (step, shadow) in object(value, "`elevation`")? {
        if !ELEVATION_STEPS.contains(&step.as_str()) {
            return Err(format!(
                "unknown step `{step}`; steps are {}",
                ELEVATION_STEPS.join(", ")
            ));
        }
        if !shadow.as_str().is_some_and(is_shadow) {
            return Err(format!(
                "`{step}` must be `none` or a shadow list such as \"0 1px 3px rgb(0 0 0 / 0.2)\""
            ));
        }
    }
    Ok(())
}

fn is_duration(value: &str) -> bool {
    let (number, scale) = match value.strip_suffix("ms") {
        Some(number) => (number, 1.0),
        None => match value.strip_suffix('s') {
            Some(number) => (number, 1000.0),
            None => return false,
        },
    };
    number.len() <= 6
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
        && number.parse::<f64>().is_ok_and(|n| n * scale <= 2000.0)
}

fn is_easing(value: &str) -> bool {
    if EASING_WORDS.contains(&value) {
        return true;
    }
    let Some(args) = value
        .strip_prefix("cubic-bezier(")
        .and_then(|v| v.strip_suffix(')'))
    else {
        return false;
    };
    let numbers: Vec<&str> = args.split(',').map(str::trim).collect();
    numbers.len() == 4
        && numbers.iter().all(|n| {
            n.len() <= 8
                && !n.is_empty()
                && n.chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
                && n.parse::<f64>().is_ok()
        })
}

/// `motion` is `{ "duration-short"?, "duration-medium"? (up to 2s), "easing-standard"?,
/// "easing-emphasized"? (a keyword or `cubic-bezier(…)`) }`.
pub fn validate_motion(value: &Value) -> Result<(), String> {
    for (key, token) in object(value, "`motion`")? {
        let text = token.as_str().unwrap_or_default();
        let ok = match key.as_str() {
            "duration-short" | "duration-medium" => is_duration(text),
            "easing-standard" | "easing-emphasized" => is_easing(text),
            _ => {
                return Err(format!(
                    "unknown key `{key}`; keys are {}",
                    MOTION_KEYS.join(", ")
                ));
            }
        };
        if !ok {
            return Err(format!(
                "`{key}` must be {}",
                if key.starts_with("duration") {
                    "a time such as \"150ms\" (at most 2s)"
                } else {
                    "a keyword (ease, linear, …) or cubic-bezier(a, b, c, d)"
                }
            ));
        }
    }
    Ok(())
}

fn is_px_or_rem(value: &str, signed: bool) -> bool {
    if value == "0" {
        return true;
    }
    ["px", "rem"].iter().any(|unit| {
        value.strip_suffix(unit).is_some_and(|number| {
            let digits = if signed {
                number.strip_prefix('-').unwrap_or(number)
            } else {
                number
            };
            !digits.is_empty()
                && digits.len() <= 6
                && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
                && digits.parse::<f64>().is_ok_and(|n| n <= 20.0)
        })
    })
}

/// `focus` is `{ "width"?: <px|rem>, "offset"?: <signed px|rem>, "style"?: "ring" | "fill" }`.
/// `fill` paints the focused element with the colour tokens `focus-fill` and `focus-text`.
pub fn validate_focus(value: &Value) -> Result<(), String> {
    for (key, token) in object(value, "`focus`")? {
        let text = token.as_str().unwrap_or_default();
        let ok = match key.as_str() {
            "width" => is_px_or_rem(text, false) && text != "0",
            "offset" => is_px_or_rem(text, true),
            "style" => text == "ring" || text == "fill",
            _ => {
                return Err(format!(
                    "unknown key `{key}`; keys are width, offset, style"
                ));
            }
        };
        if !ok {
            return Err(format!(
                "`{key}` is not valid (width and offset are px or rem, style is `ring` or `fill`)"
            ));
        }
    }
    Ok(())
}

/// `launcher` is `{ "variant"?: "grid" | "list" | "cards", "width"?: "narrow" | "default" | "wide" | "full" }`:
/// how the `/apps` screen lays out the installed apps.
pub fn validate_launcher(value: &Value) -> Result<(), String> {
    for (key, token) in object(value, "`launcher`")? {
        let text = token.as_str().unwrap_or_default();
        let choices: &[&str] = match key.as_str() {
            "variant" => &["grid", "list", "cards"],
            "width" => &["narrow", "default", "wide", "full"],
            _ => return Err(format!("unknown key `{key}`; keys are variant, width")),
        };
        if !choices.contains(&text) {
            return Err(format!("`{key}` must be one of {}", choices.join(", ")));
        }
    }
    Ok(())
}

fn choices(value: &Value, what: &str, keys: &[(&str, &[&str])]) -> Result<(), String> {
    for (key, token) in object(value, what)? {
        let Some((_, allowed)) = keys.iter().find(|(name, _)| name == key) else {
            let names: Vec<&str> = keys.iter().map(|(name, _)| *name).collect();
            return Err(format!("unknown key `{key}`; keys are {}", names.join(", ")));
        };
        if !token.as_str().is_some_and(|text| allowed.contains(&text)) {
            return Err(format!("`{key}` must be one of {}", allowed.join(", ")));
        }
    }
    Ok(())
}

/// `field` is `{ "style": "underline" | "outline" | "filled" }`: how inputs, selects and pickers are drawn.
pub fn validate_field(value: &Value) -> Result<(), String> {
    choices(value, "`field`", &[("style", &["underline", "outline", "filled"])])
}

/// `table` is `{ "header": "filled" | "plain" | "bold", "rows": "lines" | "zebra" | "plain",
/// "density": "compact" | "default" | "relaxed" }`: how lists and tables are drawn.
pub fn validate_table(value: &Value) -> Result<(), String> {
    choices(
        value,
        "`table`",
        &[
            ("header", &["filled", "plain", "bold"]),
            ("rows", &["lines", "zebra", "plain"]),
            ("density", &["compact", "default", "relaxed"]),
        ],
    )
}

/// Colour keys of an `apps` entry.
const APP_KEYS: &[&str] = &["primary", "header", "header_foreground"];

/// `apps` is `{ "<plugin or app route segment>": { "primary"?, "header"?, "header_foreground"? } }`:
/// colours that replace the theme's while that app is open. At most 40 apps.
pub fn validate_apps(value: &Value) -> Result<(), String> {
    let apps = object(value, "`apps`")?;
    if apps.len() > 40 {
        return Err("more than 40 apps".into());
    }
    for (app, colours) in apps {
        if !is_identifier(app)
            && !app
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(format!(
                "`{app}` is not an app name (lowercase letters, digits, `_` and `-`)"
            ));
        }
        for (key, colour) in object(colours, &format!("`apps.{app}`"))? {
            if !APP_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "`apps.{app}`: unknown key `{key}`; keys are {}",
                    APP_KEYS.join(", ")
                ));
            }
            if !colour
                .as_str()
                .is_some_and(|c| !c.is_empty() && is_safe_token_value(c))
            {
                return Err(format!("`apps.{app}.{key}` must be a colour"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn elevation_is_checked() {
        assert!(validate_elevation(&json!({ "sm": "0 1px 2px rgb(0 0 0 / 0.2)", "xl": "none", "md": "inset 0 0 0 1px #fff, 0 2px 4px rgba(0,0,0,.3)" })).is_ok());
        for bad in [
            json!({ "huge": "none" }),
            json!({ "sm": "0 1px 2px red; color: red" }),
            json!({ "sm": "url(x)" }),
            json!({ "sm": 3 }),
            json!({ "sm": "" }),
            json!("shadow"),
        ] {
            assert!(validate_elevation(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn motion_is_checked() {
        assert!(validate_motion(&json!({ "duration-short": "100ms", "duration-medium": "0.25s", "easing-standard": "cubic-bezier(0.2, 0, 0, 1)", "easing-emphasized": "ease-out" })).is_ok());
        for bad in [
            json!({ "duration-long": "1s" }),
            json!({ "duration-short": "5s" }),
            json!({ "duration-short": "fast" }),
            json!({ "easing-standard": "cubic-bezier(1,2,3)" }),
            json!({ "easing-standard": "bounce" }),
        ] {
            assert!(validate_motion(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn field_and_table_are_checked() {
        assert!(validate_field(&json!({ "style": "outline" })).is_ok());
        assert!(validate_table(&json!({ "header": "plain", "rows": "zebra", "density": "compact" })).is_ok());
        for bad in [json!({ "style": "box" }), json!({ "colour": "red" }), json!("outline")] {
            assert!(validate_field(&bad).is_err(), "{bad}");
        }
        assert!(validate_table(&json!({ "rows": "stripes" })).is_err());
    }

    #[test]
    fn launcher_is_checked() {
        assert!(validate_launcher(&json!({ "variant": "cards", "width": "wide" })).is_ok());
        for bad in [json!({ "variant": "wall" }), json!({ "width": 3 }), json!({ "columns": "3" })] {
            assert!(validate_launcher(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn focus_and_apps_are_checked() {
        assert!(
            validate_focus(&json!({ "width": "3px", "offset": "-2px", "style": "fill" })).is_ok()
        );
        for bad in [
            json!({ "width": "0" }),
            json!({ "width": "-2px" }),
            json!({ "offset": "5vw" }),
            json!({ "style": "glow" }),
            json!({ "color": "red" }),
        ] {
            assert!(validate_focus(&bad).is_err(), "{bad}");
        }
        assert!(validate_apps(&json!({ "crm": { "primary": "#0176d3", "header": "#032d60", "header_foreground": "#fff" } })).is_ok());
        for bad in [
            json!({ "CRM": {} }),
            json!({ "crm": { "accent": "#fff" } }),
            json!({ "crm": { "primary": "red; x" } }),
            json!({ "crm": "red" }),
        ] {
            assert!(validate_apps(&bad).is_err(), "{bad}");
        }
    }
}
