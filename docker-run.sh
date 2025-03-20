#!/usr/bin/env bash
set -eo pipefail

# Run API locally with Docker

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=./common.sh
source "$DIR/common.sh"

USAGE="Usage: $0 [OPTIONS]

Run API in Docker.

OPTIONS: All options are optional
    -h | --help
        Display these instructions.

    -f | --force
        Delete an existing container.

    -v | --verbose
        Display commands being executed.
"

FORCE=false
while [ $# -gt 0 ]; do
    case "$1" in
        -h | --help)
            print_usage_and_exit
            ;;
        -f | --force)
            FORCE=true
            ;;
        -v | --verbose)
            set -x
            ;;
    esac
    shift
done

cd "$REPO_ROOT"

DOCKER_IMAGE="axum-runtime"
DOCKER_CONTAINER="axum-example"

start_container() {
    print_magenta "Running API..."
    docker run --detach --name "$DOCKER_CONTAINER" --publish 80:8080 "$DOCKER_IMAGE"

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
}

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

if [ -z "$(docker ps --quiet --filter "name=$DOCKER_CONTAINER")" ]; then
    ./docker-build.sh
    # There is an existing container but it is not running
    if docker ps --all | grep -q "$DOCKER_CONTAINER"; then
        print_yellow "Deleting existing container"
        docker rm -f "$DOCKER_CONTAINER"
    fi
    start_container
elif [ "$FORCE" = true ]; then
    print_yellow "Deleting running container"
    docker rm -f "$DOCKER_CONTAINER"
    ./docker-build.sh
    start_container
else
    echo "Docker image is running, skipping build..."
fi

"$DIR/test-routes.sh" --local

if [ "$BASH_PLATFORM" = mac ]; then
    print_magenta "Opening API docs..."
    open http://127.0.0.1/doc
    open http://127.0.0.1/redoc
    open http://127.0.0.1/rapidoc
    open http://127.0.0.1/scalar
fi
