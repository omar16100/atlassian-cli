#!/bin/bash
# Jira Bulk Transition Script
#
# Bulk transition issues matching a JQL query through workflow states.
# Supports dry-run mode and progress tracking.
#
# Usage:
#   # Dry run (preview affected issues)
#   ./bulk-transition.sh --jql "project = PROJ AND status = 'In Progress'" \
#                        --transition "Done" --dry-run
#
#   # Execute transition
#   ./bulk-transition.sh --jql "project = PROJ AND status = 'In Progress'" \
#                        --transition "Done" --profile prod
#
# Requirements:
#   - atlassian-cli installed and configured

set -euo pipefail

# Configuration
JQL=""
TRANSITION=""
PROFILE="default"
DRY_RUN=true
CONCURRENCY=4
COMMENT=""

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
            --jql)
                JQL="$2"
                shift 2
                ;;
            --transition)
                TRANSITION="$2"
                shift 2
                ;;
            --profile)
                PROFILE="$2"
                shift 2
                ;;
            --comment)
                COMMENT="$2"
                shift 2
                ;;
            --concurrency)
                CONCURRENCY="$2"
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

    if [ -z "$JQL" ] || [ -z "$TRANSITION" ]; then
        error "Both --jql and --transition are required"
        exit 1
    fi
}

# Preview affected issues
preview_issues() {
    log "Finding issues matching JQL..."
    log "Query: $JQL"

    local issues
    issues=$(atlassian-cli jira search \
        --profile "$PROFILE" \
        --jql "$JQL" \
        --output json 2>/dev/null || echo "[]")

    local count
    count=$(echo "$issues" | jq '. | length')

    if [ "$count" -eq 0 ]; then
        warn "No issues found matching criteria"
        return 1
    fi

    log "Found $count issues to transition"

    # Show sample issues
    echo ""
    echo "Sample issues (first 10):"
    echo "$issues" | jq -r '.[:10][] | "  - \(.key): \(.fields.summary) (\(.fields.status.name))"'
    echo ""

    return 0
}

# Execute bulk transition
execute_transition() {
    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would transition issues to: $TRANSITION"
        return
    fi

    log "Executing bulk transition to: $TRANSITION"

    local args=(
        "jira" "bulk" "transition"
        "--profile" "$PROFILE"
        "--jql" "$JQL"
        "--transition" "$TRANSITION"
        "--concurrency" "$CONCURRENCY"
    )

    if [ -n "$COMMENT" ]; then
        args+=("--comment" "$COMMENT")
    fi

    atlassian-cli "${args[@]}"

    log "Transition complete"
}

# Main execution
main() {
    parse_args "$@"

    log "Jira Bulk Transition"
    log "Transition: $TRANSITION | Profile: $PROFILE"

    if [ "$DRY_RUN" = "true" ]; then
        warn "DRY-RUN MODE: No changes will be made"
    fi

    if ! preview_issues; then
        exit 0
    fi

    if [ "$DRY_RUN" = "false" ]; then
        warn "This will transition matching issues to: $TRANSITION"
        read -rp "Type 'YES' to confirm: " confirm

        if [ "$confirm" != "YES" ]; then
            log "Transition cancelled"
            exit 0
        fi
    fi

    execute_transition

    log "Bulk transition complete"
}

main "$@"
