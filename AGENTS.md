# AGENTS.md

Scoped to the Rust crate at `axum-example/`.
This file gives agent-specific guidance for this generic Axum REST API example.

## Project Overview

`axum-example` is a personal learning template for a generic Axum REST API.
It demonstrates production-shaped patterns while staying domain-neutral:
request routing, OpenAPI docs, typed schemas and errors, API-key protected admin routes,
structured logging, health and status checks, per-client rate limiting, graceful shutdown,
OpenTelemetry metrics, and an embedded Model Context Protocol (MCP) server.

The service is built on **axum 0.8** + **tokio** with Rust 2024 edition.
Runtime configuration comes from environment variables,
parsed once at startup into `Config` by `Config::from_env()`.
Invalid values fail startup with a `ConfigError` instead of silently falling back to defaults:

- `HOST` and `PORT` control the bind address.
- `API_ENV` controls the runtime environment. `production` switches to JSON logs and hides the docs UIs.
- `API_KEY` controls admin route authentication.
- `RATE_LIMIT_REQUESTS`, `RATE_LIMIT_WINDOW_SECONDS`, and `RATE_LIMIT_DISABLED` configure rate limiting.
- `REQUEST_TIMEOUT_SECONDS` sets the per-request timeout.
- `OTEL_EXPORTER_OTLP_ENDPOINT` or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` enables OTLP metric export.

Local telemetry should work without a collector.
The app always exposes Prometheus text metrics at `/metrics`,
while OTLP push export stays disabled unless an OTLP endpoint is configured.

## Build and Test Commands

After making code changes, always run:

```shell
cargo fmt
cargo clippy --all-targets -- -Dwarnings
cargo test
```

For coverage, run this when `cargo-llvm-cov` is installed:

```shell
cargo llvm-cov --all-targets
```

These commands are also represented in `.pre-commit-config.yaml` for `prek` / `pre-commit`.
It is faster to run the Cargo commands directly while iterating.

### Other useful commands

```shell
# Run the server locally on http://127.0.0.1:3000
cargo run --release

# Override log level
cargo run --release -- --log debug
RUST_LOG=debug cargo run --release

# Run one integration test file
cargo test --test mcp

# Smoke-test the running server, including /status, rate limit headers, and /mcp
./test-routes.sh

# Build and run the Docker image locally
./docker-run.sh

# Upgrade all crates, validate, and open a pull request
./update-crates.sh
```

## Code Organization

All Rust source files should be organised in this order:

1. Structs (public before private)
2. Enums (public before private)
3. Trait implementations and impl blocks (in the order structs/enums are defined)
4. Public functions
5. Private functions
6. Tests module

Within implementation blocks:

- Constructors and factory associated functions first
  (`new`, `default`, `from_*`, and builder-style `with_*` methods that return `Self`).
- After constructors, order the remaining items by visibility first, then by receiver kind.
  Visibility takes precedence over receiver kind:
  1. Public instance methods (`&self` / `&mut self`).
  2. Public associated functions (no `self`).
  3. Private instance methods.
  4. Private associated functions.

## Code Style and Conventions

Prefer [Semantic Line Breaks](https://sembr.org/) for Markdown prose, comments, and doc comments.
Treat this as a readability guideline, not a strict rule.
New sentences should go to a new line unless the full sentence is short.
Break text at natural sentence or clause boundaries.

- Rust 2024 edition.
- Clippy denies `pedantic` and `nursery`,
  plus a strict restriction list in `Cargo.toml`:
  `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `arithmetic_side_effects`,
  `string_slice`, `allow_attributes_without_reason`, and others.
  `clippy.toml` relaxes `unwrap`, `expect`, `panic`, and indexing inside `#[cfg(test)]` code.
  Integration test files in `tests/` opt out with a file-level `#![allow(..., reason = "...")]`.
- `unsafe_code` is denied and `missing_docs` warns, so every public item needs a doc comment.
- Do not use `unwrap()` or `expect()` in production code.
  Use proper error handling, or `.expect("...")` in tests with a useful message.
- Use saturating or checked arithmetic.
  `utils::increment_saturating` and `utils::decrement_saturating` cover atomic counters.
- Every `#[allow]` or `#[expect]` needs a `reason = "..."`.
  Prefer `#[expect]` so the attribute fails once it is no longer needed.
- Use `anyhow` for error propagation only in `main`.
  Library code uses typed errors from `src/errors.rs` built with `thiserror`.
- Route handlers return `Result<_, ApiError>`.
  `ApiError` renders every failure as the shared `ErrorResponse` JSON body,
  so do not build ad-hoc error tuples in handlers.
- Use `clap` derive macros for CLI argument parsing.
- Use `strum` derives (`EnumString`, `Display`) instead of hand-written `FromStr` / `Display` impls.
- Use `serde` derives for public response/request types.
- Use `utoipa::ToSchema` for OpenAPI-visible types so they show up in the docs UI.
- Use descriptive variable and function names. Prefer full names over abbreviations.
- Document public structs, enums, and functions with a doc comment explaining purpose
  and any non-obvious behavior or side effects.
- Avoid section divider or banner comments.
  Use a normal doc comment on the first item of a section,
  or split the section into its own module.
- All `use` imports go at the top of the file. Never import inside functions.

## Application Structure

The crate has a library target (`src/lib.rs`) and a thin binary (`src/main.rs`).
`main.rs` owns CLI parsing, logging bootstrap, config loading, process startup, and graceful shutdown.
Everything else lives in the library so the integration tests in `tests/` can import it.

Reusable application pieces live in focused modules:

- `src/config.rs` owns `Config`, `RateLimitConfig`, and env var parsing.
- `src/errors.rs` owns `ConfigError` and the `ApiError` route error type.
- `src/state.rs` owns `AppState`, the composition root shared with handlers as `SharedState`.
- `src/status.rs` owns `StatusService`: health checks, request counters, and env var presence reporting.
  It only reports env var names, never values.
- `src/router.rs` wires routes, middleware, docs, MCP, and fallback behavior.
  `build_router` is the HTTP surface,
  and `build_router_with_mcp` adds `/mcp` on top of it.
- `src/openapi.rs` owns `ApiDoc` and OpenAPI security metadata.
- `src/logging.rs` owns logging initialization and metadata-enriched logging macros.
- `src/middleware.rs` owns request telemetry middleware:
  request IDs, completion logs, and the `InProgressGuard` drop guard that keeps the in-flight gauge correct
  when a request future is cancelled.
- `src/rate_limit/` owns the sliding-window per-client limiter and its middleware.
- `src/mcp/` owns the embedded MCP server.
  Tools build internal requests and dispatch them through `build_router` with `oneshot`,
  so they share validation, rate limits, and metrics with HTTP traffic.
  When adding an HTTP route, consider adding a matching MCP tool.
- `src/telemetry.rs` owns OpenTelemetry instruments, OTLP export, and Prometheus rendering.
- `src/routing/routes.rs` owns public service routes
  such as `/`, `/health`, `/status`, `/metrics`, `/version`, and item routes.
- `src/routing/admin.rs` owns API-key protected admin routes.
- `src/schemas.rs` owns OpenAPI-visible request and response types.
- `src/types.rs` owns internal domain and runtime types such as `Environment`, `LogLevel`, `Item`, and the auth extractor.

Unit tests stay inline next to the code they exercise,
for example config parsing, limiter math, and MCP helper validation.
Route-level behavior is tested in `tests/` through the shared `tests/helpers` harness,
which drives the router in-process without a TCP listener.
New routes should get an integration test there,
and new routes need to be registered in `ApiDoc` so the OpenAPI spec test covers them.

## Layout

```text
axum-example/
├── Cargo.toml              # crate manifest, dependencies, lint settings
├── Cargo.lock              # locked Rust dependency graph
├── build.rs                # compile-time version/build metadata exporter
├── clippy.toml             # crate-local Clippy settings
├── Dockerfile              # cached dependency build and distroless runtime image
├── .cargo/config.toml      # cargo aliases
├── .pre-commit-config.yaml # prek / pre-commit hooks
├── README.md               # local development and operational notes
├── common.sh               # shared shell helpers for local scripts
├── docker-build.sh         # local container build helper
├── docker-run.sh           # local container run helper
├── test-routes.sh          # smoke-test script for a running local service
├── update-crates.sh        # dependency upgrade and pull request helper
├── src/
│   ├── lib.rs              # library root, module exports
│   ├── main.rs             # CLI, logging bootstrap, server bootstrap
│   ├── config.rs           # Config and RateLimitConfig from env vars
│   ├── errors.rs           # ConfigError and ApiError
│   ├── logging.rs          # structured logging macros and setup
│   ├── middleware.rs       # request telemetry middleware and InProgressGuard
│   ├── openapi.rs          # OpenAPI metadata and documentation assembly
│   ├── router.rs           # build_router and build_router_with_mcp
│   ├── schemas.rs          # OpenAPI-visible request and response types
│   ├── state.rs            # AppState and SharedState
│   ├── status.rs           # StatusService for /health and /status
│   ├── telemetry.rs        # OpenTelemetry instruments, OTLP, Prometheus output
│   ├── types.rs            # Environment, LogLevel, Item, ApiKeyExtractor
│   ├── utils.rs            # shared helpers
│   ├── version.rs          # compile-time constants populated by build.rs
│   ├── mcp/
│   │   ├── mod.rs          # MCP server, tools, resources, /mcp service
│   │   ├── helpers.rs      # path validation, internal requests, tool results
│   │   └── schemas.rs      # MCP tool parameter and response types
│   ├── rate_limit/
│   │   ├── mod.rs          # sliding-window limiter and middleware
│   │   └── config.rs       # exempt paths and fixed limiter constants
│   └── routing/
│       ├── mod.rs          # routing module root
│       ├── admin.rs        # API-key protected admin routes
│       └── routes.rs       # public service and item routes
└── tests/
    ├── helpers/mod.rs      # TestApp harness and request helpers
    ├── service_routes.rs   # root, health, status, version, metrics, docs
    ├── item_routes.rs      # item CRUD and validation
    ├── admin_routes.rs     # API key handling and admin routes
    ├── rate_limit.rs       # rate limit headers and 429 responses
    └── mcp.rs              # MCP initialize, tools, and resources
```

`build.rs` exports `BUILD_TIME`, `GIT_BRANCH`, `GIT_COMMIT`, `VERSION`, `RUST_VERSION`, and `DEPLOY_TAG`
as `env!` constants for `version.rs` to pick up.
