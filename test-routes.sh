#!/bin/bash
set -eo pipefail

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=./common.sh
source "$DIR/common.sh"

USAGE="Usage: $0 [OPTIONS]

Test API routes.

OPTIONS: All options are optional
    -h | --help             Display these instructions.
    -k | --key [KEY]        API key to use.
    -p | --port [NUMBER]    Specify port number to use. Default is 3000.
    -l | --local            Use config for running locally with Docker.
    -u | --url [URL]        Specify URL to use. Default is http://127.0.0.1
    -t | --timing           Show timing information.
    -s | --silent           Don't print responses.
    -v | --verbose          Display commands being executed.
"
export USAGE

TIMING=false
SILENT=false
while [ $# -gt 0 ]; do
    case "$1" in
        -h | --help)
            print_usage_and_exit
            ;;
        -k | --key)
            API_KEY=$2
            shift
            ;;
        -p | --port)
            PORT=$2
            shift
            ;;
        -l | --local)
            PORT=80
            URL="http://127.0.0.1"
            API_KEY="axum-api-key"
            ;;
        -u | --url)
            URL=$2
            shift
            ;;
        -t | --timing)
            TIMING=true
            ;;
        -s | --silent)
            SILENT=true
            ;;
        -v | --verbose)
            set -x
            ;;
    esac
    shift
done

API_KEY=${API_KEY:-"axum-api-key"}
PORT=${PORT:-3000}
URL=${URL:-"http://127.0.0.1"}

# Initialize counters for total and server processing times
total_time_sum=0
server_processing_time_sum=0
request_count=0

get() {
    local url="$1"
    local api_key="$2"
    if [ "$SILENT" = false ]; then
        print_cyan "GET: $1"
    fi
    response=$(curl -s -X GET --http2-prior-knowledge \
        -H "api-key: $api_key" \
        -w '{"http_code":%{http_code},"time_namelookup":%{time_namelookup},"time_connect":%{time_connect},"time_pretransfer":%{time_pretransfer},"time_redirect":%{time_redirect},"time_starttransfer":%{time_starttransfer},"time_total":%{time_total}}' \
        -o response.json "$url")
    print_response "$response"
}

delete() {
    local url="$1"
    local api_key="$2"
    if [ "$SILENT" = false ]; then
        print_cyan "DELETE: $1"
    fi
    response=$(curl -s -X DELETE --http2-prior-knowledge \
        -H "Content-Type: application/json" \
        -H "api-key: $api_key" \
        -w '{"http_code":%{http_code},"time_namelookup":%{time_namelookup},"time_connect":%{time_connect},"time_pretransfer":%{time_pretransfer},"time_redirect":%{time_redirect},"time_starttransfer":%{time_starttransfer},"time_total":%{time_total}}' \
        -o response.json \
        "$url")
    print_response "$response"
}

post() {
    local url="$1"
    local data="$2"
    local api_key="$3"
    if [ "$SILENT" = false ]; then
        print_cyan "POST: $1 $2"
    fi
    response=$(curl -s -X POST --http2-prior-knowledge \
        -H "Content-Type: application/json" \
        -H "api-key: $api_key" \
        -d "$data" \
        -w '{"http_code":%{http_code},"time_namelookup":%{time_namelookup},"time_connect":%{time_connect},"time_pretransfer":%{time_pretransfer},"time_redirect":%{time_redirect},"time_starttransfer":%{time_starttransfer},"time_total":%{time_total}}' \
        -o response.json \
        "$url")
    print_response "$response"
}

# Print the rate limit headers for a GET request.
rate_limit_headers() {
    local url="$1"
    if [ "$SILENT" = false ]; then
        print_cyan "HEADERS: $url"
        curl -s -o /dev/null -D - --http2-prior-knowledge "$url" | grep -i -E '^(ratelimit-|retry-after|x-request-id)' || true
    fi
}

# Send one JSON-RPC request to the MCP endpoint.
mcp() {
    local data="$1"
    if [ "$SILENT" = false ]; then
        print_cyan "MCP: $data"
    fi
    response=$(curl -s -X POST --http2-prior-knowledge \
        -H "Content-Type: application/json" \
        -H "Accept: application/json, text/event-stream" \
        -d "$data" \
        -w '{"http_code":%{http_code},"time_namelookup":%{time_namelookup},"time_connect":%{time_connect},"time_pretransfer":%{time_pretransfer},"time_redirect":%{time_redirect},"time_starttransfer":%{time_starttransfer},"time_total":%{time_total}}' \
        -o response.json \
        "$URL:$PORT/mcp")
    print_response "$response"
}

print_response() {
    local response="$1"
    local http_code time_namelookup time_connect time_pretransfer time_redirect time_starttransfer time_total
    http_code=$(echo "$response" | jq -r '.http_code')
    time_namelookup=$(echo "$response" | jq -r '.time_namelookup')
    time_connect=$(echo "$response" | jq -r '.time_connect')
    time_pretransfer=$(echo "$response" | jq -r '.time_pretransfer')
    time_redirect=$(echo "$response" | jq -r '.time_redirect')
    time_starttransfer=$(echo "$response" | jq -r '.time_starttransfer')
    time_total=$(echo "$response" | jq -r '.time_total')

    local server_processing_time
    server_processing_time=$(echo "$time_starttransfer - $time_pretransfer" | bc)
    server_processing_time=$(printf "%.6f" "$server_processing_time")

    if [ "$SILENT" = false ]; then
        if [[ "$http_code" =~ ^2 ]]; then
            echo "Status code: $(green "$http_code")"
        elif [[ "$http_code" =~ ^[45] ]]; then
            echo "Status code: $(red "$http_code")"
        else
            echo "Status code: $http_code"
        fi
    fi

    if [ "$TIMING" = true ] && [ "$SILENT" = false ]; then
        echo "time_namelookup:    ${time_namelookup}s"
        echo "time_connect:       ${time_connect}s"
        echo "time_pretransfer:   ${time_pretransfer}s"
        echo "time_redirect:      ${time_redirect}s"
        echo "time_starttransfer: ${time_starttransfer}s"
        echo "time_total:         ${time_total}s"
        echo "time_server:        ${server_processing_time}s"
    fi

    if [ "$SILENT" = false ]; then
        output=$(jq --color-output < response.json || cat response.json)
        if [ "$(echo "$output" | wc -l)" -gt 1 ]; then
            echo "Response:"
            echo "$output"
        else
            echo "Response: $output"
        fi
    fi
    rm response.json

    total_time_sum=$(echo "$total_time_sum + $time_total" | bc)
    server_processing_time_sum=$(echo "$server_processing_time_sum + $server_processing_time" | bc)
    request_count=$((request_count + 1))
}

calculate_averages() {
    average_total_time=$(echo "scale=6; $total_time_sum / $request_count" | bc)
    average_server_time=$(echo "scale=6; $server_processing_time_sum / $request_count" | bc)
    printf "Average total time:             %.3fs\n" "$average_total_time"
    printf "Average server processing time: %.3fs\n" "$average_server_time"
}

if ! curl -s -o /dev/null -w "%{http_code}" "$URL:$PORT" | grep -q '^2'; then
    print_error_and_exit "Failed to call API, is it running?"
fi

print_magenta "Testing routes..."

get "$URL:$PORT"
get "$URL:$PORT/health"
get "$URL:$PORT/status"
get "$URL:$PORT/version"
get "$URL:$PORT/metrics"
get "$URL:$PORT/items"
post "$URL:$PORT/items" '{"name":"esgrove"}'
get "$URL:$PORT/item?name=esgrove"
post "$URL:$PORT/items" '{"name":"esgrove"}'
post "$URL:$PORT/items" '{"name":"five","id":5555}'
post "$URL:$PORT/items" '{"name":"error","id":1}'
get "$URL:$PORT/item?name=pizzalover9000"

for name in pizzalover9000 akseli swanson; do
    post "$URL:$PORT/items" "{\"name\":\"$name\"}"
done

get "$URL:$PORT/items"
get "$URL:$PORT/items?skip=1&limit=2"

print_yellow 'Using GET with admin routes results in 405 "Method Not Allowed":'
get "$URL:$PORT/admin/remove/pizzalover"

print_yellow "Admin routes require api key:"
delete "$URL:$PORT/admin/remove/pizzalover"

delete "$URL:$PORT/admin/remove/pizzalover" "$API_KEY"
delete "$URL:$PORT/admin/remove/pizzalover9000" "$API_KEY"

get "$URL:$PORT/items"

delete "$URL:$PORT/admin/clear_items" "$API_KEY"

get "$URL:$PORT/items"

print_yellow "Rate limited routes return RateLimit headers:"
rate_limit_headers "$URL:$PORT/items"

print_magenta "Testing MCP endpoint..."
mcp '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test-routes","version":"1.0.0"}}}'
mcp '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
mcp '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"health","arguments":{}}}'
mcp '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"create_item","arguments":{"name":"mcp-item"}}}'
mcp '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_items","arguments":{}}}'
mcp '{"jsonrpc":"2.0","id":6,"method":"resources/list","params":{}}'

if [ "$TIMING" = true ]; then
    calculate_averages
fi
