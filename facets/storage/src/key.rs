use std::fmt;

use crate::StorageError;

const MAX_KEY_BYTES: usize = 1024;

/// A validated, backend-independent object key such as `invoices/2026/a.pdf`.
///
/// Keys are `/`-separated relative paths. Empty segments, `.`/`..` segments,
/// backslashes and control characters are rejected so a key can never escape
/// the backend's root, whatever the backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaKey(String);

impl MediaKey {
    pub fn parse(raw: &str) -> Result<Self, StorageError> {
        let invalid = |reason| StorageError::InvalidKey {
            key: raw.to_string(),
            reason,
        };
        if raw.is_empty() {
            return Err(invalid("key is empty"));
        }
        if raw.len() > MAX_KEY_BYTES {
            return Err(invalid("key is longer than 1024 bytes"));
        }
        if raw.contains('\\') || raw.chars().any(char::is_control) {
            return Err(invalid("key contains a backslash or control character"));
        }
        for segment in raw.split('/') {
            match segment {
                "" => return Err(invalid("key has an empty path segment")),
                "." | ".." => return Err(invalid("key has a `.` or `..` path segment")),
                _ => {}
            }
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MediaKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for MediaKey {
    type Err = StorageError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_nested_keys() {
        assert!(MediaKey::parse("invoices/2026/a.pdf").is_ok());
        assert!(MediaKey::parse("a").is_ok());
    }

    #[test]
    fn rejects_unsafe_keys() {
        for raw in [
            "",
            "/abs",
            "trailing/",
            "a//b",
            "../x",
            "a/../b",
            "./a",
            "a\\b",
            "a\0b",
        ] {
            assert!(MediaKey::parse(raw).is_err(), "`{raw:?}` should be rejected");
        }
    }
}
