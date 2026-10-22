//! Shared configuration and protocol types for the egress data plane.
//!
//! The data plane is deliberately independent of SU8: consumers only see a
//! standard HTTP CONNECT proxy address.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

pub const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Deserialize)]
pub struct NodeConfig {
    pub listen: SocketAddr,
    pub tls: NodeTlsConfig,
    /// Empty ACL denies every target (fail closed).
    #[serde(default)]
    pub acl: TargetAcl,
    #[serde(default)]
    pub control_plane: Option<NodeControlPlaneConfig>,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeControlPlaneConfig {
    pub url: String,
    pub token: String,
    pub node: String,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterControlPlaneConfig {
    pub url: String,
    pub token: String,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeTlsConfig {
    pub cert: PathBuf,
    pub key: PathBuf,
    /// CA used to verify router client certificates. Required in every mode.
    pub client_ca: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TargetAcl {
    #[serde(default)]
    pub allow: Vec<String>,
}

impl TargetAcl {
    pub fn allows(&self, authority: &str) -> bool {
        let Some((host, port)) = split_authority(authority) else {
            return false;
        };
        self.allow.iter().any(|rule| rule_matches(rule, host, port))
    }
}

fn rule_matches(rule: &str, host: &str, port: u16) -> bool {
    let Some((rule_host, rule_port)) = split_authority(rule) else {
        return false;
    };
    if rule_port != port {
        return false;
    }
    if let Some(suffix) = rule_host.strip_prefix("*.") {
        if suffix.is_empty() {
            return false;
        }
        let suffix = suffix.to_ascii_lowercase();
        let host = host.to_ascii_lowercase();
        host.ends_with(&format!(".{suffix}"))
    } else {
        rule_host.eq_ignore_ascii_case(host)
    }
}

/// Splits `host:port` into host and port. IPv6 authorities use `[addr]:port`.
pub fn split_authority(authority: &str) -> Option<(&str, u16)> {
    let authority = authority.trim();
    if authority.is_empty() {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, rest) = rest.split_once(']')?;
        if host.is_empty() {
            return None;
        }
        let port: u16 = rest.strip_prefix(':')?.parse().ok()?;
        if port == 0 {
            return None;
        }
        return Some((host, port));
    }
    let (host, port) = authority.rsplit_once(':')?;
    if host.is_empty() || host.contains('[') || host.contains(']') {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some((host, port))
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterConfig {
    pub listen: SocketAddr,
    pub line: String,
    pub tls: RouterTlsConfig,
    #[serde(default)]
    pub nodes: Vec<NodeEndpoint>,
    #[serde(default)]
    pub control_plane: Option<RouterControlPlaneConfig>,
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterTlsConfig {
    pub ca: PathBuf,
    pub client_cert: PathBuf,
    pub client_key: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEndpoint {
    pub name: String,
    /// `host:port` used for the TCP connection.
    pub endpoint: String,
    /// Name used for TLS SNI and certificate verification.
    pub server_name: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
}

/// Payload returned by the control plane for a router line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterLineConfig {
    pub version: String,
    pub line: String,
    pub policy: String,
    pub nodes: Vec<NodeEndpoint>,
}

/// Payload returned by the control plane for a node's target ACL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeAclConfig {
    pub version: String,
    pub acl: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthConfig {
    #[serde(default = "default_health_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_health_interval_secs(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
        }
    }
}

fn default_connect_timeout_secs() -> u64 {
    DEFAULT_CONNECT_TIMEOUT_SECS
}

fn default_poll_interval_secs() -> u64 {
    10
}

fn default_weight() -> u32 {
    1
}

fn default_health_interval_secs() -> u64 {
    5
}

fn default_failure_threshold() -> u32 {
    3
}

fn default_success_threshold() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acl(rules: &[&str]) -> TargetAcl {
        TargetAcl {
            allow: rules.iter().map(|value| value.to_string()).collect(),
        }
    }

    #[test]
    fn acl_requires_exact_host_and_port() {
        let acl = acl(&["ark.cn-beijing.volces.com:443"]);
        assert!(acl.allows("ark.cn-beijing.volces.com:443"));
        assert!(acl.allows("ARK.CN-BEIJING.VOLCES.COM:443"));
        assert!(!acl.allows("ark.cn-beijing.volces.com:80"));
        assert!(!acl.allows("evil.cn-beijing.volces.com:443"));
        assert!(!acl.allows("ark.cn-beijing.volces.com"));
        assert!(!acl.allows(""));
    }

    #[test]
    fn acl_wildcard_matches_subdomains_only() {
        let acl = acl(&["*.volces.com:443"]);
        assert!(acl.allows("ark.cn-beijing.volces.com:443"));
        assert!(acl.allows("a.volces.com:443"));
        assert!(!acl.allows("volces.com:443"));
        assert!(!acl.allows("evilvolces.com:443"));
        assert!(!acl.allows("a.volces.com:80"));
    }

    #[test]
    fn empty_acl_denies_everything() {
        assert!(!TargetAcl::default().allows("example.com:443"));
    }

    #[test]
    fn split_authority_handles_ipv6_and_invalid_values() {
        assert_eq!(
            split_authority("example.com:443"),
            Some(("example.com", 443))
        );
        assert_eq!(split_authority("[::1]:443"), Some(("::1", 443)));
        assert_eq!(split_authority("example.com:0"), None);
        assert_eq!(split_authority("example.com"), None);
        assert_eq!(split_authority(":443"), None);
        assert_eq!(split_authority("example.com:not-a-port"), None);
    }
}
