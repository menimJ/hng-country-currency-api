use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use sqlx::{mysql::MySqlRow, MySql, Row};

use crate::config::AppState;
use crate::models::country::Country;
use crate::services::refresh_service::{refresh_cache, RefreshResult};
use crate::utils::error::ApiError;

#[derive(Deserialize)]
pub struct ListParams {
    pub region: Option<String>,
    pub currency: Option<String>,
    /// Allowed: gdp_desc | gdp_asc | name_asc | population_desc
    pub sort: Option<String>,
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

pub async fn refresh(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let res: RefreshResult = refresh_cache(&state).await?;
    Ok((axum::http::StatusCode::OK, Json(res)))
}

// --- Basic validation using ApiError::Validation(String) ---
fn validate_list_params(p: &ListParams) -> Result<(), ApiError> {
    if let Some(s) = p.sort.as_deref() {
        let ok = matches!(s, "gdp_desc" | "gdp_asc" | "name_asc" | "population_desc");
        if !ok {
            return Err(ApiError::Validation(
                "sort must be one of gdp_desc, gdp_asc, name_asc, population_desc".into(),
            ));
        }
    }
    if let Some(page) = p.page {
        if page < 1 {
            return Err(ApiError::Validation("page must be >= 1".into()));
        }
    }
    if let Some(limit) = p.limit {
        if !(1..=200).contains(&limit) {
            return Err(ApiError::Validation("limit must be between 1 and 200".into()));
        }
    }
    if let Some(curr) = p.currency.as_deref() {
        if curr.len() != 3 {
            return Err(ApiError::Validation(
                "currency must be a 3-letter ISO code (e.g., NGN)".into(),
            ));
        }
    }
    Ok(())
}

pub async fn list_countries(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate query params → 400 if invalid
    validate_list_params(&p)?;

    // Build query dynamically with safe bindings
    let mut qb = sqlx::QueryBuilder::<MySql>::new(
        "SELECT id,name,capital,region,population,currency_code,exchange_rate,estimated_gdp,flag_url,\
         DATE_FORMAT(last_refreshed_at, '%Y-%m-%dT%H:%i:%sZ') as last_refreshed_at \
         FROM countries WHERE 1=1",
    );

    if let Some(r) = p.region.as_deref() {
        qb.push(" AND region = ").push_bind(r);
    }
    if let Some(c) = p.currency.as_deref() {
        qb.push(" AND currency_code = ").push_bind(c);
    }

    let order_clause = match p.sort.as_deref() {
        Some("gdp_desc")        => " ORDER BY estimated_gdp DESC",
        Some("gdp_asc")         => " ORDER BY estimated_gdp ASC",
        Some("name_asc")        => " ORDER BY name ASC",
        Some("population_desc") => " ORDER BY population DESC",
        _                       => " ORDER BY id ASC",
    };
    qb.push(order_clause);

    let page = p.page.unwrap_or(1).max(1);
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * limit;

    qb.push(" LIMIT ").push_bind(limit as i64);
    qb.push(" OFFSET ").push_bind(offset as i64);

    let rows: Vec<MySqlRow> = qb
        .build()
        .fetch_all(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let out: Vec<Country> = rows
        .into_iter()
        .map(|r| Country {
            id: r.try_get::<i64, _>("id").unwrap_or_default(),
            name: r.try_get::<String, _>("name").unwrap_or_default(),
            capital: r.try_get::<Option<String>, _>("capital").ok().flatten(),
            region: r.try_get::<Option<String>, _>("region").ok().flatten(),
            population: r.try_get::<i64, _>("population").unwrap_or_default(),
            currency_code: r.try_get::<Option<String>, _>("currency_code").ok().flatten(),
            exchange_rate: r.try_get::<Option<f64>, _>("exchange_rate").ok().flatten(),
            estimated_gdp: r.try_get::<Option<f64>, _>("estimated_gdp").ok().flatten(),
            flag_url: r.try_get::<Option<String>, _>("flag_url").ok().flatten(),
            last_refreshed_at: r
                .try_get::<Option<String>, _>("last_refreshed_at")
                .ok()
                .flatten(),
        })
        .collect();

    Ok((axum::http::StatusCode::OK, Json(out)))
}

pub async fn get_country(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let row = sqlx::query(
        "SELECT id,name,capital,region,population,currency_code,exchange_rate,estimated_gdp,flag_url,\
         DATE_FORMAT(last_refreshed_at, '%Y-%m-%dT%H:%i:%sZ') as last_refreshed_at \
         FROM countries WHERE LOWER(name)=LOWER(?) LIMIT 1",
    )
    .bind(name)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    let Some(r) = row else {
        return Err(ApiError::NotFound("Country not found".into()));
    };

    let c = Country {
        id: r.try_get::<i64, _>("id").unwrap_or_default(),
        name: r.try_get::<String, _>("name").unwrap_or_default(),
        capital: r.try_get::<Option<String>, _>("capital").ok().flatten(),
        region: r.try_get::<Option<String>, _>("region").ok().flatten(),
        population: r.try_get::<i64, _>("population").unwrap_or_default(),
        currency_code: r.try_get::<Option<String>, _>("currency_code").ok().flatten(),
        exchange_rate: r.try_get::<Option<f64>, _>("exchange_rate").ok().flatten(),
        estimated_gdp: r.try_get::<Option<f64>, _>("estimated_gdp").ok().flatten(),
        flag_url: r.try_get::<Option<String>, _>("flag_url").ok().flatten(),
        last_refreshed_at: r
            .try_get::<Option<String>, _>("last_refreshed_at")
            .ok()
            .flatten(),
    };

    Ok((axum::http::StatusCode::OK, Json(c)))
}

pub async fn delete_country(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let res = sqlx::query("DELETE FROM countries WHERE LOWER(name)=LOWER(?)")
        .bind(name)
        .execute(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    if res.rows_affected() == 0 {
        return Err(ApiError::NotFound("Country not found".into()));
    }

    Ok((axum::http::StatusCode::OK, Json(serde_json::json!({ "ok": true }))))
}

pub async fn status(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM countries")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let ts: Option<(String,)> =
        sqlx::query_as("SELECT v FROM app_meta WHERE k='last_refreshed_at'")
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok((
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
            "total_countries": count.0,
            "last_refreshed_at": ts.map(|x| x.0)
        })),
    ))
}

pub async fn get_image(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let path = &state.summary_image_path;
    if !path.exists() {
        return Err(ApiError::NotFound("Summary image not found".into()));
    }

    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| ApiError::Internal(format!("could not read image: {}", e)))?;

    let resp = Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .body(axum::body::Body::from(bytes))
        .map_err(|e| ApiError::Internal(format!("response build failed: {}", e)))?;

    Ok(resp)
}

// --- Health endpoint: verifies DB connectivity on demand ---
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool).await {
        Ok(_) => (axum::http::StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(e) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "ok": false, "db": e.to_string() })),
        ),
    }
}
