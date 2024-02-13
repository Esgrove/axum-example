#!/bin/bash
set -eo pipefail

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=../common.sh
source "$DIR/common.sh"

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
    response=$(curl -s -H "Content-Type: application/json" -d "$data" -w "%{http_code}" -o response.json "$url")
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
    jq < response.json
    rm response.json
}

if ! curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:3000" | grep -q '^2'; then
    print_error_and_exit "Failed to call API, is it running?"
fi

get http://127.0.0.1:3000
get http://127.0.0.1:3000/version
get http://127.0.0.1:3000/user?username=pizzalover9000
post http://127.0.0.1:3000/users '{"username":"esgrove"}'
get http://127.0.0.1:3000/user?username=esgrove
post http://127.0.0.1:3000/users '{"username":"esgrove"}'
