use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

pub async fn connect(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
        .context("connect postgres")
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("run migrations")
}

pub async fn seed(pool: &PgPool) -> Result<()> {
    for (code, name, country) in [
        ("hk", "香港", "HK"),
        ("cn", "中国大陆", "CN"),
        ("usw", "美国西海岸", "US"),
    ] {
        sqlx::query(
            "insert into regions (code, name, country) values ($1, $2, $3)
             on conflict (code) do nothing",
        )
        .bind(code)
        .bind(name)
        .bind(country)
        .execute(pool)
        .await?;
    }

    for (code, name, region) in [
        ("hk", "香港出口", "hk"),
        ("cn", "大陆出口", "cn"),
        ("usw", "美西出口", "usw"),
    ] {
        sqlx::query(
            "insert into lines (code, name, region_id)
             select $1, $2, id from regions where code = $3
             on conflict (code) do nothing",
        )
        .bind(code)
        .bind(name)
        .bind(region)
        .execute(pool)
        .await?;
    }
    Ok(())
}
