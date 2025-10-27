use reqwest::Client;
use sqlx::{mysql::MySqlPoolOptions, MySql, Pool};
use sqlx::migrate::Migrator;
use std::{env, path::PathBuf};
use tokio::fs;
use tracing::info;

// Embed migrations at compile time from ./migrations (next to Cargo.toml)
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool<MySql>,
    pub http: Client,
    pub summary_image_path: PathBuf,
}

pub struct AppConfig {
    pub port: u16,
    pub database_url: String,
    pub external_timeout_ms: u64,
    pub summary_image_path: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        let port: u16 = env::var("PORT").unwrap_or_else(|_| "8080".into()).parse()?;
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL is required");
        let external_timeout_ms: u64 = env::var("EXTERNAL_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(12_000);
        let summary_image_path =
            PathBuf::from(env::var("SUMMARY_IMAGE_PATH").unwrap_or_else(|_| "cache/summary.png".into()));
        Ok(Self { port, database_url, external_timeout_ms, summary_image_path })
    }

    pub async fn build_state(&self) -> Result<AppState, anyhow::Error> {
        // connect
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
            .connect(&self.database_url)
            .await?;

        // run embedded migrations (creates/uses `sqlx_migrations` table; idempotent)
        MIGRATOR.run(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("migrations failed: {}", e))?;
        info!("✅ Migrations up to date");

        // ping
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("DB connectivity check failed: {}", e))?;
        info!("✅ Database connected");

        // ensure cache dir
        if let Some(parent) = self.summary_image_path.parent() {
            fs::create_dir_all(parent).await.ok();
        }

        // http client
        let http = Client::builder()
            .timeout(std::time::Duration::from_millis(self.external_timeout_ms))
            .build()?;

        Ok(AppState {
            pool,
            http,
            summary_image_path: self.summary_image_path.clone(),
        })
    }
}
