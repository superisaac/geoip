use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone)]
pub enum AppError {
    InvalidIp,
    NotFound,
    DatabaseUnavailable,
    MissingCredentials,
    DownloadFailed(String),
    DatabaseUpdateFailed(String),
    BadRequest(String),
    Unauthorized,
    Internal(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
            Self::InvalidIp | Self::MissingCredentials | Self::BadRequest(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
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
            Self::Unauthorized => "unauthorized",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::InvalidIp => "invalid IP address".to_string(),
            Self::NotFound => "IP address was not found in the GeoIP database".to_string(),
            Self::DatabaseUnavailable => "GeoIP database is not loaded".to_string(),
            Self::MissingCredentials => "MaxMind credentials are required".to_string(),
            Self::DownloadFailed(_) => "Failed to download GeoIP database".to_string(),
            Self::DatabaseUpdateFailed(_) => "Failed to update GeoIP database".to_string(),
            Self::BadRequest(message) => message.clone(),
            Self::Unauthorized => "Unauthorized".to_string(),
            Self::Internal(_) => "Internal server error".to_string(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message())
    }
}

impl std::error::Error for AppError {}

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, response::IntoResponse};

    #[test]
    fn variants_map_to_expected_status_code_and_public_error_body() {
        let cases = [
            (
                AppError::InvalidIp,
                axum::http::StatusCode::BAD_REQUEST,
                "invalid_ip",
                "invalid IP address",
            ),
            (
                AppError::NotFound,
                axum::http::StatusCode::NOT_FOUND,
                "not_found",
                "IP address was not found in the GeoIP database",
            ),
            (
                AppError::DatabaseUnavailable,
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "database_unavailable",
                "GeoIP database is not loaded",
            ),
            (
                AppError::MissingCredentials,
                axum::http::StatusCode::BAD_REQUEST,
                "missing_credentials",
                "MaxMind credentials are required",
            ),
            (
                AppError::DownloadFailed("https://example.com/private.mmdb".to_string()),
                axum::http::StatusCode::BAD_GATEWAY,
                "download_failed",
                "Failed to download GeoIP database",
            ),
            (
                AppError::DatabaseUpdateFailed("/tmp/private/GeoLite2.mmdb".to_string()),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "database_update_failed",
                "Failed to update GeoIP database",
            ),
            (
                AppError::BadRequest("country must be a two-letter code".to_string()),
                axum::http::StatusCode::BAD_REQUEST,
                "bad_request",
                "country must be a two-letter code",
            ),
            (
                AppError::Unauthorized,
                axum::http::StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Unauthorized",
            ),
            (
                AppError::Internal("parser failed at byte 42".to_string()),
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Internal server error",
            ),
        ];

        for (error, expected_status, expected_code, expected_message) in cases {
            assert_eq!(error.status_code(), expected_status);
            assert_eq!(error.code(), expected_code);
            assert_eq!(error.message(), expected_message);
        }
    }

    #[tokio::test]
    async fn raw_internal_message_is_not_serialized_in_response_body() {
        let response = AppError::Internal("secret".to_string()).into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "internal_error");
        assert_eq!(value["error"]["message"], "Internal server error");
        assert!(!String::from_utf8_lossy(&body).contains("secret"));
    }
}
