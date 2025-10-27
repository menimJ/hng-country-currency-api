use std::{env, path::PathBuf, time::Duration};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serial_test::serial;
use sqlx::{mysql::MySqlPoolOptions, MySql, Pool};
use tempfile::TempDir;
use testcontainers::{clients::Cli, images::generic::GenericImage, Container, RunnableImage};
use tokio::time::sleep;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};
use wiremock::{MockServer, Request as WmRequest};

#[allow(dead_code)]
struct TestCtx<'a> {
    _mysql: Container<'a, GenericImage>,
    db_url: String,
    pool: Pool<MySql>,
    mock: MockServer,
    tmpdir: TempDir,
    app: Router,
}

async fn start_mysql(tc: &Cli) -> (Container<GenericImage>, String, Pool<MySql>) {
    // MySQL 8 container
    let img = GenericImage::new("mysql:8.0")
        .with_env_var("MYSQL_ROOT_PASSWORD", "rootpass")
        .with_env_var("MYSQL_DATABASE", "countrydb")
        .with_env_var("MYSQL_USER", "appuser")
        .with_env_var("MYSQL_PASSWORD", "apppass")
        .with_wait_for(testcontainers::images::generic::WaitFor::message_on_stdout(
            "port: 3306  MySQL Community Server - GPL",
        ));

    let mysql: Container<GenericImage> = tc.run(img);

    // Host port exposure
    let host_port = mysql.get_host_port_ipv4(3306);
    let db_url = format!("mysql://appuser:apppass@127.0.0.1:{}/countrydb", host_port);

    // Wait for readiness
    let mut last_err = None;
    for _ in 0..60 {
        match MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
        {
            Ok(pool) => {
                // sanity ping
                if let Ok(1) =
                    sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&pool).await
                {
                    return (mysql, db_url, pool);
                }
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    panic!(
        "MySQL did not become ready: {:?}",
        last_err.map(|e| e.to_string())
    );
}

async fn run_migrations(pool: &Pool<MySql>) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sql_path = root.join("migrations/0001_init.sql");
    let sql = std::fs::read_to_string(sql_path).expect("read migrations/0001_init.sql");

    // naive splitter (fine for our simple migration)
    for stmt in sql.split(';') {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        sqlx::query(s).execute(pool).await.expect("run migration stmt");
    }
}

async fn start_mocks() -> MockServer {
    let server = MockServer::start().await;

    // Countries fixture (subset)
    let countries = serde_json::json!([
      {
        "name": "Nigeria",
        "capital": "Abuja",
        "region": "Africa",
        "population": 206139589,
        "flag": "https://flagcdn.com/ng.svg",
        "currencies": [ { "code": "NGN" } ]
      },
      {
        "name": "Ghana",
        "capital": "Accra",
        "region": "Africa",
        "population": 31072940,
        "flag": "https://flagcdn.com/gh.svg",
        "currencies": [ { "code": "GHS" } ]
      }
    ]);

    Mock::given(method("GET"))
        .and(path("/countries"))
        .respond_with(ResponseTemplate::new(200).set_body_json(countries))
        .mount(&server)
        .await;

    // Rates fixture
    let rates = serde_json::json!({
        "rates": { "NGN": 1600.23, "GHS": 15.34 }
    });

    Mock::given(method("GET"))
        .and(path("/rates"))
        .respond_with(ResponseTemplate::new(200).set_body_json(rates))
        .mount(&server)
        .await;

    server
}

async fn build_app(mock: &MockServer, db_url: &str, tmpdir: &TempDir) -> Router {
    // Point app to mocks
    env::set_var("COUNTRIES_URL", format!("{}/countries", mock.uri()));
    env::set_var("RATES_URL", format!("{}/rates", mock.uri()));
    env::set_var("BASE_CURRENCY", "USD");
    env::set_var("DATABASE_URL", db_url);
    env::set_var(
        "SUMMARY_IMAGE_PATH",
        tmpdir.path().join("summary.png").to_string_lossy().to_string(),
    );
    env::set_var("EXTERNAL_TIMEOUT_MS", "5000");
    env::set_var("PORT", "0"); // unused in tests

    // Build state via real config
    let cfg = crate::config::AppConfig::from_env().expect("config");
    let state = cfg.build_state().await.expect("state");
    crate::routes::router(state)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial] // isolate env + docker use
async fn it_refreshes_and_queries() {
    // Docker client
    let tc = Cli::default();

    // MySQL
    let (mysql, db_url, pool) = start_mysql(&tc).await;
    run_migrations(&pool).await;

    // Wiremock
    let mock = start_mocks().await;

    // Temp dir for image cache
    let tmpdir = TempDir::new().expect("tmpdir");

    // App router
    let app = build_app(&mock, &db_url, &tmpdir).await;

    // POST /countries/refresh
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/countries/refresh")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let j: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(j.get("inserted").unwrap().as_u64().unwrap() >= 2);

    // GET /countries?region=Africa
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/countries?region=Africa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let arr: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(arr.is_array());
    let items = arr.as_array().unwrap();
    assert_eq!(items.len(), 2);

    // GET /status
    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // GET /countries/Nigeria
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/countries/Nigeria")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // GET /countries/image
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/countries/image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let img_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert!(!img_bytes.is_empty());

    // Keep containers alive until end of test
    drop((mysql, pool));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn it_returns_503_when_rates_fail_and_does_not_modify_db() {
    let tc = Cli::default();
    let (mysql, db_url, pool) = start_mysql(&tc).await;
    run_migrations(&pool).await;

    let mock = MockServer::start().await;

    // Countries OK
    let countries = serde_json::json!([
      {
        "name": "Nigeria",
        "capital": "Abuja",
        "region": "Africa",
        "population": 206139589,
        "flag": "https://flagcdn.com/ng.svg",
        "currencies": [ { "code": "NGN" } ]
      }
    ]);
    Mock::given(method("GET"))
        .and(path("/countries"))
        .respond_with(ResponseTemplate::new(200).set_body_json(countries))
        .mount(&mock)
        .await;

    // Rates FAIL
    Mock::given(method("GET"))
        .and(path("/rates"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;

    let tmpdir = TempDir::new().unwrap();
    let app = build_app(&mock, &db_url, &tmpdir).await;

    // POST /countries/refresh → expect 503
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/countries/refresh")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    // DB should still be empty
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM countries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    drop((mysql, pool));
}
