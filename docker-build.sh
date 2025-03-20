#!/usr/bin/env bash
set -eo pipefail

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=./common.sh
source "$DIR/common.sh"

USAGE="Usage: $0 [OPTIONS]

Build Docker image.

OPTIONS: All options are optional
    -h | --help
        Display these instructions.

    -v | --verbose
        Display commands being executed.
"

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

cd "$REPO_ROOT"

DOCKER_IMAGE="axum-runtime"
GIT_HASH=$(git rev-parse --short HEAD)
TIMESTAMP=$(date "+%Y-%m-%d")
TAG="${TIMESTAMP}-${GIT_HASH}"

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

print_magenta "Building Docker image..."
docker build \
    --pull \
    --build-arg DEPLOYMENT_TAG="$TAG" \
    --tag "$DOCKER_IMAGE":latest \
    --target "$DOCKER_IMAGE" \
    --file Dockerfile .
