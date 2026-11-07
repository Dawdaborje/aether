//! Time-limited TURN credentials for a relay that uses a shared secret (coturn's `use-auth-secret`): the username is
//! `<expiry>:<user>` and the password is the base64 of its HMAC-SHA1 under the secret.

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

use crate::protocol::IceServer;

/// A username and password valid until `now + ttl_secs` (Unix seconds).
pub fn credentials(secret: &str, user: &str, now: u64, ttl_secs: u64) -> (String, String) {
    let username = format!("{}:{}", now + ttl_secs, user);
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(secret.as_bytes()).expect("HMAC takes a key of any size");
    mac.update(username.as_bytes());
    let password = STANDARD.encode(mac.finalize().into_bytes());
    (username, password)
}

/// What a person is given on joining: the relay (when configured) with credentials made for them.
pub fn ice_servers(urls: &[String], secret: Option<&str>, user: &str, now: u64, ttl_secs: u64) -> Vec<IceServer> {
    let urls: Vec<String> = urls.iter().map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect();
    if urls.is_empty() {
        return Vec::new();
    }
    match secret.filter(|s| !s.is_empty()) {
        Some(secret) => {
            let (username, credential) = credentials(secret, user, now, ttl_secs);
            vec![IceServer { urls, username: Some(username), credential: Some(credential) }]
        }
        None => vec![IceServer { urls, username: None, credential: None }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_expire_at_the_ttl_and_name_the_user() {
        let (username, password) = credentials("s3cret", "users:ann", 1_000, 60);
        assert_eq!(username, "1060:users:ann");
        assert_eq!(password, credentials("s3cret", "users:ann", 1_000, 60).1);
        assert_ne!(password, credentials("other", "users:ann", 1_000, 60).1);
        assert_ne!(password, credentials("s3cret", "users:bob", 1_000, 60).1);
    }

    #[test]
    fn a_known_sha1_hmac() {
        // HMAC-SHA1("key", "The quick brown fox jumps over the lazy dog"), the standard test vector.
        let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(b"key").unwrap();
        mac.update(b"The quick brown fox jumps over the lazy dog");
        let hex: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9");
    }

    #[test]
    fn no_relay_means_no_servers() {
        assert!(ice_servers(&[], Some("x"), "ann", 0, 60).is_empty());
        assert!(ice_servers(&["  ".into()], Some("x"), "ann", 0, 60).is_empty());
        let open = ice_servers(&["stun:a:3478".into()], None, "ann", 0, 60);
        assert_eq!(open[0].username, None);
        let signed = ice_servers(&["turn:a:3478".into()], Some("x"), "ann", 0, 60);
        assert!(signed[0].username.is_some() && signed[0].credential.is_some());
    }
}
