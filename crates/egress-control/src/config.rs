use anyhow::{anyhow, bail, Context, Result};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub admin_user: String,
    pub admin_password: Option<String>,
    pub admin_password_hash: Option<String>,
    pub internal_token: String,
    pub cookie_secure: bool,
    pub session_ttl_secs: i64,
    pub web_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind = std::env::var("EGRESS_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8088".to_string())
            .parse()
            .context("EGRESS_BIND")?;
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL")?;
        let admin_user = std::env::var("EGRESS_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
        let admin_password = std::env::var("EGRESS_ADMIN_PASSWORD").ok();
        let admin_password_hash = std::env::var("EGRESS_ADMIN_PASSWORD_HASH").ok();
        if admin_password.is_none() && admin_password_hash.is_none() {
            return Err(anyhow!(
                "EGRESS_ADMIN_PASSWORD or EGRESS_ADMIN_PASSWORD_HASH is required"
            ));
        }
        if let Some(password) = admin_password.as_deref() {
            if password.len() < 8 {
                bail!("EGRESS_ADMIN_PASSWORD must be at least 8 characters");
            }
        }
        let internal_token =
            std::env::var("EGRESS_INTERNAL_TOKEN").context("EGRESS_INTERNAL_TOKEN")?;
        if internal_token.len() < 16 {
            bail!("EGRESS_INTERNAL_TOKEN must be at least 16 characters");
        }
        let cookie_secure = parse_bool("EGRESS_COOKIE_SECURE", false)?;
        let session_ttl_secs = std::env::var("EGRESS_SESSION_TTL_SECS")
            .ok()
            .map(|value| value.parse::<i64>())
            .transpose()
            .context("EGRESS_SESSION_TTL_SECS")?
            .unwrap_or(43200);
        if session_ttl_secs <= 0 {
            bail!("EGRESS_SESSION_TTL_SECS must be positive");
        }
        let web_dir = PathBuf::from(
            std::env::var("EGRESS_WEB_DIR").unwrap_or_else(|_| "web/dist".to_string()),
        );
        Ok(Self {
            bind,
            database_url,
            admin_user,
            admin_password,
            admin_password_hash,
            internal_token,
            cookie_secure,
            session_ttl_secs,
            web_dir,
        })
    }
}

fn parse_bool(name: &str, default: bool) -> Result<bool> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
            "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
            _ => bail!("{name} must be 0/1 or true/false"),
        },
        Err(_) => Ok(default),
    }
}
