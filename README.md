# Axum REST API example

Basic example and learning exercise for an [Axum](https://github.com/tokio-rs/axum) REST API.

## Running locally

Usage:

```console
Rust Axum REST API example.

Usage: axum-example [OPTIONS]

Options:
      --host <HOST>  Optional host IP to listen to (for example "0.0.0.0")
  -l, --log <LEVEL>  Log level to use [possible values: trace, debug, info, warn, error]
  -p, --port <PORT>  Optional port number to use (default is 3000)
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

Docker:

```shell
./run.sh
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

curl -s http://127.0.0.1:3000/user?username=akseli | jq .
curl -s http://127.0.0.1:3000/user?username=pizzalover9000 | jq .

curl -s -H "Content-Type: application/json" -d '{"username":"test"}' http://127.0.0.1:3000/users | jq .
```

### OpenAPI documentation

Uses [utoipa](https://github.com/juhaku/utoipa) to generate OpenAPI documentation and UIs.

Swagger UI is available at `/doc`, Redoc at `/redoc`, and RapiDoc at `/rapidoc`.

The raw JSON can be seen from `/api-docs/openapi.json`.
