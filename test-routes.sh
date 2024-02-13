#!/bin/bash
set -eo pipefail

get() {
    local url="$1"
    echo "GET: $1"
    response=$(curl -s -w "%{http_code}" -o response.json "$url")
    echo "Status code: $response"
    jq < response.json
    rm response.json
}

post() {
    local url="$1"
    local data="$2"
    echo "POST: $1 $2"
    response=$(curl -s -H "Content-Type: application/json" -d "$data" -w "%{http_code}" -o response.json "$url")
    echo "Status code: $response"
    jq < response.json
    rm response.json
}

if ! curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000" | grep -q '^2'; then
    echo "Failed to call API, is it running?"
    exit 1
fi

get http://127.0.0.1:3000
get http://127.0.0.1:3000/version
get http://127.0.0.1:3000/user?username=pizzalover9000
post http://127.0.0.1:3000/users '{"username":"esgrove"}'
get http://127.0.0.1:3000/user?username=esgrove
post http://127.0.0.1:3000/users '{"username":"esgrove"}'
