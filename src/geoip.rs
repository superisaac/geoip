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

fn english_name(names: Option<&std::collections::BTreeMap<&str, &str>>) -> Option<String> {
    names
        .and_then(|names| names.get("en").copied())
        .map(ToOwned::to_owned)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::IpAddr, str::FromStr};

    #[tokio::test]
    async fn unloaded_service_reports_database_unavailable() {
        let service = GeoIpService::empty();
        let ip = IpAddr::from_str("8.8.8.8").unwrap();

        let err = service
            .lookup(ip)
            .await
            .expect_err("database should be missing");

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
