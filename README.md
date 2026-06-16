# GeoIP

A Rust + axum REST API for looking up geographic location data for one or more IP addresses using a MaxMind GeoLite2 City `.mmdb` database.

## Commands

Serve the API:

```bash
geoip serve
```

Use a custom database path:

```bash
geoip serve --bind 127.0.0.1:3000 --db-path data/GeoLite2-City.mmdb
```

Download the database before serving when the database path does not exist:

```bash
MAXMIND_ACCOUNT_ID=123 \
MAXMIND_LICENSE_KEY=your-license-key \
geoip serve --download
```

Download the GeoLite2 City database:

```bash
MAXMIND_ACCOUNT_ID=123 \
MAXMIND_LICENSE_KEY=your-license-key \
geoip download --db-path data/GeoLite2-City.mmdb
```

`--bind` defaults to `0.0.0.0:5000`. `--db-path` defaults to `data/GeoLite2-City.mmdb`. `--download` is disabled unless the flag is present.

Both commands read environment variables from a `.env` file in the current directory. Existing shell environment variables take precedence.

Supported environment variables:

```text
MAXMIND_ACCOUNT_ID=123
MAXMIND_LICENSE_KEY=your-license-key
GEOIP_BEARER_TOKEN=your-api-token
```

When `GEOIP_BEARER_TOKEN` is set to a non-empty value, every API request must include:

```text
Authorization: Bearer your-api-token
```

## API

Health:

```bash
curl http://localhost:5000/health
```

Single lookup:

```bash
curl http://localhost:5000/lookup/8.8.8.8
```

Batch lookup:

```bash
curl -X POST http://localhost:5000/lookup \
  -H 'content-type: application/json' \
  -d '{"ips":["8.8.8.8","1.1.1.1","bad-ip"]}'
```

## Development

```bash
cargo test
```
