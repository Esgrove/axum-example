#!/usr/bin/env bash
set -euo pipefail

# Import common functions
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./common.sh
source "$DIR/common.sh"

USAGE="Usage: $0 [OPTIONS]

Update the Rust dependencies, validate them, and open a GitHub pull request.

OPTIONS: All options are optional
    -h | --help       Display these instructions.
    -d | --dryrun     Run updates and checks without committing.
    -v | --verbose    Display commands being executed.
"
export USAGE

DRYRUN=false
while [ $# -gt 0 ]; do
    case "$1" in
        -h | --help)
            echo "$USAGE"
            exit 0
            ;;
        -d | --dryrun)
            DRYRUN=true
            ;;
        -v | --verbose)
            set -x
            ;;
        *)
            print_usage_and_exit "Unknown argument '$1'"
            ;;
    esac
    shift
done

BRANCH="cargo-update-$(date -u +%Y%m%d-%H%M%S)"
START_BRANCH=$(git -C "$REPO_ROOT" branch --show-current)
if [ -z "$START_BRANCH" ]; then
    print_error_and_exit "Check out a branch before updating dependencies."
fi

OUTPUT_FILE=$(mktemp "${TMPDIR:-/tmp}/axum-example-crate-update.XXXXXX")
trap 'rm -f "$OUTPUT_FILE"' EXIT

cd "$REPO_ROOT"

if [ -n "$(git status --porcelain)" ]; then
    print_error_and_exit "The working tree is not clean. Commit or stash local changes before updating dependencies."
fi

print_magenta "Checking out main branch..."
git switch main
git pull --ff-only origin main

print_magenta "Creating branch $BRANCH..."
git switch -c "$BRANCH"

if ! cargo upgrade --version > /dev/null 2>&1; then
    print_yellow "cargo-upgrade is not installed, installing cargo-edit..."
    cargo install --locked cargo-edit
fi

print_magenta "Updating crates..."
printf '# Rust dependency update\n\n' > "$OUTPUT_FILE"
printf 'Updated dependencies reported by Cargo:\n\n```text\n' >> "$OUTPUT_FILE"
cargo upgrade -i 2>&1 | tee -a "$OUTPUT_FILE"
cargo update --color never 2>&1 |
    sed -e '1{/Updating crates.io index/d;}' -e 's/^[[:space:]]*//' -e '/^$/d' |
    tee -a "$OUTPUT_FILE"
printf '```\n' >> "$OUTPUT_FILE"

if git diff --quiet -- Cargo.toml Cargo.lock; then
    print_green "Rust dependencies are already up to date."
    git switch "$START_BRANCH"
    git branch -D "$BRANCH"
    exit 0
fi

print_magenta "Validating dependency updates..."
cargo fmt
cargo clippy --all-targets --fix --allow-dirty -- -Dwarnings
cargo fmt
cargo test

if [ "$DRYRUN" = true ]; then
    print_green "Dry run complete. Changes are available on branch $BRANCH."
    exit 0
fi

print_magenta "Committing changes..."
git add --all
git commit -m "Update Rust dependencies"

print_magenta "Pushing branch to origin..."
git push --set-upstream origin "$BRANCH"

if command -v gh > /dev/null 2>&1; then
    print_magenta "Creating pull request on GitHub..."
    gh pr create \
        --base main \
        --head "$BRANCH" \
        --title "Update Rust dependencies" \
        --body-file "$OUTPUT_FILE" \
        --label dependencies
    gh pr view --web
else
    print_yellow "GitHub CLI is not installed. The branch was pushed, but no pull request was created."
fi

print_green "Done!"
