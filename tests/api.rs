use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    response::Response,
};
use geoip::{geoip::GeoIpService, routes};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn health_reports_database_loaded_state() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty()));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response_json(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database_loaded"], false);
}

#[tokio::test]
async fn bearer_token_protects_health_endpoint() {
    let app = routes::router(routes::AppState::with_bearer_token(
        GeoIpService::empty(),
        Some("secret".to_string()),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn bearer_token_accepts_matching_authorization_header() {
    let app = routes::router(routes::AppState::with_bearer_token(
        GeoIpService::empty(),
        Some("secret".to_string()),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn bearer_token_rejects_wrong_authorization_header() {
    let app = routes::router(routes::AppState::with_bearer_token(
        GeoIpService::empty(),
        Some("secret".to_string()),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn single_lookup_rejects_invalid_ip() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty()));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/lookup/bad-ip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "invalid_ip");
    assert_eq!(body["error"]["message"], "invalid IP address");
}

#[tokio::test]
async fn batch_lookup_returns_database_unavailable_before_validating_items() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty()));
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

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "database_unavailable");
    assert_eq!(body["error"]["message"], "GeoIP database is not loaded");
}

#[tokio::test]
async fn batch_lookup_returns_database_unavailable_when_database_is_not_loaded() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty()));
    let body = Body::from(json!({ "ips": ["8.8.8.8"] }).to_string());

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

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "database_unavailable");
    assert_eq!(body["error"]["message"], "GeoIP database is not loaded");
}

#[tokio::test]
async fn admin_database_update_endpoint_is_not_registered() {
    let app = routes::router(routes::AppState::new(GeoIpService::empty()));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/database/update")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn response_json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
