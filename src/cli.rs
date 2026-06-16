use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub const DEFAULT_DB_PATH: &str = "data/GeoLite2-City.mmdb";
pub const DEFAULT_BIND: &str = "0.0.0.0:5000";

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
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: String,
        #[arg(long, default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
        #[arg(long)]
        download: bool,
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
                download: false,
            }
        );
    }

    #[test]
    fn serve_uses_default_bind() {
        let cli = Cli::parse_from(["geoip", "serve"]);

        assert_eq!(
            cli.command,
            Command::Serve {
                bind: "0.0.0.0:5000".to_string(),
                db_path: PathBuf::from("data/GeoLite2-City.mmdb"),
                download: false,
            }
        );
    }

    #[test]
    fn parses_serve_download_flag() {
        let cli = Cli::parse_from(["geoip", "serve", "--download"]);

        assert_eq!(
            cli.command,
            Command::Serve {
                bind: "0.0.0.0:5000".to_string(),
                db_path: PathBuf::from("data/GeoLite2-City.mmdb"),
                download: true,
            }
        );
    }

    #[test]
    fn serve_download_does_not_accept_value() {
        let err = Cli::try_parse_from(["geoip", "serve", "--download", "true"])
            .expect_err("download is a flag");

        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
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
