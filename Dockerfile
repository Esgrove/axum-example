# https://hub.docker.com/_/rust
FROM rust:latest AS builder
ARG DEPLOYMENT_TAG
WORKDIR /api

COPY Cargo.toml Cargo.lock build.rs .git ./
# Build and cache the dependencies
RUN mkdir src && echo "fn main() {println!(\"If you see this, something went wrong in Docker build\");}" > src/main.rs
RUN cargo fetch
RUN cargo build --release
RUN rm -f src/main.rs

# Copy the actual code files and build the application
COPY ./ ./
# Update the main file date so Cargo rebuilds it
RUN touch src/main.rs
RUN DEPLOYMENT_TAG=${DEPLOYMENT_TAG} cargo build \
    --release && \
    # target dir can be different depending on target platform / arch
    mv "$(find . -path "*/release/axum-example")" axum-example && \
    file axum-example

FROM ubuntu:jammy AS axum-runtime
RUN apt-get update && \
    apt-get install -y curl && \
    rm -rf /var/lib/apt/lists/*
# Copy Rust binary created in builder stage
COPY --from=builder api/axum-example /usr/local/bin/axum-example
# Start API
# Print backtrace when a panic occurs
ENV RUST_BACKTRACE=1
CMD ["axum-example", "--host", "0.0.0.0", "--port", "8080"]

HEALTHCHECK --interval=1m --timeout=3s \
    CMD curl -fs http://localhost/ || exit 1
