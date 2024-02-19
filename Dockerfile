# https://hub.docker.com/_/rust
FROM rust:latest as builder
WORKDIR /axum-example

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
RUN cargo install --path .

FROM ubuntu:jammy as axum-runtime
RUN apt-get update && \
    apt-get install -y curl && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/axum-example /usr/local/bin/axum-example
CMD ["axum-example", "--host", "0.0.0.0", "--port", "80"]

HEALTHCHECK --interval=1m --timeout=3s \
    CMD curl -fs http://localhost/ || exit 1
