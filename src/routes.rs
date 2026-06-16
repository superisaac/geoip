use std::{net::IpAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, ErrorBody},
    geoip::{GeoIpService, LookupResult},
};

#[derive(Clone)]
pub struct AppState {
    geoip: GeoIpService,
    bearer_token: Option<String>,
}

impl AppState {
    pub fn new(geoip: GeoIpService) -> Arc<Self> {
        Self::with_bearer_token(geoip, None)
    }

    pub fn with_bearer_token(geoip: GeoIpService, bearer_token: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            geoip,
            bearer_token: bearer_token.and_then(normalize_bearer_token),
        })
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/lookup/:ip", get(single_lookup))
        .route("/lookup", post(batch_lookup))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    database_loaded: bool,
}

async fn health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<HealthResponse>, AppError> {
    authorize(&state, &headers)?;

    Ok(Json(HealthResponse {
        status: "ok",
        database_loaded: state.geoip.is_loaded().await,
    }))
}

async fn single_lookup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(ip): Path<String>,
) -> Result<Json<LookupResult>, AppError> {
    authorize(&state, &headers)?;

    let ip = parse_ip(&ip)?;
    state.geoip.lookup(ip).await.map(Json)
}

#[derive(Debug, Deserialize)]
struct BatchLookupRequest {
    ips: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BatchLookupResponse {
    results: Vec<BatchLookupItem>,
}

async fn batch_lookup(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<BatchLookupRequest>,
) -> Result<Json<BatchLookupResponse>, AppError> {
    authorize(&state, &headers)?;

    if !state.geoip.is_loaded().await {
        return Err(AppError::DatabaseUnavailable);
    }

    let mut results = Vec::with_capacity(request.ips.len());

    for ip in request.ips {
        let item = match parse_ip(&ip) {
            Ok(parsed_ip) => match state.geoip.lookup(parsed_ip).await {
                Ok(result) => BatchLookupItem::Success(result),
                Err(AppError::DatabaseUnavailable) => return Err(AppError::DatabaseUnavailable),
                Err(error) => BatchLookupItem::error(ip, error),
            },
            Err(error) => BatchLookupItem::error(ip, error),
        };

        results.push(item);
    }

    Ok(Json(BatchLookupResponse { results }))
}

impl BatchLookupItem {
    fn error(ip: String, error: AppError) -> Self {
        Self::Error(BatchLookupError {
            ip,
            error: ErrorBody {
                code: error.code(),
                message: error.message(),
            },
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum BatchLookupItem {
    Success(LookupResult),
    Error(BatchLookupError),
}

#[derive(Debug, Serialize)]
struct BatchLookupError {
    ip: String,
    error: ErrorBody,
}

fn parse_ip(ip: &str) -> Result<IpAddr, AppError> {
    ip.parse().map_err(|_| AppError::invalid_ip())
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let Some(expected_token) = state.bearer_token.as_deref() else {
        return Ok(());
    };

    let Some(header) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(AppError::Unauthorized);
    };

    let Ok(header) = header.to_str() else {
        return Err(AppError::Unauthorized);
    };

    if header == format!("Bearer {expected_token}") {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn normalize_bearer_token(token: String) -> Option<String> {
    let token = token.trim().to_string();

    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}
