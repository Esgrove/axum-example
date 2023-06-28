# Axum REST API Template

<https://github.com/tokio-rs/axum>

## Running locally

Start server:

```shell
RUST_LOG=info cargo run --release
```

Test routes:

```shell
curl -s http://127.0.0.1:3000 | jq .

curl -s -H "Content-Type: application/json" -d '{"username":"test"}' http://127.0.0.1:3000/users | jq .
```
