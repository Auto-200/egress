use crate::error::AppError;
use egress_proto::split_authority;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::BTreeSet;

pub async fn audit(
    pool: &PgPool,
    actor: &str,
    action: &str,
    object: &str,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("insert into audit_events (actor, action, object, detail) values ($1, $2, $3, $4)")
        .bind(actor)
        .bind(action)
        .bind(object)
        .bind(detail)
        .execute(pool)
        .await?;
    Ok(())
}

pub fn validate_code(value: &str, field: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(AppError::bad_request(format!("{field} 长度需在 1-64")));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(AppError::bad_request(format!(
            "{field} 只能包含小写字母、数字、-、_"
        )));
    }
    Ok(value.to_string())
}

pub fn validate_name(value: &str, field: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err(AppError::bad_request(format!("{field} 长度需在 1-128")));
    }
    Ok(value.to_string())
}

pub fn validate_endpoint(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if split_authority(value).is_none() {
        return Err(AppError::bad_request("endpoint 必须是 host:port"));
    }
    Ok(value.to_string())
}

pub fn validate_server_name(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 253
        || value.contains(':')
        || value.contains('/')
        || value.contains(' ')
    {
        return Err(AppError::bad_request("server_name 必须是合法的主机名"));
    }
    Ok(value.to_string())
}

pub fn validate_acl(values: &[String]) -> Result<Vec<String>, AppError> {
    if values.len() > 256 {
        return Err(AppError::bad_request("target_acl 最多 256 条"));
    }
    let mut unique = BTreeSet::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if split_authority(value).is_none() {
            return Err(AppError::bad_request(format!(
                "ACL 条目必须是 host:port：{value}"
            )));
        }
        unique.insert(value.to_string());
    }
    Ok(unique.into_iter().collect())
}

pub fn validate_status<'a>(value: &'a str, allowed: &[&str]) -> Result<&'a str, AppError> {
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(AppError::bad_request(format!(
            "status 只能是 {}",
            allowed.join(" / ")
        )))
    }
}

pub fn validate_weight(value: i32) -> Result<i32, AppError> {
    if (1..=1000).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::bad_request("weight 需在 1-1000"))
    }
}

pub fn validate_capacity(value: i32) -> Result<i32, AppError> {
    if (0..=1_000_000).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::bad_request("capacity 需在 0-1000000"))
    }
}
