# ============================================================
# Stage 1: Build
# ============================================================
FROM rust:1.94-slim-bookworm AS builder

ARG TARGETARCH

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install protoc (multi-arch)
RUN case "$TARGETARCH" in \
      amd64) PROTOC_ARCH="x86_64" ;; \
      arm64) PROTOC_ARCH="aarch_64" ;; \
      *) PROTOC_ARCH="x86_64" ;; \
    esac && \
    curl -OL "https://github.com/protocolbuffers/protobuf/releases/download/v29.5/protoc-29.5-linux-${PROTOC_ARCH}.zip" && \
    unzip "protoc-29.5-linux-${PROTOC_ARCH}.zip" -d /usr/local && \
    rm "protoc-29.5-linux-${PROTOC_ARCH}.zip"

WORKDIR /app

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY omem-server/ omem-server/

# Build with all features (glibc supports everything including Bedrock)
RUN cargo build --release -p omem-server

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim

LABEL org.opencontainers.image.source="https://github.com/ourmem/omem"
LABEL org.opencontainers.image.description="ourmem — Shared Memory That Never Forgets"
LABEL org.opencontainers.image.licenses="Apache-2.0"

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false -d /data omem \
    && mkdir -p /data && chown omem:omem /data

COPY --from=builder /app/target/release/omem-server /usr/local/bin/

VOLUME ["/data"]
WORKDIR /data
ENV RUST_LOG=info

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s \
    CMD curl -f http://localhost:8080/health || exit 1

USER omem

CMD ["omem-server"]
