# https://hub.docker.com/_/rust
FROM rust:latest as builder
RUN USER=root cargo new --bin axum-example
WORKDIR /axum-example

COPY ./Cargo.toml ./Cargo.lock ./build.rs ./
COPY ./src ./src
RUN cargo install --path .

FROM rust:slim as axum-runtime
COPY --from=builder /usr/local/cargo/bin/axum-example /usr/local/bin/axum-example
CMD ["axum-example", "--host", "0.0.0.0", "--port", "80"]

HEALTHCHECK --interval=1m --timeout=3s \
    CMD curl -f http://localhost/ || exit 1
