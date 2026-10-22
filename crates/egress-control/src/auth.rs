use crate::config::Config;
use crate::error::AppError;
use crate::models::{LoginRequest, UserResponse};
use crate::AppState;
use anyhow::{anyhow, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chrono::{Duration as ChronoDuration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tracing::warn;

pub const SESSION_COOKIE: &str = "egress_session";

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub username: String,
}

pub fn admin_password_hash(cfg: &Config) -> Result<String> {
    if let Some(hash) = &cfg.admin_password_hash {
        PasswordHash::new(hash)
            .map_err(|err| anyhow!("EGRESS_ADMIN_PASSWORD_HASH is not a valid PHC hash: {err}"))?;
        return Ok(hash.clone());
    }
    let password = cfg
        .admin_password
        .as_deref()
        .ok_or_else(|| anyhow!("missing admin password"))?;
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| anyhow!("hash admin password: {err}"))?
        .to_string())
}

pub async fn require_admin(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let Some(token) = session_token(req.headers()) else {
        return Err(AppError::unauthorized());
    };
    let token_hash = hash_token(&token);
    let exists: Option<(String,)> = sqlx::query_as(
        "select token_hash from admin_sessions where token_hash = $1 and expires_at > now()",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?;
    if exists.is_none() {
        return Err(AppError::unauthorized());
    }
    req.extensions_mut().insert(AuthUser {
        username: state.cfg.admin_user.clone(),
    });
    Ok(next.run(req).await)
}

pub async fn require_internal(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if token.is_empty() || token != state.cfg.internal_token {
        return Err(AppError::forbidden("invalid internal token"));
    }
    Ok(next.run(req).await)
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let username_ok = input.username == state.cfg.admin_user;
    let password_ok = if username_ok {
        let parsed = PasswordHash::new(&state.admin_hash).map_err(AppError::internal)?;
        Argon2::default()
            .verify_password(input.password.as_bytes(), &parsed)
            .is_ok()
    } else {
        false
    };
    if !username_ok || !password_ok {
        tokio::time::sleep(Duration::from_millis(300)).await;
        return Err(AppError::unauthorized());
    }

    let token = new_token();
    let token_hash = hash_token(&token);
    let expires_at = Utc::now() + ChronoDuration::seconds(state.cfg.session_ttl_secs);
    let mut tx = state.db.begin().await?;
    sqlx::query("delete from admin_sessions where expires_at < now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("insert into admin_sessions (token_hash, expires_at) values ($1, $2)")
        .bind(&token_hash)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let mut response = Json(UserResponse {
        user: state.cfg.admin_user.clone(),
    })
    .into_response();
    let cookie = cookie_header(&token, state.cfg.session_ttl_secs, state.cfg.cookie_secure);
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(AppError::internal)?,
    );
    Ok(response)
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(token) = session_token(&headers) {
        sqlx::query("delete from admin_sessions where token_hash = $1")
            .bind(hash_token(&token))
            .execute(&state.db)
            .await?;
    }
    let mut response = Json(UserResponse {
        user: String::new(),
    })
    .into_response();
    let cookie = clear_cookie(state.cfg.cookie_secure);
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(AppError::internal)?,
    );
    Ok(response)
}

pub async fn me(Extension(user): Extension<AuthUser>) -> Json<UserResponse> {
    Json(UserResponse {
        user: user.username,
    })
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name == SESSION_COOKIE && !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn cookie_header(token: &str, max_age: i64, secure: bool) -> String {
    let mut cookie =
        format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn clear_cookie(secure: bool) -> String {
    let mut cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

pub fn spawn_session_cleanup(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(err) = sqlx::query("delete from admin_sessions where expires_at < now()")
                .execute(&state.db)
                .await
            {
                warn!(error = %err, "session cleanup failed");
            }
        }
    });
}
