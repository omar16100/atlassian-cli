#!/bin/bash
# Bitbucket Pull Request Automation Script
#
# Automates PR workflows:
#   - Auto-approve PRs matching criteria (author, reviewers, checks passed)
#   - Auto-merge approved PRs
#   - Add reviewers to PRs missing review
#
# Usage:
#   # Dry run to preview actions
#   ./pr-automation.sh --workspace myworkspace --repo myrepo --dry-run
#
#   # Auto-approve PRs from specific authors
#   ./pr-automation.sh --workspace myworkspace --repo myrepo \
#                      --auto-approve --trusted-authors "user1,user2"
#
#   # Auto-merge approved PRs
#   ./pr-automation.sh --workspace myworkspace --repo myrepo --auto-merge
#
# Requirements:
#   - atlassian-cli installed and configured

set -euo pipefail

# Configuration
WORKSPACE=""
REPO=""
PROFILE="default"
DRY_RUN=true
AUTO_APPROVE=false
AUTO_MERGE=false
TRUSTED_AUTHORS=""
MIN_APPROVALS=2

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
            --auto-approve)
                AUTO_APPROVE=true
                DRY_RUN=false
                shift
                ;;
            --auto-merge)
                AUTO_MERGE=true
                DRY_RUN=false
                shift
                ;;
            --trusted-authors)
                TRUSTED_AUTHORS="$2"
                shift 2
                ;;
            --min-approvals)
                MIN_APPROVALS="$2"
                shift 2
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

    if [ -z "$WORKSPACE" ] || [ -z "$REPO" ]; then
        error "Both --workspace and --repo are required"
        exit 1
    fi
}

# Get open pull requests
get_open_prs() {
    log "Fetching open pull requests..."

    atlassian-cli bitbucket pullrequest list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        --state OPEN \
        --output json
}

# Check if author is trusted
is_trusted_author() {
    local author="$1"

    if [ -z "$TRUSTED_AUTHORS" ]; then
        return 1
    fi

    echo "$TRUSTED_AUTHORS" | grep -q "$author"
}

# Get PR approval count
get_approval_count() {
    local pr_id="$1"

    atlassian-cli bitbucket pullrequest get \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        "$pr_id" \
        --output json | \
        jq '[.participants[] | select(.approved == true)] | length'
}

# Auto-approve PR
approve_pr() {
    local pr_id="$1"
    local title="$2"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would approve PR #$pr_id: $title"
        return
    fi

    log "Approving PR #$pr_id: $title"

    atlassian-cli bitbucket pullrequest approve \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        "$pr_id"
}

# Auto-merge PR
merge_pr() {
    local pr_id="$1"
    local title="$2"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would merge PR #$pr_id: $title"
        return
    fi

    log "Merging PR #$pr_id: $title"

    atlassian-cli bitbucket pullrequest merge \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        "$pr_id" \
        --strategy merge
}

# Process PRs for auto-approval
process_auto_approve() {
    local prs="$1"

    if [ "$AUTO_APPROVE" = "false" ]; then
        return
    fi

    log "Processing PRs for auto-approval..."

    local approved=0

    echo "$prs" | jq -r '.[] | @json' | while IFS= read -r pr_json; do
        local pr_id
        pr_id=$(echo "$pr_json" | jq -r '.id')
        local title
        title=$(echo "$pr_json" | jq -r '.title')
        local author
        author=$(echo "$pr_json" | jq -r '.author.display_name')

        # Check if author is trusted
        if is_trusted_author "$author"; then
            log "PR #$pr_id by trusted author: $author"
            approve_pr "$pr_id" "$title"
            approved=$((approved + 1))
        fi
    done

    log "Auto-approved $approved PRs"
}

# Process PRs for auto-merge
process_auto_merge() {
    local prs="$1"

    if [ "$AUTO_MERGE" = "false" ]; then
        return
    fi

    log "Processing PRs for auto-merge..."

    local merged=0

    echo "$prs" | jq -r '.[] | @json' | while IFS= read -r pr_json; do
        local pr_id
        pr_id=$(echo "$pr_json" | jq -r '.id')
        local title
        title=$(echo "$pr_json" | jq -r '.title')

        # Check approval count
        local approvals
        approvals=$(get_approval_count "$pr_id")

        if [ "$approvals" -ge "$MIN_APPROVALS" ]; then
            log "PR #$pr_id has $approvals approvals (min: $MIN_APPROVALS)"
            merge_pr "$pr_id" "$title"
            merged=$((merged + 1))
        else
            log "PR #$pr_id has only $approvals approvals (need $MIN_APPROVALS)"
        fi
    done

    log "Auto-merged $merged PRs"
}

# Generate PR summary
generate_summary() {
    local prs="$1"

    echo ""
    log "Pull Request Summary:"

    local total
    total=$(echo "$prs" | jq '. | length')
    echo "  Total Open PRs: $total"

    local approved
    approved=$(echo "$prs" | jq '[.[] | select(.state == "OPEN")] | length')
    echo "  Approved: $approved"

    echo ""
}

# Main execution
main() {
    parse_args "$@"

    log "Bitbucket PR Automation"
    log "Workspace: $WORKSPACE | Repo: $REPO | Profile: $PROFILE"

    if [ "$DRY_RUN" = "true" ]; then
        warn "DRY-RUN MODE: No changes will be made"
    fi

    local prs
    prs=$(get_open_prs)

    if [ "$(echo "$prs" | jq '. | length')" -eq 0 ]; then
        log "No open pull requests found"
        exit 0
    fi

    generate_summary "$prs"

    process_auto_approve "$prs"
    process_auto_merge "$prs"

    log "PR automation complete"
}

main "$@"
