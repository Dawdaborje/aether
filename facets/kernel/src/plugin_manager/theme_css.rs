//! The stylesheet a theme plugin may ship (`"css": "theme.css"` in its theme file).
//!
//! It is applied to the whole app, after the stock styles, so a theme can restyle the
//! parts widgets expose (`[data-part="list.row"]`, `[data-part="form.field"]`, …) as well
//! as the stock layout. What it may not do is reach outside the app or run anything, so
//! the file is checked when the plugin loads and stored already checked:
//!
//! * no `@import`: nothing is fetched from elsewhere, and no `image-set()`, `src()` or
//!   `element()` which can fetch too;
//! * `url()` only names files under the plugin's `assets/` folder, and is rewritten to the
//!   address those are served from;
//! * no `expression()`, `behavior`, `-moz-binding`, `javascript:` and no backslash
//!   escapes, which can hide any of these;
//! * only the at-rules a stylesheet needs (`@media`, `@supports`, `@layer`, `@container`,
//!   `@keyframes`, `@font-face`);
//! * at most 64 KB, with balanced braces.
//!
//! Comments are removed, so nothing can hide in one.

use super::themes::is_asset_reference;

pub const MAX_BYTES: usize = 64 * 1024;

const FORBIDDEN: &[&str] = &[
    "@import",
    "@namespace",
    "@charset",
    "expression(",
    "javascript:",
    "vbscript:",
    "behavior:",
    "-moz-binding",
    "image-set(",
    "-webkit-image-set(",
    "src(",
    "element(",
    "</",
    "<!--",
    "\\",
];

const AT_RULES: &[&str] = &[
    "media",
    "supports",
    "layer",
    "container",
    "keyframes",
    "-webkit-keyframes",
    "font-face",
];

/// Check `css` and return it ready to serve for `plugin`: comments removed, `url()`s
/// rewritten to the plugin's asset route.
pub fn sanitize(css: &str, plugin: &str) -> Result<String, String> {
    if css.len() > MAX_BYTES {
        return Err(format!("is larger than {} KB", MAX_BYTES / 1024));
    }
    if css.contains('\0') {
        return Err("contains a NUL character".into());
    }
    let text = strip_comments(css)?;
    let lowered = text.to_ascii_lowercase();
    for token in FORBIDDEN {
        if lowered.contains(token) {
            return Err(format!("may not contain `{token}`"));
        }
    }
    check_braces(&text)?;
    check_at_rules(&lowered)?;
    rewrite_urls(&text, plugin)
}

fn strip_comments(css: &str) -> Result<String, String> {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("*/").ok_or("has a comment that never ends")?;
        // A comment is a space, so `a/**/b` does not become `ab`.
        out.push(' ');
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn check_braces(text: &str) -> Result<(), String> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for ch in text.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '{') => depth += 1,
            (None, '}') => {
                depth -= 1;
                if depth < 0 {
                    return Err("has a `}` with no `{`".into());
                }
            }
            _ => {}
        }
    }
    if quote.is_some() {
        return Err("has a quote that is never closed".into());
    }
    if depth != 0 {
        return Err("has a `{` that is never closed".into());
    }
    Ok(())
}

fn check_at_rules(lowered: &str) -> Result<(), String> {
    let bytes = lowered.as_bytes();
    let mut index = 0;
    while let Some(offset) = lowered[index..].find('@') {
        let start = index + offset + 1;
        let end = bytes[start..]
            .iter()
            .position(|b| !(b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_'))
            .map_or(bytes.len(), |p| start + p);
        let name = &lowered[start..end];
        if !AT_RULES.contains(&name) {
            return Err(format!("may not use `@{name}`"));
        }
        index = end;
    }
    Ok(())
}

/// Replace each `url(...)` with the plugin's asset address, or refuse it.
fn rewrite_urls(text: &str, plugin: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let lowered = rest.to_ascii_lowercase();
        let Some(start) = lowered.find("url(") else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        let end = after.find(')').ok_or("has a `url(` that never closes")?;
        let inner = after[..end].trim();
        let path = inner
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(inner);
        let Some(file) = path
            .strip_prefix("assets/")
            .filter(|_| is_asset_reference(path))
        else {
            return Err(format!(
                "`url({inner})` must be a file under `assets/` of the plugin"
            ));
        };
        out.push_str(&format!("url(\"/api/ui/plugins/{plugin}/assets/{file}\")"));
        rest = &after[end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ordinary_css_and_rewrites_urls() {
        let css = r#"
            /* a theme */
            [data-part="list.row"]:hover { background: color-mix(in oklab, var(--primary) 8%, transparent); }
            @media (max-width: 640px) { [data-part="page.sheet"] { padding: 0.5rem; } }
            @font-face { font-family: "Sea"; src: url("assets/fonts/sea.woff2") format("woff2"); }
            .hero { background: url(assets/img/bg.png); }
        "#;
        let out = sanitize(css, "sea_theme").unwrap();
        assert!(out.contains("[data-part=\"list.row\"]:hover"));
        assert!(!out.contains("a theme"));
        assert!(out.contains("url(\"/api/ui/plugins/sea_theme/assets/fonts/sea.woff2\")"));
        assert!(out.contains("url(\"/api/ui/plugins/sea_theme/assets/img/bg.png\")"));
    }

    #[test]
    fn refuses_what_could_reach_out_or_run() {
        let bad = [
            "@import url('https://evil.example/x.css');",
            "a { background: url(https://evil.example/x.png); }",
            "a { background: url('//evil.example/x.png'); }",
            "a { background: url(data:image/svg+xml;base64,AAAA); }",
            "a { background: url(assets/../secrets.png); }",
            "a { background: image-set('https://evil.example/x.png' 1x); }",
            "a { width: expression(alert(1)); }",
            "a { background: u\\72l(https://evil.example/x); }",
            "a { behavior: url(assets/x.htc); }",
            "a { -moz-binding: url(assets/x.xml#y); }",
            "</style><script>alert(1)</script>",
            "@namespace svg url(http://www.w3.org/2000/svg);",
            "@page { margin: 0 }",
            "a { color: red; ",
            "a { color: red; } }",
            "a { content: \"unclosed }",
            "/* never ends",
        ];
        for css in bad {
            assert!(sanitize(css, "p").is_err(), "{css}");
        }
    }

    #[test]
    fn comments_cannot_hide_anything() {
        // The check runs on the text with comments removed, and a comment is a space.
        // `ur/**/l(` is `ur l(` to a browser: not a `url(`. Either it is refused or what
        // comes out has no `url(` in it.
        match sanitize("a { background: ur/**/l(https://evil.example/x); }", "p") {
            Ok(out) => assert!(!out.to_ascii_lowercase().contains("url(")),
            Err(_) => {}
        }
        assert!(sanitize("@im/**/port 'x';", "p").is_err());
        assert!(sanitize("a { color: /* @import */ red; }", "p").is_ok());
    }

    #[test]
    fn has_a_size_limit() {
        let big = format!("a {{ color: red; }}{}", " ".repeat(MAX_BYTES));
        assert!(sanitize(&big, "p").is_err());
    }
}
