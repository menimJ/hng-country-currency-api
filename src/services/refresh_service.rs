use crate::config::AppState;
use crate::types::external::{ErRates, RcCountry};
use crate::utils::error::ApiError;
use crate::utils::image::build_summary_image;
use chrono::Utc;
use rand::Rng;
use std::env;
use tracing::error;

#[derive(serde::Serialize)]
pub struct RefreshResult {
    pub inserted: u64,
    pub updated: u64,
    pub last_refreshed_at: String,
}

pub async fn refresh_cache(state: &AppState) -> Result<RefreshResult, ApiError> {
    // Allow tests / env to override the external endpoints
    let default_countries = "https://restcountries.com/v2/all?fields=name,capital,region,population,flag,currencies".to_string();
    let countries_url = env::var("COUNTRIES_URL").unwrap_or(default_countries);

    let base = env::var("BASE_CURRENCY").unwrap_or_else(|_| "USD".into());
    let default_rates = format!("https://open.er-api.com/v6/latest/{}", base);
    let rates_url = env::var("RATES_URL").unwrap_or(default_rates);

    let countries: Vec<RcCountry> = state
        .http
        .get(&countries_url)
        .send()
        .await
        .map_err(|e| ApiError::External(format!("Could not fetch data from restcountries: {}", e)))?
        .json()
        .await
        .map_err(|e| ApiError::External(format!("Could not parse countries: {}", e)))?;

    let rates_resp: ErRates = state
        .http
        .get(&rates_url)
        .send()
        .await
        .map_err(|e| ApiError::External(format!("Could not fetch data from open-er-api: {}", e)))?
        .json()
        .await
        .map_err(|e| ApiError::External(format!("Could not parse rates: {}", e)))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut inserted = 0u64;
    let mut updated = 0u64;

    for c in countries {
        let name = c.name.trim().to_string();
        let population = c.population.unwrap_or(0);
        let capital = c.capital.map(|s| s.trim().to_string());
        let region = c.region.map(|s| s.trim().to_string());
        let flag_url = c.flag.map(|s| s.trim().to_string());

        let currency_code = c
            .currencies
            .as_ref()
            .and_then(|v| v.first())
            .and_then(|cur| cur.code.as_ref())
            .map(|s| s.trim().to_string());

        let (exchange_rate, estimated_gdp): (Option<f64>, Option<f64>) =
            match currency_code.as_deref() {
                None => (None, Some(0.0)),
                Some(code) => match rates_resp.rates.get(code) {
                    None => (None, None),
                    Some(rate) if *rate > 0.0 => {
                        let mut rng = rand::thread_rng();
                        let multiplier: f64 = rng.gen_range(1000.0..=2000.0);
                        let est = (population as f64 * multiplier) / *rate;
                        (Some(*rate), Some(est))
                    }
                    _ => (None, None),
                },
            };

        let res = sqlx::query(
            r#"
            INSERT INTO countries
                (name, capital, region, population, currency_code, exchange_rate, estimated_gdp, flag_url, last_refreshed_at)
            VALUES
                (?,    ?,       ?,      ?,          ?,             ?,             ?,              ?,        NOW())
            ON DUPLICATE KEY UPDATE
                capital=VALUES(capital),
                region=VALUES(region),
                population=VALUES(population),
                currency_code=VALUES(currency_code),
                exchange_rate=VALUES(exchange_rate),
                estimated_gdp=VALUES(estimated_gdp),
                flag_url=VALUES(flag_url),
                last_refreshed_at=NOW()
            "#,
        )
        .bind(&name)
        .bind(capital)
        .bind(region)
        .bind(population)
        .bind(currency_code)
        .bind(exchange_rate)
        .bind(estimated_gdp)
        .bind(flag_url)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("db upsert failed: {}", e)))?;

        let n = res.rows_affected();
        if n == 1 {
            inserted += 1;
        } else if n == 2 {
            updated += 1;
        }
    }

    let now_iso = Utc::now().to_rfc3339();
    sqlx::query("REPLACE INTO app_meta (k, v) VALUES ('last_refreshed_at', ?)")
        .bind(&now_iso)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("meta update failed: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if let Err(e) = build_summary_image(&state.pool, &state.summary_image_path).await {
        error!("summary image failed: {}", e);
    }

    Ok(RefreshResult {
        inserted,
        updated,
        last_refreshed_at: now_iso,
    })
}
