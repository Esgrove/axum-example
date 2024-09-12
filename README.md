# Axum REST API example

Personal example and learning exercise for an [Axum](https://github.com/tokio-rs/axum) REST API.

Features:

- API key authentication for admin routes with a custom extractor
- Anyhow error handling support in routes
- Custom JSON rejection error handling
- OpenAPI documentation using [utoipa](https://github.com/juhaku/utoipa)

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

curl -s http://127.0.0.1:3000/version | jq .

curl -s http://127.0.0.1:3000/item?name=akseli | jq .
curl -s http://127.0.0.1:3000/item?name=pizzalover9000 | jq .

curl -s -H "Content-Type: application/json" -d '{"name":"test"}' http://127.0.0.1:3000/items | jq .
```

### OpenAPI documentation

Swagger UI is available at `/doc`,
Redoc at `/redoc`,
RapiDoc at `/rapidoc`,
and Scalar at `/scalar`.

The raw JSON can be seen from `/api-docs/openapi.json`.

## TODO

- Metrics with full instrumentation, for example OpenTelemetry or Prometheus
- Use https://github.com/ProbablyClem/utoipauto for automatic OpenAPI generation
