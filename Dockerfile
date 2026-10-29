# https://hub.docker.com/_/rust
FROM rust:latest AS builder
ARG DEPLOY_TAG
WORKDIR /api

COPY Cargo.toml Cargo.lock build.rs ./
# Git metadata for the build script
COPY .git/ ./.git/
# Build and cache the dependencies
RUN mkdir src && \
    echo "fn main() {println!(\"If you see this, something went wrong in Docker build\");}" > src/main.rs && \
    echo "" > src/lib.rs
RUN cargo fetch
RUN cargo build --release
RUN rm -f src/main.rs src/lib.rs

# Copy the actual code files and build the application
COPY ./ ./
# Update the source file dates so Cargo rebuilds them
RUN touch src/main.rs src/lib.rs
RUN DEPLOY_TAG=${DEPLOY_TAG} cargo build \
    --release && \
    # target dir can be different depending on target platform / arch
    mv "$(find . -path "*/release/axum-example")" axum-example && \
    file axum-example

FROM ubuntu:latest AS axum-runtime
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
    CMD curl -fs http://localhost:8080/health || exit 1
