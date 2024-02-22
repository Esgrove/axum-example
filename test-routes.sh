#!/bin/bash
set -eo pipefail

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=../common.sh
source "$DIR/common.sh"

USAGE="Usage: $0 [OPTIONS]

Test API routes.
Port number can be set with env variable: PORT=3000 $0

OPTIONS: All options are optional
    -h | --help
        Display these instructions.

    -v | --verbose
        Display commands being executed."

while [ $# -gt 0 ]; do
    case "$1" in
        -h | --help)
            print_usage_and_exit
            ;;
        -v | --verbose)
            set -x
            ;;
    esac
    shift
done

PORT=${PORT:-3000}

get() {
    local url="$1"
    print_magenta "GET: $1"
    response=$(curl -s -w "%{http_code}" -o response.json "$url")
    print_response "$response"
}

post() {
    local url="$1"
    local data="$2"
    print_magenta "POST: $1 $2"
    response=$(curl -s -X POST -H "Content-Type: application/json" -d "$data" -w "%{http_code}" -o response.json "$url")
    print_response "$response"
}

delete() {
    local url="$1"
    print_magenta "DELETE: $1"
    response=$(curl -s -X DELETE -w "%{http_code}" -o response.json "$url")
    print_response "$response"
}

print_response() {
    local response="$1"
    if echo "$response" | grep -q '^2'; then
        echo "Status code: $(green "$response")"
    elif echo "$response" | grep -q '^4'; then
        echo "Status code: $(red "$response")"
    else
        echo "Status code: $response"
    fi
    output=$(jq --color-output < response.json)
    if [ "$(echo "$output" | wc -l)" -gt 1 ]; then
        echo "Response:"
        echo "$output"
    else
        echo "Response: $output"
    fi
    rm response.json
}

if ! curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:$PORT" | grep -q '^2'; then
    print_error_and_exit "Failed to call API, is it running?"
fi

get "http://127.0.0.1:$PORT"
get "http://127.0.0.1:$PORT/version"
get "http://127.0.0.1:$PORT/list_users"
post "http://127.0.0.1:$PORT/users" '{"username":"esgrove"}'
get "http://127.0.0.1:$PORT/user?username=esgrove"
post "http://127.0.0.1:$PORT/users" '{"username":"esgrove"}'
get "http://127.0.0.1:$PORT/user?username=pizzalover9000"

for name in pizzalover9000 akseli swanson; do
    post "http://127.0.0.1:$PORT/users" "{\"username\":\"$name\"}"
done

get "http://127.0.0.1:$PORT/list_users"

# Trying to use GET with admin routes results in 405 "Method Not Allowed"
get "http://127.0.0.1:$PORT/admin/remove/pizzalover"

delete "http://127.0.0.1:$PORT/admin/remove/pizzalover"
delete "http://127.0.0.1:$PORT/admin/remove/pizzalover9000"

get "http://127.0.0.1:$PORT/list_users"

delete "http://127.0.0.1:$PORT/admin/clear_users"

get "http://127.0.0.1:$PORT/list_users"
