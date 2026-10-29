# syntax=docker/dockerfile:1.7
#
# Two-stage build.
# The builder caches compiled dependencies in their own layer,
# and the runtime image is a small distroless image running as a non-root user.

# https://hub.docker.com/_/rust
# Keep the Rust version in sync with `rust-version` in Cargo.toml.
FROM rust:1.99.1-trixie AS builder
WORKDIR /build

# Pre-build dependencies in a separate layer, so they stay cached when only `src/` changes.
# Build arguments that vary per build come after this layer,
# otherwise Docker would rebuild the dependencies even when Cargo.toml and Cargo.lock are unchanged.
COPY Cargo.toml Cargo.lock ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    mkdir src && \
    printf '%s\n' \
    '//! Dependency-only placeholder library for Docker layer caching.' > src/lib.rs && \
    printf '%s\n' \
    '//! Dependency-only placeholder binary for Docker layer caching.' \
    '' \
    'fn main() { eprintln!("dependency-only build, this binary should never run"); }' \
    > src/main.rs && \
    cargo fetch && \
    cargo build --release --bin axum-example && \
    rm -rf src target/release/axum-example target/release/axum-example.d \
    target/release/deps/axum_example-* target/release/deps/libaxum_example-* \
    target/release/.fingerprint/axum-example-*

# Copy the real source, and `.git` so the build script can read the commit and branch.
COPY build.rs ./
COPY src/ ./src/
COPY .git/ ./.git/

ARG DEPLOY_TAG=local
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    DEPLOY_TAG=${DEPLOY_TAG} cargo build --release --bin axum-example && \
    file target/release/axum-example

# https://github.com/GoogleContainerTools/distroless
# `cc-debian13` ships glibc, libgcc, and CA certificates, which is all the binary needs at runtime.
FROM gcr.io/distroless/cc-debian13:nonroot AS axum-runtime

COPY --from=builder /build/target/release/axum-example /usr/local/bin/axum-example

# Print a backtrace when a panic occurs.
ENV RUST_BACKTRACE=1
# Container platforms often inject PORT, and this default covers a plain `docker run`.
ENV PORT=8080
# Set `API_ENV` at deploy time, for example to `production`,
# which switches to JSON logs and hides the docs UIs.

EXPOSE 8080

# Bind on all interfaces so the container network can reach the listener.
# The distroless image has no shell or curl, so use the orchestrator's HTTP probe against `/health`.
CMD ["axum-example", "--host", "0.0.0.0"]
