# GeoIP REST API Design

## Goal

Build a small Rust + axum REST API that looks up geographic location data for one or more IP addresses using a MaxMind GeoLite2 City `.mmdb` database. The service must also support downloading and hot-loading the latest GeoLite2 database with MaxMind credentials.

## Scope

In scope:

- Serve HTTP endpoints with axum.
- Query a local GeoLite2 City `.mmdb` database.
- Support single-IP and batch-IP lookup.
- Download the latest GeoLite2 City database using `MAXMIND_ACCOUNT_ID` and `MAXMIND_LICENSE_KEY`.
- Keep an existing usable database if a refresh fails.
- Include focused unit and handler tests.

Out of scope:

- Authentication for admin endpoints.
- Persistent query history.
- GeoLite2 ASN or Country databases.
- Rate limiting.
- Docker packaging.

## Configuration

The service will read configuration from environment variables:

- `BIND_ADDR`: HTTP bind address. Default: `0.0.0.0:3000`.
- `GEOIP_DB_PATH`: local database path. Default: `data/GeoLite2-City.mmdb`.
- `GEOIP_UPDATE_ON_STARTUP`: whether to download the database on startup if needed. Default: `true`.
- `GEOIP_UPDATE_INTERVAL_HOURS`: background refresh interval. Default: `24`; `0` disables background refresh.
- `MAXMIND_ACCOUNT_ID`: required for database download.
- `MAXMIND_LICENSE_KEY`: required for database download.

Lookup endpoints can run without MaxMind credentials if a valid local database already exists. Download attempts require both credentials.

## HTTP API

### `GET /health`

Returns service status and whether the database is loaded.

Example response:

```json
{
  "status": "ok",
  "database_loaded": true
}
```

### `GET /lookup/{ip}`

Looks up one IP address.

Success response:

```json
{
  "ip": "8.8.8.8",
  "country": {
    "iso_code": "US",
    "name": "United States"
  },
  "city": {
    "name": "Mountain View"
  },
  "location": {
    "latitude": 37.386,
    "longitude": -122.0838,
    "time_zone": "America/Los_Angeles"
  },
  "subdivisions": [
    {
      "iso_code": "CA",
      "name": "California"
    }
  ]
}
```

Errors:

- `400 Bad Request` for invalid IP syntax.
- `404 Not Found` when the database has no record for the IP.
- `503 Service Unavailable` when no database is loaded.

### `POST /lookup`

Looks up multiple IP addresses.

Request:

```json
{
  "ips": ["8.8.8.8", "1.1.1.1"]
}
```

Response:

```json
{
  "results": [
    {
      "ip": "8.8.8.8",
      "location": {
        "country": {
          "iso_code": "US",
          "name": "United States"
        }
      }
    },
    {
      "ip": "bad-ip",
      "error": {
        "code": "invalid_ip",
        "message": "invalid IP address"
      }
    }
  ]
}
```

Batch lookup validates each IP independently. A single invalid or missing IP does not fail the entire batch unless the request body itself is malformed or the database is unavailable.

### `POST /admin/database/update`

Downloads the latest GeoLite2 City database, extracts the `.mmdb`, atomically replaces the configured database path, and hot-loads the new reader.

Success response:

```json
{
  "updated": true,
  "database_loaded": true
}
```

Errors:

- `400 Bad Request` when MaxMind credentials are missing.
- `502 Bad Gateway` when the MaxMind download fails.
- `500 Internal Server Error` for local file, archive, or database load failures.

If an update fails after an old database has already been loaded, the old database remains active.

## Architecture

The service will use small modules with explicit responsibilities:

- `config`: parse environment configuration and defaults.
- `error`: shared application error type and HTTP response mapping.
- `geoip`: own the loaded MaxMind reader and expose lookup methods.
- `download`: download and extract the GeoLite2 archive, then atomically replace the local mmdb.
- `routes`: define axum handlers and request/response DTOs.
- `main`: wire configuration, state, startup update, background refresh, and axum server.

Application state will hold a clonable GeoIP service backed by an async-safe lock around the current database reader. Lookups take a read lock; successful updates swap in a newly opened reader.

## Data Flow

Startup:

1. Load configuration from environment.
2. Try to load `GEOIP_DB_PATH` if it exists.
3. If `GEOIP_UPDATE_ON_STARTUP=true` and the database is missing, download the latest database.
4. If download succeeds, load the downloaded database.
5. Start background refresh when `GEOIP_UPDATE_INTERVAL_HOURS > 0`.
6. Start the axum server.

Lookup:

1. Parse the IP address.
2. Read the current database reader.
3. Query the GeoLite2 City record.
4. Convert available MaxMind fields into stable JSON response fields.

Update:

1. Validate credentials.
2. Download the `.tar.gz` archive from MaxMind.
3. Extract the `.mmdb` file into a temporary path.
4. Open the new database to validate it.
5. Atomically replace the configured database file.
6. Swap the active in-memory reader.

## Error Handling

Errors will be mapped to stable JSON responses:

```json
{
  "error": {
    "code": "database_unavailable",
    "message": "GeoIP database is not loaded"
  }
}
```

Expected error codes:

- `invalid_ip`
- `not_found`
- `database_unavailable`
- `missing_credentials`
- `download_failed`
- `database_update_failed`
- `bad_request`
- `internal_error`

## Testing

Tests will be written before implementation changes.

Coverage targets:

- Configuration defaults and environment overrides.
- IP parsing and single lookup handler behavior.
- Batch lookup per-item error behavior.
- Database unavailable behavior.
- Download/update behavior using an injectable downloader or local fixture instead of real MaxMind network access.
- Archive extraction chooses the `.mmdb` file and preserves the old database on failure.

Integration tests should avoid requiring live MaxMind credentials. Live download can be verified manually by running the service with real credentials.

## Acceptance Criteria

- The project builds as a Rust axum service.
- `GET /health`, `GET /lookup/{ip}`, `POST /lookup`, and `POST /admin/database/update` are implemented.
- The service can use an existing GeoLite2 City `.mmdb`.
- The service can download and hot-load the latest GeoLite2 City database when credentials are configured.
- A failed update does not replace a working database.
- Tests cover the core request validation, lookup behavior, and update failure behavior.
