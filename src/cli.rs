use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_DB_PATH: &str = "data/GeoLite2-City.mmdb";

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "geoip",
    version,
    about = "GeoIP REST API and database downloader"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Serve {
        #[arg(long)]
        bind: String,
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },
    Download {
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::*;

    #[test]
    fn parses_serve_command() {
        let cli = Cli::parse_from([
            "geoip",
            "serve",
            "--bind",
            "127.0.0.1:8080",
            "--db-path",
            "/tmp/geo.mmdb",
        ]);

        assert_eq!(
            cli.command,
            Command::Serve {
                bind: "127.0.0.1:8080".to_string(),
                db_path: PathBuf::from("/tmp/geo.mmdb"),
            }
        );
    }

    #[test]
    fn serve_requires_bind() {
        let err = Cli::try_parse_from(["geoip", "serve"]).expect_err("bind is required");

        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parses_download_command_with_default_db_path() {
        let cli = Cli::parse_from(["geoip", "download"]);

        assert_eq!(
            cli.command,
            Command::Download {
                db_path: PathBuf::from("data/GeoLite2-City.mmdb"),
            }
        );
    }
}
