use std::{
    ffi::OsStr,
    fmt, fs,
    io::{self, Write},
    path::Path,
};

use flate2::read::GzDecoder;
use maxminddb::Reader;
use tar::Archive;
use tempfile::NamedTempFile;

use crate::error::AppError;

const GEOLITE2_CITY_DOWNLOAD_URL: &str =
    "https://download.maxmind.com/geoip/databases/GeoLite2-City/download?suffix=tar.gz";

#[derive(Clone)]
pub struct MaxMindCredentials {
    account_id: String,
    license_key: String,
}

impl fmt::Debug for MaxMindCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MaxMindCredentials")
            .field("account_id", &"<redacted>")
            .field("license_key", &"<redacted>")
            .finish()
    }
}

impl MaxMindCredentials {
    pub fn new(
        account_id: Option<String>,
        license_key: Option<String>,
    ) -> Result<Option<Self>, AppError> {
        let account_id = account_id.map(|value| value.trim().to_string());
        let license_key = license_key.map(|value| value.trim().to_string());
        let account_id = account_id.filter(|value| !value.is_empty());
        let license_key = license_key.filter(|value| !value.is_empty());

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
    GEOLITE2_CITY_DOWNLOAD_URL.to_string()
}

pub async fn download_and_replace(
    credentials: &MaxMindCredentials,
    target_path: impl AsRef<Path>,
) -> Result<(), AppError> {
    let target_path = target_path.as_ref();
    let mut response = reqwest::Client::new()
        .get(download_url(credentials))
        .basic_auth(&credentials.account_id, Some(&credentials.license_key))
        .send()
        .await
        .map_err(|error| AppError::DownloadFailed(error.to_string()))?;

    if !response.status().is_success() {
        return Err(AppError::DownloadFailed(format!(
            "MaxMind download returned status {}",
            response.status()
        )));
    }

    let content_length = response.content_length();
    let mut archive_bytes = Vec::with_capacity(content_length.unwrap_or_default() as usize);
    let mut downloaded = 0_u64;

    eprintln!("downloading GeoLite2 City database");
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AppError::DownloadFailed(error.to_string()))?
    {
        downloaded += chunk.len() as u64;
        archive_bytes.extend_from_slice(&chunk);
        print_download_progress(downloaded, content_length);
    }
    eprintln!();

    let target_path = target_path.to_path_buf();
    let display_path = target_path.display().to_string();

    eprintln!("extracting GeoLite2 database");
    tokio::task::spawn_blocking(move || extract_mmdb_and_replace(&archive_bytes, target_path))
        .await
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))??;

    eprintln!("database saved to {display_path}");
    Ok(())
}

fn extract_mmdb_and_replace(
    archive_bytes: &[u8],
    target_path: impl AsRef<Path>,
) -> Result<(), AppError> {
    extract_mmdb_and_replace_with_validator(archive_bytes, target_path, validate_mmdb)
}

fn extract_mmdb_and_replace_with_validator(
    archive_bytes: &[u8],
    target_path: impl AsRef<Path>,
    validate: impl FnOnce(&Path) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let target_path = target_path.as_ref();
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;

    let decoder = GzDecoder::new(archive_bytes.as_ref());
    let mut archive = Archive::new(decoder);
    let mut temp_file = NamedTempFile::new_in(parent)
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
    let mut found_database = false;

    let entries = archive
        .entries()
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;

    for entry in entries {
        let mut entry = entry.map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
        let is_database = {
            let path = entry
                .path()
                .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
            path.extension()
                .is_some_and(|extension| extension == OsStr::new("mmdb"))
        };

        if is_database {
            io::copy(&mut entry, &mut temp_file)
                .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
            temp_file
                .flush()
                .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
            found_database = true;
            break;
        }
    }

    if !found_database {
        return Err(AppError::DatabaseUpdateFailed(
            "download archive did not contain an mmdb database".to_string(),
        ));
    }

    eprintln!("validating database");
    validate(temp_file.path())?;

    temp_file
        .persist(target_path)
        .map_err(|error| AppError::DatabaseUpdateFailed(error.error.to_string()))?;

    Ok(())
}

fn validate_mmdb(path: &Path) -> Result<(), AppError> {
    Reader::open_readfile(path)
        .map_err(|error| AppError::DatabaseUpdateFailed(error.to_string()))?;
    Ok(())
}

fn print_download_progress(downloaded: u64, content_length: Option<u64>) {
    match content_length {
        Some(total) if total > 0 => {
            let percent = downloaded as f64 / total as f64 * 100.0;
            eprint!(
                "\rdownloaded {} / {} ({percent:.1}%)",
                human_bytes(downloaded),
                human_bytes(total)
            );
        }
        _ => eprint!("\rdownloaded {}", human_bytes(downloaded)),
    }
    let _ = io::stderr().flush();
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];

    for next_unit in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }

    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Cursor;
    use tar::{Builder, Header};
    use tempfile::tempdir;

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

        assert!(matches!(
            result,
            Err(crate::error::AppError::MissingCredentials)
        ));
    }

    #[test]
    fn credentials_debug_redacts_license_key() {
        let credentials = MaxMindCredentials {
            account_id: "123".to_string(),
            license_key: "secret".to_string(),
        };

        let debug = format!("{credentials:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("123"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn treats_blank_credentials_as_missing() {
        let absent = MaxMindCredentials::new(Some("  ".to_string()), Some("\t".to_string()));
        let partial = MaxMindCredentials::new(Some("123".to_string()), Some(" ".to_string()));
        let trimmed =
            MaxMindCredentials::new(Some(" 123 ".to_string()), Some(" secret ".to_string()))
                .expect("trimmed credentials should be valid")
                .expect("trimmed credentials should be present");

        assert!(absent.expect("both blank should be absent").is_none());
        assert!(matches!(
            partial,
            Err(crate::error::AppError::MissingCredentials)
        ));
        assert_eq!(trimmed.account_id, "123");
        assert_eq!(trimmed.license_key, "secret");
    }

    #[test]
    fn tar_gz_with_mmdb_persists_expected_bytes_to_target() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("GeoLite2-City.mmdb");
        let archive =
            archive_with_entries(&[("GeoLite2-City_20260616/GeoLite2-City.mmdb", b"mmdb")]);

        extract_mmdb_and_replace_with_validator(&archive, &target, |_| Ok(())).unwrap();

        assert_eq!(fs::read(target).unwrap(), b"mmdb");
    }

    #[test]
    fn invalid_mmdb_validation_error_leaves_existing_target() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("GeoLite2-City.mmdb");
        fs::write(&target, b"existing").unwrap();
        let archive =
            archive_with_entries(&[("GeoLite2-City_20260616/GeoLite2-City.mmdb", b"not-mmdb")]);

        let result = extract_mmdb_and_replace_with_validator(&archive, &target, |_| {
            Err(AppError::DatabaseUpdateFailed("invalid mmdb".to_string()))
        });

        assert!(matches!(
            result,
            Err(crate::error::AppError::DatabaseUpdateFailed(_))
        ));
        assert_eq!(fs::read(target).unwrap(), b"existing");
    }

    #[test]
    fn tar_gz_without_mmdb_returns_update_error_and_leaves_existing_target() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("GeoLite2-City.mmdb");
        fs::write(&target, b"existing").unwrap();
        let archive = archive_with_entries(&[("GeoLite2-City_20260616/README.txt", b"readme")]);

        let result = extract_mmdb_and_replace(&archive, &target);

        assert!(matches!(
            result,
            Err(crate::error::AppError::DatabaseUpdateFailed(_))
        ));
        assert_eq!(fs::read(target).unwrap(), b"existing");
    }

    #[test]
    fn corrupt_archive_returns_update_error_and_leaves_existing_target() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("GeoLite2-City.mmdb");
        fs::write(&target, b"existing").unwrap();

        let result = extract_mmdb_and_replace(b"not a gzip archive", &target);

        assert!(matches!(
            result,
            Err(crate::error::AppError::DatabaseUpdateFailed(_))
        ));
        assert_eq!(fs::read(target).unwrap(), b"existing");
    }

    #[test]
    fn formats_download_progress_byte_counts() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(2 * 1024 * 1024), "2.0 MiB");
    }

    fn archive_with_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = Builder::new(encoder);

        for (path, contents) in entries {
            let mut header = Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, path, Cursor::new(*contents))
                .unwrap();
        }

        builder.finish().unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }
}
