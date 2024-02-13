# Axum REST API Template

Simple example for using the Axum framework for a REST API.

<https://github.com/tokio-rs/axum>

## Running locally

Usage:

```console
Rust Axum REST API example.

Usage: axum-example [OPTIONS]

Options:
  -p, --port <PORT>  Optional port number to use (default is 3000)
  -l, --log <LEVEL>  Log level to use [possible values: trace, debug, info, warn, error]
  -v, --version      Print version info and exit
  -h, --help         Print help (see more with '--help')
```

Start server:

```shell
# default
cargo run --release

# log level arg
cargo run --release -- --log error

# log level from env
RUST_LOG=debug cargo run --release
```

Test routes:

```shell
curl -s http://127.0.0.1:3000 | jq .

curl -s http://127.0.0.1:3000/version | jq .

curl -s http://127.0.0.1:3000/user?username=akseli | jq .
curl -s http://127.0.0.1:3000/user?username=pizzalover9000 | jq .

curl -s -H "Content-Type: application/json" -d '{"username":"test"}' http://127.0.0.1:3000/users | jq .
```
