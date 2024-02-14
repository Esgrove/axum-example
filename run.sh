#!/usr/bin/env bash
set -eo pipefail

# Run API locally with Docker

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=./common.sh
source "$DIR/common.sh"

cd "$REPO_ROOT"

if ! docker info > /dev/null 2>&1; then
    if [ "$BASH_PLATFORM" = mac ]; then
        print_yellow "Docker does not seem to be running. Starting Docker..."
        open -a Docker.app
        sleep 10
        if ! docker info > /dev/null 2>&1; then
            print_error_and_exit "Timed out waiting for Docker to start..."
        fi
    else
        print_error_and_exit "Docker does not seem to be running. Start Docker first..."
    fi
fi

if [ -z "$(docker ps -q --filter "name=axum-example")" ]; then
    print_magenta "Building Docker image..."
    docker build -t axum-runtime .

    if docker ps -a | grep -q axum-example; then
        print_yellow "Deleting existing container"
        docker rm -f axum-example
    fi

    print_magenta "Running API..."
    docker run -d --name axum-example -p 80:80 axum-runtime

    echo "Waiting for container to start..."
    timeout=10
    while ! curl -fs http://127.0.0.1 > /dev/null; do
        sleep 1
        ((timeout--))
        if [ "$timeout" -le 0 ]; then
            print_error_and_exit "Container failed to start"
        fi
    done
    print_green "API is running"
else
    echo "Docker image is running, skipping build..."
fi

PORT=80 ./test-routes.sh
