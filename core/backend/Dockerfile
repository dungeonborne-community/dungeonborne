# Stage 1: Build
FROM rust:1.82-slim-bookworm AS builder
WORKDIR /app

# Install build dependencies (if system libs are needed)
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

# If you switched Cargo.toml to protocol-ws, the build takes ~1 minute
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary and the assets folder (Network.pb, Configure.pb, cert.pem, key.pem, dump_*.bin)
COPY --from=builder /app/target/release/dbserver-rs /app/dbserver-rs
COPY assets /app/assets

ENV BIND_IP=0.0.0.0
ENV ASSETS_DIR=/app/assets
ENV DB_PATH=surrealdb-service:8000

EXPOSE 80 443 8085 6601 6602 6603 6604

CMD ["/app/dbserver-rs"]