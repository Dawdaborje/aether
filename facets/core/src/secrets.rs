//! Encryption of secret settings (API keys, passwords) at rest.
//!
//! A secret setting is stored in the database as `enc:v1:` followed by the base64 of a random
//! nonce and the AES-256-GCM ciphertext. The key never lives in the database, so a leaked
//! database does not leak credentials. It comes from, in order:
//!
//! 1. the `AETHER_SECRET_KEY` environment variable,
//! 2. `secret_key` under `[security]` in `aether.toml`,
//! 3. `<app_dir>/conf/secret.key`, created with a random key (mode 0600) the first time it is
//!    needed.
//!
//! Every process that reads credentials (the HTTP server and any standalone scheduler) must see
//! the same key: share the `app_dir`, or set the same environment variable or config value.
//! Losing the key makes stored secrets unreadable; they have to be entered again.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::RngExt;
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc:v1:";
const NONCE_BYTES: usize = 12;
/// Shortest key text accepted from the environment or `aether.toml`.
pub const MIN_KEY_CHARS: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the secret key must be at least {MIN_KEY_CHARS} characters (use a long random value)")]
    KeyTooShort,
    #[error("could not read or create the secret key file {path}: {source}")]
    KeyFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the secret key is unavailable: {0}")]
    KeyUnavailable(String),
    #[error("that value was not encrypted with this installation's secret key (was the key changed?)")]
    CannotDecrypt,
}

/// Encrypts and decrypts secret setting values.
#[derive(Clone)]
pub struct SecretBox {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBox(..)")
    }
}

impl SecretBox {
    /// From key text of any length of at least [`MIN_KEY_CHARS`].
    pub fn from_key_text(text: &str) -> Result<Self, SecretError> {
        if text.trim().chars().count() < MIN_KEY_CHARS {
            return Err(SecretError::KeyTooShort);
        }
        let key = Sha256::digest(text.trim().as_bytes());
        // A 32-byte key is always accepted by AES-256.
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretError::KeyTooShort)?;
        Ok(Self { cipher })
    }

    /// Find the key the way the module documentation describes, creating the key file if no
    /// other source exists.
    pub fn load(configured: Option<&str>, app_dir: &Path) -> Result<Self, SecretError> {
        if let Ok(text) = std::env::var("AETHER_SECRET_KEY") {
            if !text.trim().is_empty() {
                return Self::from_key_text(&text);
            }
        }
        if let Some(text) = configured.filter(|text| !text.trim().is_empty()) {
            return Self::from_key_text(text);
        }
        let path = app_dir.join("conf").join("secret.key");
        let key_file = |source| SecretError::KeyFile { path: path.clone(), source };
        match std::fs::read_to_string(&path) {
            Ok(text) => return Self::from_key_text(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(key_file(error)),
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(key_file)?;
        }
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        let text: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(text.as_bytes()).map_err(key_file)?;
                log::info!("Created the secret key file {}; keep it with your backups", path.display());
                Self::from_key_text(&text)
            }
            // Another process created it first: use theirs.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::from_key_text(&std::fs::read_to_string(&path).map_err(key_file)?)
            }
            Err(error) => Err(key_file(error)),
        }
    }

    /// `enc:v1:…` for `plaintext`. Each call uses a fresh random nonce.
    pub fn encrypt(&self, plaintext: &str) -> String {
        let mut nonce = [0u8; NONCE_BYTES];
        rand::rng().fill(&mut nonce);
        // Encrypting can only fail for inputs far beyond what a setting holds.
        let sealed = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .unwrap_or_default();
        let mut bytes = nonce.to_vec();
        bytes.extend(sealed);
        format!("{PREFIX}{}", STANDARD.encode(bytes))
    }

    /// The plaintext of a value made by [`encrypt`](Self::encrypt).
    pub fn decrypt(&self, stored: &str) -> Result<String, SecretError> {
        let encoded = stored.strip_prefix(PREFIX).ok_or(SecretError::CannotDecrypt)?;
        let bytes = STANDARD.decode(encoded).map_err(|_| SecretError::CannotDecrypt)?;
        if bytes.len() <= NONCE_BYTES {
            return Err(SecretError::CannotDecrypt);
        }
        let (nonce, sealed) = bytes.split_at(NONCE_BYTES);
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(nonce), sealed)
            .map_err(|_| SecretError::CannotDecrypt)?;
        String::from_utf8(plain).map_err(|_| SecretError::CannotDecrypt)
    }

    /// Whether `stored` looks like something [`encrypt`](Self::encrypt) made.
    pub fn is_encrypted(stored: &str) -> bool {
        stored.starts_with(PREFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret_box() -> SecretBox {
        SecretBox::from_key_text("a long random test key 123456").unwrap()
    }

    #[test]
    fn round_trips_and_never_repeats() {
        let secrets = secret_box();
        let a = secrets.encrypt("sk_live_abc");
        let b = secrets.encrypt("sk_live_abc");
        assert!(SecretBox::is_encrypted(&a));
        assert!(!a.contains("sk_live_abc"));
        assert_ne!(a, b, "a fresh nonce each time");
        assert_eq!(secrets.decrypt(&a).unwrap(), "sk_live_abc");
        assert_eq!(secrets.decrypt(&secrets.encrypt("")).unwrap(), "");
    }

    #[test]
    fn another_key_or_tampering_cannot_decrypt() {
        let sealed = secret_box().encrypt("token");
        let other = SecretBox::from_key_text("a different key, also long enough").unwrap();
        assert!(other.decrypt(&sealed).is_err());
        let mut tampered = sealed.clone();
        tampered.pop();
        tampered.push(if sealed.ends_with('A') { 'B' } else { 'A' });
        assert!(secret_box().decrypt(&tampered).is_err());
        for bad in ["plain text", "enc:v1:", "enc:v1:!!!", "enc:v1:AAAA"] {
            assert!(secret_box().decrypt(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn short_keys_are_refused() {
        assert!(matches!(SecretBox::from_key_text("short"), Err(SecretError::KeyTooShort)));
    }

    #[test]
    fn the_key_file_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        // The environment variable would win over the file; make sure the test sees the file.
        let first = SecretBox::load(None, dir.path());
        if std::env::var("AETHER_SECRET_KEY").is_ok() {
            return;
        }
        let sealed = first.unwrap().encrypt("x");
        let again = SecretBox::load(None, dir.path()).unwrap();
        assert_eq!(again.decrypt(&sealed).unwrap(), "x");
        let file = dir.path().join("conf/secret.key");
        assert!(file.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn config_value_beats_the_key_file() {
        if std::env::var("AETHER_SECRET_KEY").is_ok() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let configured = SecretBox::load(Some("configured key long enough"), dir.path()).unwrap();
        assert!(!dir.path().join("conf/secret.key").exists());
        let same = SecretBox::from_key_text("configured key long enough").unwrap();
        assert_eq!(same.decrypt(&configured.encrypt("v")).unwrap(), "v");
    }
}
