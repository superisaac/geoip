# GeoIP REST API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust + axum REST API that queries MaxMind GeoLite2 City `.mmdb` data for one or more IP addresses and can download/hot-load the latest database.

**Architecture:** Create a small Cargo application split into focused modules: configuration, error mapping, GeoIP lookup, database downloading/updating, HTTP routes, and startup wiring. Handlers depend on a clonable `AppState` that owns a `GeoIpService` with an async-safe current database reader, so update operations can validate and swap a reader without breaking active lookups.

**Tech Stack:** Rust 2021, axum, tokio, serde, maxminddb, reqwest, tar, flate2, tempfile, tower, tower-http, thiserror, tracing.

---

## File Structure

- Create `Cargo.toml`: crate metadata, runtime dependencies, and test dependencies.
- Create `src/lib.rs`: exports modules for handler and unit tests.
- Create `src/main.rs`: loads config, initializes state, optional startup update, background refresh, and HTTP serving.
- Create `src/config.rs`: environment parsing and typed service configuration.
- Create `src/error.rs`: `AppError`, stable error codes, and `IntoResponse` mapping.
- Create `src/geoip.rs`: mmdb reader loading, current reader state, lookup result DTOs, and lookup behavior.
- Create `src/download.rs`: MaxMind URL construction, archive download, extraction, atomic replacement, and update coordination.
- Create `src/routes.rs`: axum router, request/response DTOs, health, single lookup, batch lookup, and manual update handlers.
- Modify `README.md`: local run instructions, environment variables, and API examples.
- Create `tests/api.rs`: route-level tests using a fake lookup service path where possible.

## Task 1: Cargo Project Skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`

- [ ] **Step 1: Write the failing skeleton check**

Run:

```bash
cargo check
```

Expected: FAIL with `could not find Cargo.toml`.

- [ ] **Step 2: Create `Cargo.toml`**

```toml
[package]
name = "geoip"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1"
async-trait = "0.1"
axum = "0.7"
base64 = "0.22"
flate2 = "1"
maxminddb = "0.24"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tar = "0.4"
tempfile = "3"
thiserror = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal", "fs", "sync", "time"] }
tower-http = { version = "0.5", features = ["trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
```

- [ ] **Step 3: Create minimal library and binary**

`src/lib.rs`:

```rust
pub mod config;
pub mod download;
pub mod error;
pub mod geoip;
pub mod routes;
```

`src/main.rs`:

```rust
fn main() {
    println!("geoip service");
}
```

- [ ] **Step 4: Run check to verify the skeleton compiles as far as missing modules**

Run:

```bash
cargo check
```

Expected: FAIL with missing module files for `config`, `download`, `error`, `geoip`, and `routes`.

- [ ] **Step 5: Add empty module files and verify check passes**

Create empty files:

```text
src/config.rs
src/download.rs
src/error.rs
src/geoip.rs
src/routes.rs
```

Run:

```bash
cargo check
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add Cargo.toml src
git commit -m "chore: add rust project skeleton"
```

Expected: Commit succeeds. If the environment cannot write `.git`, record that the commit was skipped and continue.

## Task 2: Configuration

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing config tests**

Append to `src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config_from(pairs: &[(&str, &str)]) -> Config {
        let env = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect::<HashMap<_, _>>();
        Config::from_map(&env).expect("config should parse")
    }

    #[test]
    fn uses_defaults_when_env_is_empty() {
        let config = Config::from_map(&HashMap::new()).expect("config should parse");

        assert_eq!(config.bind_addr, "0.0.0.0:3000");
        assert_eq!(config.db_path, std::path::PathBuf::from("data/GeoLite2-City.mmdb"));
        assert!(config.update_on_startup);
        assert_eq!(config.update_interval_hours, 24);
        assert!(config.maxmind_account_id.is_none());
        assert!(config.maxmind_license_key.is_none());
    }

    #[test]
    fn parses_overrides() {
        let config = config_from(&[
            ("BIND_ADDR", "127.0.0.1:8080"),
            ("GEOIP_DB_PATH", "/tmp/test.mmdb"),
            ("GEOIP_UPDATE_ON_STARTUP", "false"),
            ("GEOIP_UPDATE_INTERVAL_HOURS", "0"),
            ("MAXMIND_ACCOUNT_ID", "123"),
            ("MAXMIND_LICENSE_KEY", "abc"),
        ]);

        assert_eq!(config.bind_addr, "127.0.0.1:8080");
        assert_eq!(config.db_path, std::path::PathBuf::from("/tmp/test.mmdb"));
        assert!(!config.update_on_startup);
        assert_eq!(config.update_interval_hours, 0);
        assert_eq!(config.maxmind_account_id.as_deref(), Some("123"));
        assert_eq!(config.maxmind_license_key.as_deref(), Some("abc"));
    }

    #[test]
    fn rejects_invalid_interval() {
        let env = HashMap::from([("GEOIP_UPDATE_INTERVAL_HOURS".to_string(), "abc".to_string())]);
        let err = Config::from_map(&env).expect_err("invalid integer should fail");

        assert!(err.to_string().contains("GEOIP_UPDATE_INTERVAL_HOURS"));
    }
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test config
```

Expected: FAIL because `Config` and `Config::from_map` are undefined.

- [ ] **Step 3: Implement config**

Replace `src/config.rs` with:

```rust
use std::{collections::HashMap, env, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub bind_addr: String,
    pub db_path: PathBuf,
    pub update_on_startup: bool,
    pub update_interval_hours: u64,
    pub maxmind_account_id: Option<String>,
    pub maxmind_license_key: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("invalid boolean for {name}: {value}")]
    InvalidBool { name: &'static str, value: String },
    #[error("invalid integer for {name}: {value}")]
    InvalidInteger { name: &'static str, value: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let env = env::vars().collect::<HashMap<_, _>>();
        Self::from_map(&env)
    }

    pub fn from_map(env: &HashMap<String, String>) -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: get(env, "BIND_ADDR").unwrap_or("0.0.0.0:3000").to_string(),
            db_path: PathBuf::from(get(env, "GEOIP_DB_PATH").unwrap_or("data/GeoLite2-City.mmdb")),
            update_on_startup: parse_bool(env, "GEOIP_UPDATE_ON_STARTUP", true)?,
            update_interval_hours: parse_u64(env, "GEOIP_UPDATE_INTERVAL_HOURS", 24)?,
            maxmind_account_id: optional(env, "MAXMIND_ACCOUNT_ID"),
            maxmind_license_key: optional(env, "MAXMIND_LICENSE_KEY"),
        })
    }
}

fn get<'a>(env: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    env.get(name).map(String::as_str).filter(|value| !value.is_empty())
}

fn optional(env: &HashMap<String, String>, name: &str) -> Option<String> {
    get(env, name).map(ToOwned::to_owned)
}

fn parse_bool(
    env: &HashMap<String, String>,
    name: &'static str,
    default: bool,
) -> Result<bool, ConfigError> {
    match get(env, name) {
        None => Ok(default),
        Some("true" | "1" | "yes" | "on") => Ok(true),
        Some("false" | "0" | "no" | "off") => Ok(false),
        Some(value) => Err(ConfigError::InvalidBool {
            name,
            value: value.to_string(),
        }),
    }
}

fn parse_u64(
    env: &HashMap<String, String>,
    name: &'static str,
    default: u64,
) -> Result<u64, ConfigError> {
    match get(env, name) {
        None => Ok(default),
        Some(value) => value.parse().map_err(|_| ConfigError::InvalidInteger {
            name,
            value: value.to_string(),
        }),
    }
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
cargo test config
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/config.rs
git commit -m "feat: add service configuration"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Task 3: Error Responses

**Files:**
- Modify: `src/error.rs`

- [ ] **Step 1: Write failing error tests**

Append to `src/error.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn invalid_ip_maps_to_bad_request() {
        let response = AppError::invalid_ip().into_response();
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn database_unavailable_maps_to_service_unavailable() {
        let response = AppError::DatabaseUnavailable.into_response();
        assert_eq!(response.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test error
```

Expected: FAIL because `AppError` is undefined.

- [ ] **Step 3: Implement error type**

Replace `src/error.rs` with:

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Clone)]
pub enum AppError {
    InvalidIp,
    NotFound,
    DatabaseUnavailable,
    MissingCredentials,
    DownloadFailed(String),
    DatabaseUpdateFailed(String),
    BadRequest(String),
    Internal(String),
}

#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn invalid_ip() -> Self {
        Self::InvalidIp
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidIp | Self::MissingCredentials | Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::DatabaseUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::DownloadFailed(_) => StatusCode::BAD_GATEWAY,
            Self::DatabaseUpdateFailed(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidIp => "invalid_ip",
            Self::NotFound => "not_found",
            Self::DatabaseUnavailable => "database_unavailable",
            Self::MissingCredentials => "missing_credentials",
            Self::DownloadFailed(_) => "download_failed",
            Self::DatabaseUpdateFailed(_) => "database_update_failed",
            Self::BadRequest(_) => "bad_request",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::InvalidIp => "invalid IP address".to_string(),
            Self::NotFound => "IP address was not found in the GeoIP database".to_string(),
            Self::DatabaseUnavailable => "GeoIP database is not loaded".to_string(),
            Self::MissingCredentials => "MaxMind credentials are required".to_string(),
            Self::DownloadFailed(message)
            | Self::DatabaseUpdateFailed(message)
            | Self::BadRequest(message)
            | Self::Internal(message) => message.clone(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.message(),
            },
        };

        (status, Json(body)).into_response()
    }
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
cargo test error
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/error.rs
git commit -m "feat: add json error responses"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Task 4: GeoIP Lookup Service

**Files:**
- Modify: `src/geoip.rs`

- [ ] **Step 1: Write failing lookup state tests**

Append to `src/geoip.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::IpAddr, str::FromStr};

    #[tokio::test]
    async fn unloaded_service_reports_database_unavailable() {
        let service = GeoIpService::empty();
        let ip = IpAddr::from_str("8.8.8.8").unwrap();

        let err = service.lookup(ip).await.expect_err("database should be missing");

        assert!(matches!(err, crate::error::AppError::DatabaseUnavailable));
    }

    #[test]
    fn lookup_result_serializes_without_empty_optional_fields() {
        let result = LookupResult {
            ip: "8.8.8.8".to_string(),
            country: Some(NamedCode {
                iso_code: Some("US".to_string()),
                name: Some("United States".to_string()),
            }),
            city: None,
            location: None,
            subdivisions: Vec::new(),
        };

        let json = serde_json::to_value(result).unwrap();

        assert_eq!(json["ip"], "8.8.8.8");
        assert!(json.get("city").is_none());
        assert_eq!(json["country"]["iso_code"], "US");
    }
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test geoip
```

Expected: FAIL because `GeoIpService`, `LookupResult`, and related DTOs are undefined.

- [ ] **Step 3: Implement lookup service and DTOs**

Replace `src/geoip.rs` with:

```rust
use std::{net::IpAddr, path::Path, sync::Arc};

use maxminddb::{geoip2, Reader};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::error::AppError;

#[derive(Clone, Default)]
pub struct GeoIpService {
    reader: Arc<RwLock<Option<Arc<Reader<Vec<u8>>>>>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LookupResult {
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<NamedCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<NamedValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subdivisions: Vec<NamedCode>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NamedCode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NamedValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

impl GeoIpService {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_reader(reader: Reader<Vec<u8>>) -> Self {
        Self {
            reader: Arc::new(RwLock::new(Some(Arc::new(reader)))),
        }
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let reader = Reader::open_readfile(path)
            .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
        Ok(Self::from_reader(reader))
    }

    pub async fn is_loaded(&self) -> bool {
        self.reader.read().await.is_some()
    }

    pub async fn replace_from_path(&self, path: impl AsRef<Path>) -> Result<(), AppError> {
        let reader = Reader::open_readfile(path)
            .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
        *self.reader.write().await = Some(Arc::new(reader));
        Ok(())
    }

    pub async fn lookup(&self, ip: IpAddr) -> Result<LookupResult, AppError> {
        let reader = self
            .reader
            .read()
            .await
            .clone()
            .ok_or(AppError::DatabaseUnavailable)?;

        let city = reader
            .lookup::<geoip2::City>(ip)
            .map_err(|error| match error {
                maxminddb::MaxMindDBError::AddressNotFoundError(_) => AppError::NotFound,
                other => AppError::Internal(other.to_string()),
            })?;

        Ok(city_to_result(ip, city))
    }
}

fn english_name(names: Option<&std::collections::HashMap<&str, &str>>) -> Option<String> {
    names.and_then(|names| names.get("en").copied()).map(ToOwned::to_owned)
}

fn city_to_result(ip: IpAddr, city: geoip2::City<'_>) -> LookupResult {
    let country = city.country.map(|country| NamedCode {
        iso_code: country.iso_code.map(ToOwned::to_owned),
        name: english_name(country.names.as_ref()),
    });

    let city_name = city.city.and_then(|city| english_name(city.names.as_ref()));
    let location = city.location.map(|location| Location {
        latitude: location.latitude,
        longitude: location.longitude,
        time_zone: location.time_zone.map(ToOwned::to_owned),
    });

    let subdivisions = city
        .subdivisions
        .unwrap_or_default()
        .into_iter()
        .map(|subdivision| NamedCode {
            iso_code: subdivision.iso_code.map(ToOwned::to_owned),
            name: english_name(subdivision.names.as_ref()),
        })
        .collect();

    LookupResult {
        ip: ip.to_string(),
        country,
        city: city_name.map(|name| NamedValue { name: Some(name) }),
        location,
        subdivisions,
    }
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
cargo test geoip
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/geoip.rs
git commit -m "feat: add geoip lookup service"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Task 5: HTTP Routes

**Files:**
- Modify: `src/routes.rs`
- Create: `tests/api.rs`

- [ ] **Step 1: Write failing route tests**

Create `tests/api.rs`:

```rust
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use geoip::{geoip::GeoIpService, routes};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn health_reports_database_loaded_state() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty(), None, None));

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn single_lookup_rejects_invalid_ip() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty(), None, None));

    let response = app
        .oneshot(Request::builder().uri("/lookup/bad-ip").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn batch_lookup_keeps_invalid_ip_as_item_error() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty(), None, None));
    let body = Body::from(json!({ "ips": ["bad-ip"] }).to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lookup")
                .header("content-type", "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test --test api
```

Expected: FAIL because `routes::router` and `routes::AppState` are undefined.

- [ ] **Step 3: Implement routes**

Replace `src/routes.rs` with:

```rust
use std::{net::IpAddr, path::PathBuf, str::FromStr, sync::Arc};

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    download,
    error::{AppError, ErrorBody},
    geoip::{GeoIpService, LookupResult},
};

#[derive(Clone)]
pub struct AppState {
    pub geoip: GeoIpService,
    pub db_path: Option<PathBuf>,
    pub maxmind: Option<download::MaxMindCredentials>,
}

impl AppState {
    pub fn new(
        geoip: GeoIpService,
        db_path: Option<PathBuf>,
        maxmind: Option<download::MaxMindCredentials>,
    ) -> Arc<Self> {
        Arc::new(Self {
            geoip,
            db_path,
            maxmind,
        })
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    database_loaded: bool,
}

#[derive(Debug, Deserialize)]
struct BatchLookupRequest {
    ips: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BatchLookupResponse {
    results: Vec<BatchLookupItem>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum BatchLookupItem {
    Success(LookupResult),
    Error { ip: String, error: ErrorBody },
}

#[derive(Debug, Serialize)]
struct UpdateResponse {
    updated: bool,
    database_loaded: bool,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/lookup/:ip", get(lookup_one))
        .route("/lookup", post(lookup_batch))
        .route("/admin/database/update", post(update_database))
        .with_state(state)
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        database_loaded: state.geoip.is_loaded().await,
    })
}

async fn lookup_one(
    State(state): State<Arc<AppState>>,
    Path(ip): Path<String>,
) -> Result<Json<LookupResult>, AppError> {
    let ip = IpAddr::from_str(&ip).map_err(|_| AppError::InvalidIp)?;
    state.geoip.lookup(ip).await.map(Json)
}

async fn lookup_batch(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BatchLookupRequest>,
) -> Json<BatchLookupResponse> {
    let mut results = Vec::with_capacity(payload.ips.len());

    for raw_ip in payload.ips {
        let ip = match IpAddr::from_str(&raw_ip) {
            Ok(ip) => ip,
            Err(_) => {
                results.push(BatchLookupItem::Error {
                    ip: raw_ip,
                    error: ErrorBody {
                        code: "invalid_ip",
                        message: "invalid IP address".to_string(),
                    },
                });
                continue;
            }
        };

        match state.geoip.lookup(ip).await {
            Ok(result) => results.push(BatchLookupItem::Success(result)),
            Err(error) => results.push(BatchLookupItem::Error {
                ip: raw_ip,
                error: ErrorBody {
                    code: error.code(),
                    message: error.message(),
                },
            }),
        }
    }

    Json(BatchLookupResponse { results })
}

async fn update_database(State(state): State<Arc<AppState>>) -> Result<Json<UpdateResponse>, AppError> {
    let db_path = state
        .db_path
        .as_ref()
        .ok_or_else(|| AppError::DatabaseUpdateFailed("database path is not configured".to_string()))?;
    let credentials = state.maxmind.as_ref().ok_or(AppError::MissingCredentials)?;

    download::download_and_replace(credentials, db_path).await?;
    state.geoip.replace_from_path(db_path).await?;

    Ok(Json(UpdateResponse {
        updated: true,
        database_loaded: state.geoip.is_loaded().await,
    }))
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
cargo test --test api
```

Expected: PASS after `download` exposes the stubbed types in Task 6. If it fails only because `download::MaxMindCredentials` is undefined, proceed to Task 6 and re-run.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/routes.rs tests/api.rs
git commit -m "feat: add geoip http routes"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Task 6: MaxMind Download and Atomic Replacement

**Files:**
- Modify: `src/download.rs`

- [ ] **Step 1: Write failing download tests**

Append to `src/download.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_geolite2_city_download_url() {
        let credentials = MaxMindCredentials {
            account_id: "123".to_string(),
            license_key: "secret".to_string(),
        };

        let url = download_url(&credentials);

        assert!(url.contains("GeoLite2-City"));
        assert!(url.contains("suffix=tar.gz"));
    }

    #[test]
    fn rejects_incomplete_credentials() {
        let result = MaxMindCredentials::new(Some("123".to_string()), None);

        assert!(matches!(result, Err(crate::error::AppError::MissingCredentials)));
    }
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```bash
cargo test download
```

Expected: FAIL because download functions and credential types are undefined.

- [ ] **Step 3: Implement download module**

Replace `src/download.rs` with:

```rust
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine as _};
use flate2::read::GzDecoder;
use tar::Archive;
use tempfile::NamedTempFile;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct MaxMindCredentials {
    pub account_id: String,
    pub license_key: String,
}

impl MaxMindCredentials {
    pub fn new(account_id: Option<String>, license_key: Option<String>) -> Result<Option<Self>, AppError> {
        match (account_id, license_key) {
            (Some(account_id), Some(license_key)) => Ok(Some(Self {
                account_id,
                license_key,
            })),
            (None, None) => Ok(None),
            _ => Err(AppError::MissingCredentials),
        }
    }
}

pub fn download_url(_credentials: &MaxMindCredentials) -> String {
    "https://download.maxmind.com/geoip/databases/GeoLite2-City/download?suffix=tar.gz".to_string()
}

pub async fn download_and_replace(
    credentials: &MaxMindCredentials,
    target_path: impl AsRef<Path>,
) -> Result<(), AppError> {
    let bytes = download_archive(credentials).await?;
    extract_and_replace_mmdb(&bytes, target_path.as_ref())
}

async fn download_archive(credentials: &MaxMindCredentials) -> Result<Vec<u8>, AppError> {
    let auth = general_purpose::STANDARD.encode(format!(
        "{}:{}",
        credentials.account_id, credentials.license_key
    ));

    let response = reqwest::Client::new()
        .get(download_url(credentials))
        .header(reqwest::header::AUTHORIZATION, format!("Basic {auth}"))
        .send()
        .await
        .map_err(|error| AppError::DownloadFailed(error.to_string()))?;

    if !response.status().is_success() {
        return Err(AppError::DownloadFailed(format!(
            "MaxMind download returned status {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::DownloadFailed(error.to_string()))?;

    Ok(bytes.to_vec())
}

fn extract_and_replace_mmdb(bytes: &[u8], target_path: &Path) -> Result<(), AppError> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
    }

    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(parent)
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;

    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    let mut found = false;

    for entry in archive
        .entries()
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?
    {
        let mut entry = entry.map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
        let path = entry
            .path()
            .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?
            .to_path_buf();

        if path.extension().and_then(|value| value.to_str()) == Some("mmdb") {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
            std::io::Write::write_all(&mut temp, &data)
                .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
            found = true;
            break;
        }
    }

    if !found {
        return Err(AppError::DatabaseUpdateFailed(
            "GeoLite2 archive did not contain an mmdb file".to_string(),
        ));
    }

    temp.persist(target_path)
        .map_err(|error| AppError::DatabaseUpdateFailed(error.error.to_string()))?;

    Ok(())
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```bash
cargo test download
cargo test --test api
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/download.rs
git commit -m "feat: add maxmind database download"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Task 7: Application Startup

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write the failing startup check**

Run:

```bash
cargo check
```

Expected: FAIL if `main.rs` still only prints and does not import current modules, or PASS if previous tasks compile. This step establishes the current baseline before replacing startup wiring.

- [ ] **Step 2: Implement axum startup**

Replace `src/main.rs` with:

```rust
use std::{path::Path, time::Duration};

use anyhow::Context;
use geoip::{
    config::Config,
    download::{self, MaxMindCredentials},
    geoip::GeoIpService,
    routes::{self, AppState},
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env().context("failed to load configuration")?;
    let credentials = MaxMindCredentials::new(
        config.maxmind_account_id.clone(),
        config.maxmind_license_key.clone(),
    )?;

    let geoip = if Path::new(&config.db_path).exists() {
        GeoIpService::load_from_path(&config.db_path)?
    } else {
        GeoIpService::empty()
    };

    if config.update_on_startup && !geoip.is_loaded().await {
        if let Some(credentials) = credentials.as_ref() {
            match download::download_and_replace(credentials, &config.db_path).await {
                Ok(()) => geoip.replace_from_path(&config.db_path).await?,
                Err(error) => warn!(%error, "startup GeoIP database update failed"),
            }
        } else {
            warn!("GeoIP database is missing and MaxMind credentials are not configured");
        }
    }

    if config.update_interval_hours > 0 {
        spawn_refresh_task(
            geoip.clone(),
            config.db_path.clone(),
            credentials.clone(),
            config.update_interval_hours,
        );
    }

    let state = AppState::new(geoip, Some(config.db_path.clone()), credentials);
    let app = routes::router(state).layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(&config.bind_addr).await?;

    info!(bind_addr = %config.bind_addr, "GeoIP API listening");
    axum::serve(listener, app).await?;

    Ok(())
}

fn spawn_refresh_task(
    geoip: GeoIpService,
    db_path: std::path::PathBuf,
    credentials: Option<MaxMindCredentials>,
    interval_hours: u64,
) {
    tokio::spawn(async move {
        let Some(credentials) = credentials else {
            return;
        };
        let mut interval = tokio::time::interval(Duration::from_secs(interval_hours * 60 * 60));

        loop {
            interval.tick().await;
            match download::download_and_replace(&credentials, &db_path).await {
                Ok(()) => {
                    if let Err(error) = geoip.replace_from_path(&db_path).await {
                        error!(%error, "failed to load refreshed GeoIP database");
                    }
                }
                Err(error) => warn!(%error, "background GeoIP database update failed"),
            }
        }
    });
}
```

- [ ] **Step 3: Run check and tests**

Run:

```bash
cargo check
cargo test
```

Expected: PASS.

- [ ] **Step 4: Commit**

Run:

```bash
git add src/main.rs
git commit -m "feat: wire geoip api server"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Task 8: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README with run and API instructions**

Replace `README.md` with:

```markdown
# GeoIP

A Rust + axum REST API for looking up geographic location data for one or more IP addresses using a MaxMind GeoLite2 City `.mmdb` database.

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `BIND_ADDR` | `0.0.0.0:3000` | HTTP listen address |
| `GEOIP_DB_PATH` | `data/GeoLite2-City.mmdb` | Local mmdb path |
| `GEOIP_UPDATE_ON_STARTUP` | `true` | Download database on startup when needed |
| `GEOIP_UPDATE_INTERVAL_HOURS` | `24` | Background refresh interval; `0` disables refresh |
| `MAXMIND_ACCOUNT_ID` | unset | MaxMind account ID for downloads |
| `MAXMIND_LICENSE_KEY` | unset | MaxMind license key for downloads |

Lookup endpoints work without MaxMind credentials when `GEOIP_DB_PATH` already points to a valid GeoLite2 City database.

## Run

```bash
cargo run
```

With automatic download:

```bash
MAXMIND_ACCOUNT_ID=123 \
MAXMIND_LICENSE_KEY=your-license-key \
cargo run
```

## API

Health:

```bash
curl http://localhost:3000/health
```

Single lookup:

```bash
curl http://localhost:3000/lookup/8.8.8.8
```

Batch lookup:

```bash
curl -X POST http://localhost:3000/lookup \
  -H 'content-type: application/json' \
  -d '{"ips":["8.8.8.8","1.1.1.1","bad-ip"]}'
```

Manual database update:

```bash
curl -X POST http://localhost:3000/admin/database/update
```

## Test

```bash
cargo test
```
```

- [ ] **Step 2: Run final verification**

Run:

```bash
cargo fmt --check
cargo test
cargo check
```

Expected: PASS.

- [ ] **Step 3: Commit**

Run:

```bash
git add README.md
git commit -m "docs: add geoip api usage"
```

Expected: Commit succeeds or is recorded as skipped if `.git` is not writable.

## Self-Review

- Spec coverage: the plan covers axum endpoints, GeoLite2 City lookup, single and batch lookup, automatic and manual download/update, stable errors, startup behavior, and README usage.
- Placeholder scan: no unfinished placeholder markers or unspecified implementation steps are intentionally left.
- Type consistency: `Config`, `AppError`, `GeoIpService`, `MaxMindCredentials`, `AppState`, and route function names are introduced before use by later tasks.
- Known execution constraint: this environment currently cannot write `.git/index.lock` without an escalation path that is being rejected by the approval service. Commit steps remain in the plan for normal execution; in this session they may need to be recorded as skipped.
