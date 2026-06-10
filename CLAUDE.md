# CLAUDE.md

Scoped to the Rust crate at `axum-example/`.
This file gives agent-specific guidance for this generic Axum REST API example.

## Project Overview

`axum-example` is a personal learning template for a generic Axum REST API.
It demonstrates production-shaped patterns while staying domain-neutral:
request routing, OpenAPI docs, typed schemas, API-key protected admin routes,
structured logging, health checks, graceful shutdown, and OpenTelemetry metrics.

The service is built on **axum 0.8** + **tokio** with Rust 2024 edition.
Runtime configuration comes from environment variables:

- `HOST` and `PORT` control the bind address.
- `API_ENV` controls the runtime environment.
- `API_KEY` controls admin route authentication.
- `OTEL_EXPORTER_OTLP_ENDPOINT` or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT`
  enables OTLP metric export.

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

# Smoke-test the running server
./test-routes.sh

# Build and run the Docker image locally
./docker-run.sh
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
- Clippy is configured with `pedantic` and `nursery` lints enabled,
  plus `unwrap_used = "deny"` with `allow-unwrap-in-tests = true` in `clippy.toml`.
- Do not use plain `unwrap()` in production code.
  Use proper error handling, or `.expect("...")` in tests with a useful message.
- Use `anyhow` for error propagation in `main` and binary entrypoints.
- Use `clap` derive macros for CLI argument parsing.
- Use `strum` derives (`EnumString`, `Display`) instead of hand-written `FromStr` / `Display` impls.
- Use `serde` derives for public response/request types.
- Use `utoipa::ToSchema` for OpenAPI-visible types so they show up in the docs UI.
- Use descriptive variable and function names.
  Prefer full names over abbreviations.
- Document public structs, enums, and functions with a doc comment explaining purpose
  and any non-obvious behavior or side effects.
- Avoid section divider or banner comments.
  Use a normal doc comment on the first item of a section,
  or split the section into its own module.
- All `use` imports go at the top of the file.
  Never import inside functions.

## Application Structure

The project is currently a single binary crate.
Keep `src/main.rs` thin:
it should own CLI parsing, logging bootstrap, process startup, and graceful shutdown wiring.

Reusable application pieces live in focused modules:

- `src/router.rs` wires routes, middleware, docs, and fallback behavior.
- `src/openapi.rs` owns `ApiDoc` and OpenAPI security metadata.
- `src/logging.rs` owns logging initialization and metadata-enriched logging macros.
- `src/middleware.rs` owns request telemetry middleware.
- `src/telemetry.rs` owns OpenTelemetry instruments, OTLP export, and Prometheus rendering.
- `src/routing/routes.rs` owns public service routes such as `/`, `/health`, `/metrics`, `/version`, and item routes.
- `src/routing/admin.rs` owns API-key protected admin routes.
- `src/schemas.rs` owns OpenAPI-visible request and response types.
- `src/types.rs` owns shared application state, config, environment, and auth extractor types.

Only introduce a library target if integration tests or downstream examples need to import
the router and shared types from outside the binary crate.
Until then, keep unit tests close to the modules they exercise.

## Layout

```text
axum-example/
├── Cargo.toml              # crate manifest, dependencies, lint settings
├── Cargo.lock              # locked Rust dependency graph
├── build.rs                # compile-time version/build metadata exporter
├── clippy.toml             # crate-local Clippy settings
├── .pre-commit-config.yaml # prek / pre-commit hooks
├── README.md               # local development and operational notes
├── common.sh               # shared shell helpers for local scripts
├── docker-run.sh           # local container run helper
├── test-routes.sh          # smoke-test script for a running local service
├── src/
│   ├── main.rs             # CLI, logging bootstrap, server bootstrap
│   ├── logging.rs          # structured logging macros and setup
│   ├── middleware.rs       # request telemetry middleware
│   ├── openapi.rs          # OpenAPI metadata and documentation assembly
│   ├── router.rs           # build_router: routes, middleware, docs, fallback
│   ├── schemas.rs          # OpenAPI-visible request and response types
│   ├── telemetry.rs        # OpenTelemetry instruments, OTLP, Prometheus output
│   ├── types.rs            # Environment, LogLevel, AppState, SharedState
│   ├── utils.rs            # shared helpers
│   ├── version.rs          # compile-time constants populated by build.rs
│   └── routing/
│       ├── admin.rs        # API-key protected admin routes
│       └── routes.rs       # public service and item routes
```

`build.rs` exports `BUILD_TIME`, `GIT_BRANCH`, `GIT_COMMIT`, `VERSION`, `RUST_VERSION`, and `DEPLOY_TAG`
as `env!` constants for `version.rs` to pick up.
