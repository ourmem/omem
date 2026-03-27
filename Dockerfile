# ============================================================
# Stage 1: Build
# ============================================================
FROM rust:1.94-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install protoc 29.5 (Debian bookworm default is too old for lancedb)
RUN curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v29.5/protoc-29.5-linux-x86_64.zip \
    && unzip protoc-29.5-linux-x86_64.zip -d /usr/local \
    && rm protoc-29.5-linux-x86_64.zip

WORKDIR /app

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY omem-server/ omem-server/

RUN cargo build --release -p omem-server

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/omem-server /usr/local/bin/

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s \
    CMD curl -f http://localhost:8080/health || exit 1

ENV RUST_LOG=info

CMD ["omem-server"]
