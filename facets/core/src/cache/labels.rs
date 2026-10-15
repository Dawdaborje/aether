//! Colon-hierarchical Mantle key labels.

/// Kind of a structured plugin cache key (`row:`, `list:`, …) or kernel entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheKind {
    Row,
    List,
    Agg,
    Meta,
    Kernel,
    Other,
}

impl CacheKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Row => "row",
            Self::List => "list",
            Self::Agg => "agg",
            Self::Meta => "meta",
            Self::Kernel => "kernel",
            Self::Other => "other",
        }
    }

    /// Higher = keep longer under pressure (`meta` > `row` > `agg` > `list`).
    pub fn dignity(self) -> u8 {
        match self {
            Self::Kernel | Self::Meta => 4,
            Self::Row => 3,
            Self::Agg => 2,
            Self::List => 1,
            Self::Other => 0,
        }
    }
}

/// Parsed dimensions of a fully-qualified Mantle key (`namespace::key`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheLabel {
    pub org: Option<String>,
    pub plugin: Option<String>,
    pub kind: CacheKind,
    pub model: Option<String>,
    pub is_kernel: bool,
}

impl CacheLabel {
    pub fn parse(fq_key: &str) -> Self {
        let (namespace, key) = match fq_key.split_once("::") {
            Some(parts) => parts,
            None => {
                return Self {
                    org: None,
                    plugin: None,
                    kind: CacheKind::Other,
                    model: None,
                    is_kernel: fq_key.starts_with("kernel"),
                };
            }
        };

        if namespace == "kernel" || namespace.starts_with("kernel:") {
            let org = namespace
                .strip_prefix("kernel:")
                .filter(|rest| !rest.is_empty())
                .map(str::to_string);
            // The first segment (`session`, `user`, `plugin`, …) is the kernel entry's family.
            let family = key.split(':').next().filter(|f| !f.is_empty());
            return Self {
                org,
                plugin: None,
                kind: CacheKind::Kernel,
                model: family.map(str::to_string),
                is_kernel: true,
            };
        }

        let (org, plugin) = if let Some(rest) = namespace.strip_prefix("plugin:") {
            let mut parts = rest.splitn(2, ':');
            let org = parts.next().filter(|s| !s.is_empty()).map(str::to_string);
            let plugin = parts.next().filter(|s| !s.is_empty()).map(str::to_string);
            (org, plugin)
        } else {
            (None, None)
        };

        let (kind, model) = parse_user_key(key);
        Self {
            org,
            plugin,
            kind,
            model,
            is_kernel: false,
        }
    }
}

fn parse_user_key(key: &str) -> (CacheKind, Option<String>) {
    let mut parts = key.split(':');
    let head = parts.next().unwrap_or("");
    match head {
        "row" => (CacheKind::Row, parts.next().map(str::to_string)),
        "list" => (CacheKind::List, parts.next().map(str::to_string)),
        "agg" => (CacheKind::Agg, parts.next().map(str::to_string)),
        "meta" => (CacheKind::Meta, None),
        _ => (CacheKind::Other, None),
    }
}

/// Build a structured row key: `row:{model}:{id}`.
pub fn row_key(model: &str, id: &str) -> String {
    format!("row:{model}:{id}")
}

/// Build a structured list key: `list:{model}:{query_hash}`.
pub fn list_key(model: &str, query_hash: &str) -> String {
    format!("list:{model}:{query_hash}")
}

/// Build a structured meta key: `meta:{name}`.
pub fn meta_key(name: &str) -> String {
    format!("meta:{name}")
}

/// Build a structured agg key: `agg:{model}:{name}:{scope_hash}`.
pub fn agg_key(model: &str, name: &str, scope_hash: &str) -> String {
    format!("agg:{model}:{name}:{scope_hash}")
}

pub fn kernel_session_key(token_hash: &str) -> String {
    format!("session:{token_hash}")
}

pub const KERNEL_NAMESPACE: &str = "kernel";

pub fn kernel_org_namespace(org: &str) -> String {
    format!("kernel:{org}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plugin_row() {
        let label = CacheLabel::parse("plugin:acme:crm::row:contact:abc");
        assert_eq!(label.org.as_deref(), Some("acme"));
        assert_eq!(label.plugin.as_deref(), Some("crm"));
        assert_eq!(label.kind, CacheKind::Row);
        assert_eq!(label.model.as_deref(), Some("contact"));
        assert!(!label.is_kernel);
    }

    #[test]
    fn parses_kernel_session() {
        let label = CacheLabel::parse("kernel::session:deadbeef");
        assert!(label.is_kernel);
        assert_eq!(label.kind, CacheKind::Kernel);
        assert_eq!(label.model.as_deref(), Some("session"));
    }
}
