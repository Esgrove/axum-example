# https://hub.docker.com/_/rust
FROM rust:latest as builder
WORKDIR /axum-example
COPY ./ ./
RUN cargo build --release && cargo install --path .

# TODO: use smaller (debian) image with a compatible glibc version
FROM rust:slim as axum-runtime
COPY --from=builder /usr/local/cargo/bin/axum-example /usr/local/bin/axum-example
CMD ["axum-example", "--host", "0.0.0.0", "--port", "80"]

HEALTHCHECK --interval=1m --timeout=3s \
    CMD curl -f http://localhost/ || exit 1
