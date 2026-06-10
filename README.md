# Axum REST API example

Personal example and learning exercise for an [Axum](https://github.com/tokio-rs/axum) REST API.

Features:

- OpenAPI documentation using [utoipa](https://github.com/juhaku/utoipa)
- API key authentication for admin routes with a custom extractor
- Anyhow error handling support in routes
- Custom JSON rejection error handling
- OpenTelemetry HTTP metrics with optional OTLP export and Prometheus `/metrics` scraping
- Structured logs with build metadata
- JSON fallback body for unknown paths

## Running locally

Usage:

```console
Rust Axum REST API example.

Usage: axum-example [OPTIONS]

Options:
      --host <IP>    Optional host IP to listen to (for example "0.0.0.0") [env: HOST=]
  -l, --log <LEVEL>  Log level to use [default: info] [possible values: trace, debug, info, warn, error]
  -p, --port <PORT>  Optional port number to use [env: PORT=] [default: 3000]
  -v, --version      Print version info and exit
  -h, --help         Print help
```

### Start server

Run locally:

```shell
cargo run --release

# Specify log level
cargo run --release -- --log error

# log level from env
RUST_LOG=debug cargo run --release
```

Build Docker image and run container:

```shell
./docker-run.sh
```

### Test routes

Start the server first and then in another terminal (tab):

```shell
./test-routes.sh
```

Or manually:

```shell
curl -s http://127.0.0.1:3000 | jq .

curl -s http://127.0.0.1:3000/health | jq .

curl -s http://127.0.0.1:3000/version | jq .

curl -s http://127.0.0.1:3000/metrics

curl -s http://127.0.0.1:3000/item?name=akseli | jq .
curl -s http://127.0.0.1:3000/item?name=pizzalover9000 | jq .

curl -s -H "Content-Type: application/json" -d '{"name":"test"}' http://127.0.0.1:3000/items | jq .

curl -s 'http://127.0.0.1:3000/items?skip=1&limit=10' | jq .
```

### OpenAPI documentation

Swagger UI is available at `/doc`,
Redoc at `/redoc`,
RapiDoc at `/rapidoc`,
and Scalar at `/scalar`.

The raw JSON can be seen from `/api-docs/openapi.json`.

### Telemetry

The app records generic HTTP metrics with OpenTelemetry instruments.
Prometheus text output is available from `/metrics`.

OTLP export is disabled by default.
Set one of these env vars to push metrics to an OpenTelemetry collector:

```shell
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317 cargo run --release
OTEL_EXPORTER_OTLP_METRICS_ENDPOINT=http://127.0.0.1:4317 cargo run --release
```

Metrics use low-cardinality labels such as `method`, `route`, `status_class`, and `status_code`.
The route label comes from Axum's matched route pattern instead of the raw request URL.

Exported metric names include:

- `axum_example_http_requests_started_total`
- `axum_example_http_requests_completed_total`
- `axum_example_http_request_duration_ms`
- `axum_example_http_in_progress_requests`
- `axum_example_http_errors_total`

## Development

Run the normal verification loop before committing:

```shell
cargo fmt
cargo clippy --all-targets -- -Dwarnings
cargo test
```

For coverage, install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```shell
cargo install cargo-llvm-cov
cargo llvm-cov --all-targets
cargo llvm-cov --all-targets --html
```

Coverage is intentionally not part of the default pre-commit hook because it is slower than formatting,
linting, and unit tests.

### Pre-Commit Hooks

This repository uses a `.pre-commit-config.yaml` that can be run by either `prek` or `pre-commit`.
With `prek`:

```shell
brew install prek
prek install -f
prek run -a
```
