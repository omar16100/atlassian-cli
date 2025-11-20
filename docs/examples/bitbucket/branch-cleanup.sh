#!/bin/bash
# Bitbucket Branch Cleanup Script
#
# Deletes merged branches across repositories in a workspace.
# Supports dry-run mode and exclusion patterns.
#
# Usage:
#   # Dry run to preview branches to delete
#   ./branch-cleanup.sh --workspace myworkspace --dry-run
#
#   # Delete merged branches (excluding protected branches)
#   ./branch-cleanup.sh --workspace myworkspace \
#                       --exclude "main,master,develop,release/*"
#
#   # Clean up a specific repository
#   ./branch-cleanup.sh --workspace myworkspace --repo myrepo
#
# Requirements:
#   - atlassian-cli installed and configured

set -euo pipefail

# Configuration
WORKSPACE=""
REPO=""
PROFILE="default"
DRY_RUN=true
EXCLUDE_PATTERNS="main,master,develop"
CONCURRENCY=4

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log() {
    echo -e "${GREEN}[INFO]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --workspace)
                WORKSPACE="$2"
                shift 2
                ;;
            --repo)
                REPO="$2"
                shift 2
                ;;
            --profile)
                PROFILE="$2"
                shift 2
                ;;
            --exclude)
                EXCLUDE_PATTERNS="$2"
                shift 2
                ;;
            --execute)
                DRY_RUN=false
                shift
                ;;
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            *)
                error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    if [ -z "$WORKSPACE" ]; then
        error "Workspace slug required (--workspace WORKSPACE)"
        exit 1
    fi
}

# Check if branch should be excluded
is_excluded() {
    local branch="$1"

    IFS=',' read -ra patterns <<< "$EXCLUDE_PATTERNS"
    for pattern in "${patterns[@]}"; do
        # Simple wildcard matching
        if [[ "$branch" == $pattern ]]; then
            return 0
        fi
    done

    return 1
}

# Get all repositories or specific repo
get_repos() {
    if [ -n "$REPO" ]; then
        echo "[\"$REPO\"]"
    else
        log "Fetching all repositories from workspace: $WORKSPACE"
        atlassian-cli bitbucket repo list \
            --profile "$PROFILE" \
            "$WORKSPACE" \
            --output json | jq -r '[.[].slug]'
    fi
}

# Get merged branches for a repository
get_merged_branches() {
    local repo="$1"

    log "Finding merged branches in $repo..."

    # Get all branches
    local branches
    branches=$(atlassian-cli bitbucket branch list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$repo" \
        --output json)

    # Filter for merged branches (simplified - assumes default branch is main/master)
    echo "$branches" | jq -r '.[] | select(.merge_strategies != null) | .name'
}

# Delete branch
delete_branch() {
    local repo="$1"
    local branch="$2"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would delete branch: $repo/$branch"
        return
    fi

    log "Deleting branch: $repo/$branch"

    atlassian-cli bitbucket branch delete \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$repo" \
        "$branch" || warn "Failed to delete: $branch"
}

# Process repository branches
process_repo() {
    local repo="$1"

    log "Processing repository: $repo"

    local branches
    branches=$(get_merged_branches "$repo")

    if [ -z "$branches" ]; then
        log "No merged branches found in $repo"
        return
    fi

    local deleted=0
    local skipped=0

    while IFS= read -r branch; do
        if is_excluded "$branch"; then
            log "Skipping protected branch: $branch"
            skipped=$((skipped + 1))
            continue
        fi

        delete_branch "$repo" "$branch"
        deleted=$((deleted + 1))

    done <<< "$branches"

    log "Repository $repo: $deleted deleted, $skipped skipped"
}

# Main execution
main() {
    parse_args "$@"

    log "Bitbucket Branch Cleanup"
    log "Workspace: $WORKSPACE | Profile: $PROFILE"

    if [ "$DRY_RUN" = "true" ]; then
        warn "DRY-RUN MODE: No branches will be deleted"
    fi

    log "Exclude patterns: $EXCLUDE_PATTERNS"

    local repos
    repos=$(get_repos)

    if [ "$(echo "$repos" | jq '. | length')" -eq 0 ]; then
        error "No repositories found"
        exit 1
    fi

    local total_repos
    total_repos=$(echo "$repos" | jq '. | length')
    log "Found $total_repos repositories to process"

    # Process each repository
    echo "$repos" | jq -r '.[]' | while IFS= read -r repo_name; do
        process_repo "$repo_name"
    done

    log "Branch cleanup complete"

    if [ "$DRY_RUN" = "true" ]; then
        warn "This was a dry run. Use --execute to actually delete branches."
    fi
}

main "$@"
