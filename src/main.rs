use std::env;

use anyhow::Context;
use clap::Parser;
use geoip::{
    cli::{Cli, Command},
    download::{self, MaxMindCredentials},
    geoip::GeoIpService,
    routes::{self, AppState},
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match Cli::parse().command {
        Command::Serve { bind, db_path } => serve(bind, db_path).await,
        Command::Download { db_path } => download_database(db_path).await,
    }
}

async fn serve(bind: String, db_path: std::path::PathBuf) -> anyhow::Result<()> {
    let geoip = if db_path.exists() {
        GeoIpService::load_from_path(&db_path)
            .with_context(|| format!("failed to load GeoIP database from {}", db_path.display()))?
    } else {
        warn!(db_path = %db_path.display(), "GeoIP database does not exist; lookup endpoints will return 503");
        GeoIpService::empty()
    };

    let state = AppState::with_bearer_token(geoip, bearer_token_from_env());
    let app = routes::router(state).layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(&bind)
        .await
        .with_context(|| format!("failed to bind TCP listener to {bind}"))?;

    info!(bind_addr = %bind, db_path = %db_path.display(), "GeoIP API listening");
    axum::serve(listener, app).await?;

    Ok(())
}

fn load_dotenv() {
    if let Err(error) = dotenvy::dotenv() {
        if !matches!(error, dotenvy::Error::Io(ref io_error) if io_error.kind() == std::io::ErrorKind::NotFound)
        {
            warn!(%error, "failed to load .env");
        }
    }
}

fn bearer_token_from_env() -> Option<String> {
    env::var("GEOIP_BEARER_TOKEN")
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

async fn download_database(db_path: std::path::PathBuf) -> anyhow::Result<()> {
    let credentials = MaxMindCredentials::new(
        env::var("MAXMIND_ACCOUNT_ID").ok(),
        env::var("MAXMIND_LICENSE_KEY").ok(),
    )?
    .ok_or(geoip::error::AppError::MissingCredentials)?;

    download::download_and_replace(&credentials, &db_path).await?;
    info!(db_path = %db_path.display(), "GeoLite2 City database downloaded");

    Ok(())
}
