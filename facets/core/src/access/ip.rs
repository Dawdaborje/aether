use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use axum::http::HeaderMap;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

use crate::config_manager::models::{AuditConfig, IpStorage};

/// The client address of a request.
///
/// `X-Forwarded-For` is honoured only when the TCP peer is a configured
/// trusted proxy; the list is then read from the right, skipping trusted
/// proxies, so a client cannot choose its own address by sending the header.
pub fn client_ip(headers: &HeaderMap, peer: Option<IpAddr>, trusted: &[IpAddr]) -> Option<IpAddr> {
    let peer = peer?;
    if !trusted.contains(&peer) {
        return Some(peer);
    }
    let forwarded = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|part| part.trim().parse::<IpAddr>().ok())
        .collect::<Vec<_>>();
    Some(
        forwarded
            .into_iter()
            .rev()
            .find(|candidate| !trusted.contains(candidate))
            .unwrap_or(peer),
    )
}

/// How an address is written to audit rows, from `[audit]`.
#[derive(Clone)]
pub struct IpPolicy {
    mode: IpStorage,
    key: Vec<u8>,
}

impl std::fmt::Debug for IpPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpPolicy").field("mode", &self.mode).finish_non_exhaustive()
    }
}

impl IpPolicy {
    pub fn from_config(config: &AuditConfig) -> Self {
        Self {
            mode: config.ip,
            key: config
                .ip_hash_key
                .as_deref()
                .unwrap_or_default()
                .as_bytes()
                .to_vec(),
        }
    }

    pub fn apply(&self, ip: Option<IpAddr>) -> Option<String> {
        let ip = ip?;
        Some(match self.mode {
            IpStorage::Full => ip.to_string(),
            IpStorage::Truncated => truncate(ip).to_string(),
            IpStorage::Hashed => self.hash(ip),
        })
    }

    fn hash(&self, ip: IpAddr) -> String {
        let text = ip.to_string();
        // HMAC accepts keys of any length, so `new_from_slice` cannot fail in
        // practice; the plain keyed SHA-256 fallback exists so no path panics.
        let digest = match Hmac::<Sha256>::new_from_slice(&self.key) {
            Ok(mut mac) => {
                mac.update(text.as_bytes());
                mac.finalize().into_bytes().to_vec()
            }
            Err(_) => Sha256::digest([self.key.as_slice(), text.as_bytes()].concat()).to_vec(),
        };
        let hex: String = digest.iter().take(16).map(|byte| format!("{byte:02x}")).collect();
        format!("h:{hex}")
    }
}

/// IPv4 `/24`, IPv6 `/48`.
fn truncate(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, c, _] = v4.octets();
            IpAddr::V4(Ipv4Addr::new(a, b, c, 0))
        }
        IpAddr::V6(v6) => {
            let [a, b, c, ..] = v6.segments();
            IpAddr::V6(Ipv6Addr::new(a, b, c, 0, 0, 0, 0, 0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(forwarded: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = forwarded.parse() {
            headers.insert("x-forwarded-for", value);
        }
        headers
    }

    fn ip(raw: &str) -> IpAddr {
        raw.parse().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }

    #[test]
    fn ignores_forwarded_for_from_untrusted_peers() {
        let resolved = client_ip(&headers("1.2.3.4"), Some(ip("203.0.113.9")), &[]);
        assert_eq!(resolved, Some(ip("203.0.113.9")));
    }

    #[test]
    fn reads_forwarded_for_through_trusted_proxies() {
        let trusted = [ip("10.0.0.1"), ip("10.0.0.2")];
        // client -> proxy 10.0.0.2 -> proxy 10.0.0.1 -> aether
        let resolved = client_ip(
            &headers("198.51.100.7, 10.0.0.2"),
            Some(ip("10.0.0.1")),
            &trusted,
        );
        assert_eq!(resolved, Some(ip("198.51.100.7")));
    }

    #[test]
    fn a_spoofed_leftmost_entry_does_not_win() {
        let trusted = [ip("10.0.0.1")];
        let resolved = client_ip(
            &headers("6.6.6.6, 198.51.100.7"),
            Some(ip("10.0.0.1")),
            &trusted,
        );
        assert_eq!(resolved, Some(ip("198.51.100.7")));
    }

    #[test]
    fn falls_back_to_the_peer_without_a_usable_header() {
        let trusted = [ip("10.0.0.1")];
        assert_eq!(
            client_ip(&headers("garbage"), Some(ip("10.0.0.1")), &trusted),
            Some(ip("10.0.0.1"))
        );
        assert_eq!(client_ip(&HeaderMap::new(), None, &trusted), None);
    }

    #[test]
    fn storage_modes() {
        let config = |mode, key: Option<&str>| AuditConfig {
            ip: mode,
            ip_hash_key: key.map(str::to_string),
            retention_days: None,
        };
        let address = Some(ip("203.0.113.77"));

        assert_eq!(
            IpPolicy::from_config(&config(IpStorage::Full, None)).apply(address),
            Some("203.0.113.77".into())
        );
        assert_eq!(
            IpPolicy::from_config(&config(IpStorage::Truncated, None)).apply(address),
            Some("203.0.113.0".into())
        );
        assert_eq!(
            IpPolicy::from_config(&config(IpStorage::Truncated, None))
                .apply(Some(ip("2001:db8:abcd:1234::1"))),
            Some("2001:db8:abcd::".into())
        );

        let hashed = IpPolicy::from_config(&config(IpStorage::Hashed, Some("0123456789abcdef")));
        let first = hashed.apply(address);
        assert_eq!(first, hashed.apply(address), "hash is stable");
        assert_ne!(first, hashed.apply(Some(ip("203.0.113.78"))));
        assert!(first.as_deref().is_some_and(|h| h.starts_with("h:") && !h.contains("203")));
        let other_key = IpPolicy::from_config(&config(IpStorage::Hashed, Some("fedcba9876543210")));
        assert_ne!(first, other_key.apply(address), "hash depends on the key");

        assert_eq!(IpPolicy::from_config(&config(IpStorage::Full, None)).apply(None), None);
    }
}
