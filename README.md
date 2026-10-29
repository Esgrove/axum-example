# Axum REST API example

Personal example and learning exercise for an [Axum](https://github.com/tokio-rs/axum) REST API.

Features:

- OpenAPI documentation using [utoipa](https://github.com/juhaku/utoipa) with Swagger UI, Redoc, RapiDoc, and Scalar
- API key authentication for admin routes with a custom extractor
- Typed `ApiError` that renders every failure as the same JSON error body
- Typed runtime configuration from environment variables with startup validation
- Structured `/health` and detailed `/status` endpoints
- Sliding-window per-client rate limiting with standard `RateLimit-*` headers
- Request ID propagation and structured request completion logs
- OpenTelemetry HTTP metrics with optional OTLP export and Prometheus `/metrics` scraping
- Embedded [Model Context Protocol](https://modelcontextprotocol.io) server at `/mcp`
- Library and binary split with an integration test suite under `tests/`
- Strict Clippy lints, `unsafe` denied, and missing docs warned
## Running locally

Usage:

```console
Axum API example

Usage: axum-example [OPTIONS]

Options:
  -H, --host <IP>    Optional host IP to listen on (for example "0.0.0.0") [env: HOST=]
  -l, --log <LEVEL>  Log level to use [default: info] [possible values: trace, debug, info, warn, error]
  -p, --port <PORT>  TCP port number to listen on [env: PORT=] [default: 3000]
  -v, --version      Print version info and exit
  -h, --help         Print help (see more with '--help')
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

### Configuration

Runtime configuration is read from environment variables at startup.
Invalid values stop the server with a descriptive error instead of falling back silently.

| Variable                              | Default        | Description                                                         |
| ------------------------------------- | -------------- | ------------------------------------------------------------------- |
| `HOST`                                | `127.0.0.1`    | Listen address, same as `--host`.                                   |
| `PORT`                                | `3000`         | Listen port, same as `--port`.                                      |
| `API_ENV`                             | `local`        | Runtime environment. `production` uses JSON logs and hides doc UIs. |
| `API_KEY`                             | `axum-api-key` | API key required by the `/admin` routes.                            |
| `RATE_LIMIT_REQUESTS`                 | `300`          | Maximum requests per client in one window.                          |
| `RATE_LIMIT_WINDOW_SECONDS`           | `60`           | Sliding window length in seconds.                                   |
| `RATE_LIMIT_DISABLED`                 | `false`        | Set to `true` to turn rate limiting off.                            |
| `REQUEST_TIMEOUT_SECONDS`             | `10`           | Timeout applied to every request.                                   |
| `OTEL_EXPORTER_OTLP_ENDPOINT`         | unset          | Enables OTLP metric export.                                         |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | unset          | Enables OTLP metric export with a metrics-specific endpoint.        |

### Endpoints

| Method   | Path                   | Description                                                     |
| -------- | ---------------------- | --------------------------------------------------------------- |
| `GET`    | `/`                    | Service name and current timestamp.                             |
| `GET`    | `/health`              | Overall health, version, and uptime.                            |
| `GET`    | `/status`              | Individual checks, request counters, config, and env var names. |
| `GET`    | `/version`             | Build metadata.                                                 |
| `GET`    | `/metrics`             | Prometheus text metrics.                                        |
| `GET`    | `/item?name=...`       | Look up one item by name.                                       |
| `GET`    | `/items`               | List items with optional `skip` and `limit`.                    |
| `POST`   | `/items`               | Create an item.                                                 |
| `GET`    | `/items/{name}`        | Get one item.                                                   |
| `PUT`    | `/items/{name}`        | Update an item.                                                 |
| `DELETE` | `/admin/remove/{name}` | Remove an item, requires the `api-key` header.                  |
| `DELETE` | `/admin/clear_items`   | Remove all items, requires the `api-key` header.                |
| `POST`   | `/mcp`                 | Model Context Protocol endpoint.                                |

Errors use one JSON shape across all routes:

```json
{ "error": "Not Found", "message": "Item does not exist: missing" }
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

curl -s http://127.0.0.1:3000/status | jq .

curl -s http://127.0.0.1:3000/version | jq .

curl -s http://127.0.0.1:3000/metrics

curl -s http://127.0.0.1:3000/item?name=akseli | jq .
curl -s http://127.0.0.1:3000/item?name=pizzalover9000 | jq .

curl -s -H "Content-Type: application/json" -d '{"name":"test"}' http://127.0.0.1:3000/items | jq .

curl -s 'http://127.0.0.1:3000/items?skip=1&limit=10' | jq .

curl -s -X DELETE -H "api-key: axum-api-key" http://127.0.0.1:3000/admin/clear_items | jq .
```

### Rate limiting

Every route except `/health` and `/metrics` is limited per client with a sliding window.
The client is identified by the rightmost `X-Forwarded-For` address,
which is the one appended by the nearest trusted proxy,
falling back to the socket peer address.

Responses include standard rate limit headers:

```text
RateLimit-Limit: 300
RateLimit-Remaining: 299
RateLimit-Reset: 60
```

Rejected requests get `429 Too Many Requests` with a `Retry-After` header.
State is in-memory and per process,
so running several replicas needs a shared store such as Redis.

### MCP server

The service embeds a [Model Context Protocol](https://modelcontextprotocol.io) server at `/mcp`.
It uses the Streamable HTTP transport, runs stateless, and returns plain JSON responses.

Service tools are `health`, `status`, `version`, and `metrics`.
Item tools are `list_items`, `get_item`, `create_item`, `update_item`, and `delete_item`.
Resources are `axum-example://service/metadata` and `axum-example://openapi.json`.

Each tool call builds an internal HTTP request and sends it through the same router as normal traffic,
so validation, rate limits, metrics, and error responses match the HTTP routes exactly.
`delete_item` forwards to the admin route and takes the API key as an `apiKey` argument,
so MCP clients need the same credential as HTTP clients.

Example with curl:

```shell
curl -s http://127.0.0.1:3000/mcp \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_items","arguments":{}}}' | jq .
```

Add it to an MCP client such as Cursor:

```json
{
  "mcpServers": {
    "axum-example": {
      "url": "http://127.0.0.1:3000/mcp"
    }
  }
}
```

### OpenAPI documentation

Swagger UI is available at `/doc`,
Redoc at `/redoc`,
RapiDoc at `/rapidoc`,
and Scalar at `/docs` with a `/scalar` alias.

The raw JSON can be seen from `/api-docs/openapi.json`.
Documentation UIs are not mounted when `API_ENV=production`.

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
- `axum_example_http_rate_limited_total`

The in-progress gauge uses a drop guard,
so it is decremented even when a client disconnects and the request future is cancelled.

Every response carries an `x-request-id` header.
An incoming `x-request-id` is reused, otherwise a new one is generated.
Request completion logs include the request ID, method, matched route, status, and latency.

## Development

Run the normal verification loop before committing:

```shell
cargo fmt
cargo clippy --all-targets -- -Dwarnings
cargo test
```

Integration tests live in `tests/` and drive the router in-process without opening a TCP listener:

| File                      | Covers                                         |
| ------------------------- | ---------------------------------------------- |
| `tests/service_routes.rs` | root, health, status, version, metrics, docs   |
| `tests/item_routes.rs`    | item CRUD and validation                       |
| `tests/admin_routes.rs`   | API key handling and admin routes              |
| `tests/rate_limit.rs`     | rate limit headers, 429 responses, exemptions  |
| `tests/mcp.rs`            | MCP initialize, tools, resources, rate limits  |

For coverage, install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```shell
cargo install cargo-llvm-cov
cargo llvm-cov --all-targets
cargo llvm-cov --all-targets --html
```

Coverage is intentionally not part of the default pre-commit hook because it is slower than formatting,
linting, and unit tests.

### Updating dependencies

`update-crates.sh` upgrades all crates, runs formatting, Clippy, and tests,
and opens a pull request with the changes:

```shell
./update-crates.sh
# Only update and validate locally
./update-crates.sh --dryrun
```

### Pre-Commit Hooks

This repository uses a `.pre-commit-config.yaml` that can be run by either `prek` or `pre-commit`.
With `prek`:

```shell
brew install prek
prek install -f
prek run -a
```
