# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.88
ARG DEBIAN_VERSION=bookworm

FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --locked --release

FROM debian:${DEBIAN_VERSION}-slim AS runtime

LABEL org.opencontainers.image.title="geoip" \
      org.opencontainers.image.description="GeoIP REST API backed by MaxMind GeoLite2 City" \
      org.opencontainers.image.source="https://github.com/superisaac/geoip" \
      org.opencontainers.image.licenses="MIT"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin geoip \
    && mkdir -p /data \
    && chown -R geoip:geoip /data

COPY --from=builder /app/target/release/geoip /usr/local/bin/geoip

USER geoip
WORKDIR /data

EXPOSE 5000

ENTRYPOINT ["/usr/local/bin/geoip"]
CMD ["serve", "--bind", "0.0.0.0:5000", "--db-path", "/data/GeoLite2-City.mmdb"]
