//! Files and images held by records (`image` and `file` fields).
//!
//! A record never holds the bytes. It holds a small object that names a file in the
//! organization's media storage:
//!
//! ```json
//! { "key": "media/2026/10/9f3c…", "name": "receipt.pdf", "mime": "application/pdf",
//!   "size": 48213, "sha256": "…" }
//! ```
//!
//! A field with `multiple` holds a list of these. The upload endpoint
//! (`media_api.rs`) stores the bytes and returns the object; a plugin or page then writes
//! it into the field like any other value.
//!
//! This module is the pure part: what a key looks like, what a stored value must be, and
//! what a file's bytes say it is.

use serde_json::{Map, Value};

/// Folder (inside an organization's media storage) every uploaded file lives under.
pub const MEDIA_FOLDER: &str = "media";

/// Largest file one upload may carry.
pub const MAX_MEDIA_BYTES: usize = 25 * 1024 * 1024;

/// Most files a `multiple` field holds.
pub const MAX_FILES_PER_FIELD: usize = 20;

/// Longest old-style text value (a data URL, a URL, a bare key) an image field accepts, so
/// a record written before the field became an image can be saved again unchanged.
const MAX_LEGACY_TEXT: usize = 700_000;

/// What the bytes of a file say it is. Only these are accepted: everything else (HTML, SVG,
/// scripts, executables) could run in a browser or on a machine when opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Pdf,
    Zip,
    Text,
}

impl Kind {
    pub fn mime(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::Gif => "image/gif",
            Kind::Webp => "image/webp",
            Kind::Pdf => "application/pdf",
            // Office documents (docx, xlsx, pptx) and plain archives.
            Kind::Zip => "application/zip",
            Kind::Text => "text/plain; charset=utf-8",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Kind::Png => "png",
            Kind::Jpeg => "jpg",
            Kind::Gif => "gif",
            Kind::Webp => "webp",
            Kind::Pdf => "pdf",
            Kind::Zip => "zip",
            Kind::Text => "txt",
        }
    }

    pub fn is_image(self) -> bool {
        matches!(self, Kind::Png | Kind::Jpeg | Kind::Gif | Kind::Webp)
    }
}

/// The kind of file `bytes` is, from its content (never from a name or a header the
/// client sent). `None` for anything not allowed.
pub fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Kind::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Kind::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(Kind::Gif);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(Kind::Webp);
    }
    if bytes.starts_with(b"%PDF-") {
        return Some(Kind::Pdf);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(Kind::Zip);
    }
    // Text: valid UTF-8 with no NUL, and not markup a browser would run.
    let head = &bytes[..bytes.len().min(8192)];
    if !bytes.is_empty()
        && std::str::from_utf8(head).is_ok()
        && !head.contains(&0)
        && looks_like_text(head)
    {
        return Some(Kind::Text);
    }
    None
}

fn looks_like_text(head: &[u8]) -> bool {
    // Text has no control characters other than tab and line breaks; a binary header does.
    if head
        .iter()
        .any(|&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r') || b == 0x7f)
    {
        return false;
    }
    let start = String::from_utf8_lossy(head);
    let start = start.trim_start().to_ascii_lowercase();
    !(start.starts_with("<!doctype")
        || start.starts_with("<html")
        || start.starts_with("<svg")
        || start.starts_with("<script")
        || start.starts_with("<?xml"))
}

/// A file name that is safe to show and to put in a `Content-Disposition` header: no path,
/// no control characters or quotes, at most 120 characters. `"file"` when nothing is left.
pub fn clean_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | ':' | '|' | '?' | '*'))
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.');
    let cleaned: String = cleaned.chars().take(120).collect();
    if cleaned.trim().is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

/// `media/2026/10/<32 hex>.<ext>`: unguessable, and the same object never gets two names.
pub fn new_key(year: i32, month: u32, random: [u8; 16], kind: Kind) -> String {
    let mut id = String::with_capacity(32);
    for byte in random {
        id.push_str(&format!("{byte:02x}"));
    }
    format!(
        "{MEDIA_FOLDER}/{year:04}/{month:02}/{id}.{}",
        kind.extension()
    )
}

/// Whether `key` is a key [`new_key`] could have made: under `media/`, plain segments only.
pub fn is_media_key(key: &str) -> bool {
    if key.len() > 120 || !key.starts_with("media/") {
        return false;
    }
    key.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    })
}

/// Check what is being written into an `image` or `file` field.
///
/// `null` clears the field. An object (or, with `multiple`, a list of objects) must name a
/// media key and carry the file's name, type and size. A bare text value is accepted for
/// records written before the field was an image (a data URL or URL), so they can be saved
/// again; the web app shows and replaces it.
pub fn validate_value(value: &Value, multiple: bool, image: bool) -> Result<(), String> {
    match value {
        Value::Null => Ok(()),
        Value::Array(items) if multiple => {
            if items.len() > MAX_FILES_PER_FIELD {
                return Err(format!("holds at most {MAX_FILES_PER_FIELD} files"));
            }
            items.iter().try_for_each(|item| validate_one(item, image))
        }
        Value::Array(_) => Err("holds one file; make the field `multiple` for a list".into()),
        other if multiple && !other.is_string() => Err("must be a list of uploaded files".into()),
        other => validate_one(other, image),
    }
}

fn validate_one(value: &Value, image: bool) -> Result<(), String> {
    match value {
        Value::String(text) => {
            if text.chars().count() <= MAX_LEGACY_TEXT {
                Ok(())
            } else {
                Err("is too long to be a file reference".into())
            }
        }
        Value::Object(object) => validate_object(object, image),
        _ => Err("must be an uploaded file (the object the upload returns)".into()),
    }
}

fn validate_object(object: &Map<String, Value>, image: bool) -> Result<(), String> {
    let key = object
        .get("key")
        .and_then(Value::as_str)
        .ok_or("needs the file's `key`")?;
    if !is_media_key(key) {
        return Err("`key` is not an uploaded file".into());
    }
    let mime = object
        .get("mime")
        .and_then(Value::as_str)
        .ok_or("needs the file's `mime`")?;
    if image && !mime.starts_with("image/") {
        return Err("must be an image".into());
    }
    if !object.get("name").is_some_and(Value::is_string) {
        return Err("needs the file's `name`".into());
    }
    let size = object.get("size").and_then(Value::as_u64);
    if !size.is_some_and(|size| size <= MAX_MEDIA_BYTES as u64) {
        return Err("needs the file's `size`, within the upload limit".into());
    }
    if let Some(sha) = object.get("sha256")
        && !sha
            .as_str()
            .is_some_and(|sha| sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("`sha256` must be 64 hex digits".into());
    }
    for key in object.keys() {
        if !["key", "name", "mime", "size", "sha256"].contains(&key.as_str()) {
            return Err(format!("has an unknown entry `{key}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file() -> Value {
        json!({ "key": "media/2026/10/9f3c0a.pdf", "name": "r.pdf", "mime": "application/pdf", "size": 10 })
    }

    #[test]
    fn sniffs_by_content() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), Some(Kind::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0]), Some(Kind::Jpeg));
        assert_eq!(sniff(b"GIF89a.."), Some(Kind::Gif));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Kind::Webp));
        assert_eq!(sniff(b"%PDF-1.7\n"), Some(Kind::Pdf));
        assert_eq!(sniff(b"PK\x03\x04..."), Some(Kind::Zip));
        assert_eq!(sniff("a,b\n1,2\n".as_bytes()), Some(Kind::Text));
    }

    #[test]
    fn refuses_what_could_run() {
        for bytes in [
            &b"<html><script>alert(1)</script>"[..],
            b"  <!DOCTYPE html><p>",
            b"<svg xmlns='http://www.w3.org/2000/svg' onload='x()'/>",
            b"<?xml version='1.0'?><svg/>",
            b"MZ\x90\x00\x03\x00\x00\x00",
            b"\x7fELF\x02\x01\x01",
            b"",
            b"text with \0 nul",
        ] {
            assert_eq!(sniff(bytes), None, "{:?}", String::from_utf8_lossy(bytes));
        }
    }

    #[test]
    fn cleans_names() {
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name("C:\\Users\\a\\scan.pdf"), "scan.pdf");
        assert_eq!(clean_name("a\"b<c>.png"), "abc.png");
        assert_eq!(clean_name(".htaccess"), "htaccess");
        assert_eq!(clean_name("   "), "file");
        assert_eq!(clean_name(&"x".repeat(300)).chars().count(), 120);
    }

    #[test]
    fn keys_are_unguessable_and_recognised() {
        let key = new_key(2026, 3, [0xAB; 16], Kind::Webp);
        assert_eq!(key, format!("media/2026/03/{}.webp", "ab".repeat(16)));
        assert!(is_media_key(&key));
        for bad in [
            "",
            "media",
            "plugins/x/y",
            "media/../x",
            "media//x",
            "media/a b",
            "/media/x",
            "media/x\\y",
        ] {
            assert!(!is_media_key(bad), "{bad}");
        }
    }

    #[test]
    fn validates_stored_values() {
        assert!(validate_value(&Value::Null, false, false).is_ok());
        assert!(validate_value(&file(), false, false).is_ok());
        assert!(validate_value(&json!([file(), file()]), true, false).is_ok());
        // A data URL from before the field was an image.
        assert!(validate_value(&json!("data:image/png;base64,AAAA"), false, true).is_ok());

        let bad = [
            (json!([file()]), false, false),
            (json!(42), false, false),
            (
                json!({ "key": "plugins/x/a.pdf", "name": "a", "mime": "a/b", "size": 1 }),
                false,
                false,
            ),
            (
                json!({ "key": "media/a.pdf", "mime": "a/b", "size": 1 }),
                false,
                false,
            ),
            (
                json!({ "key": "media/a.pdf", "name": "a", "mime": "a/b", "size": 1 }),
                false,
                true,
            ),
            (
                json!({ "key": "media/a.pdf", "name": "a", "mime": "a/b", "size": 99999999999u64 }),
                false,
                false,
            ),
            (
                json!({ "key": "media/a.pdf", "name": "a", "mime": "a/b", "size": 1, "url": "x" }),
                false,
                false,
            ),
            (
                json!({ "key": "media/a.pdf", "name": "a", "mime": "a/b", "size": 1, "sha256": "zz" }),
                false,
                false,
            ),
        ];
        for (value, multiple, image) in bad {
            assert!(validate_value(&value, multiple, image).is_err(), "{value}");
        }
        let many: Vec<Value> = (0..21).map(|_| file()).collect();
        assert!(validate_value(&Value::Array(many), true, false).is_err());
    }
}
