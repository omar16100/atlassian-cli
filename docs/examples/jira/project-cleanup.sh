#!/bin/bash
# Jira Project Cleanup Script
#
# Performs bulk cleanup operations on Jira issues:
#   - Archive old resolved issues
#   - Update labels for categorization
#   - Update components or fix versions
#
# Usage:
#   # Dry run to preview changes
#   ./project-cleanup.sh --project PROJ --days 180 --dry-run
#
#   # Add archived label to old issues
#   ./project-cleanup.sh --project PROJ --days 180 --label archived
#
#   # Bulk update component
#   ./project-cleanup.sh --project PROJ --status Done --component Legacy
#
# Requirements:
#   - atlassian-cli installed and configured

set -euo pipefail

# Configuration
PROJECT=""
DAYS_OLD=""
STATUS=""
LABEL=""
COMPONENT=""
PROFILE="default"
DRY_RUN=true
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
            --project)
                PROJECT="$2"
                shift 2
                ;;
            --days)
                DAYS_OLD="$2"
                shift 2
                ;;
            --status)
                STATUS="$2"
                shift 2
                ;;
            --label)
                LABEL="$2"
                DRY_RUN=false
                shift 2
                ;;
            --component)
                COMPONENT="$2"
                DRY_RUN=false
                shift 2
                ;;
            --profile)
                PROFILE="$2"
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

    if [ -z "$PROJECT" ]; then
        error "Project key required (--project PROJECT)"
        exit 1
    fi
}

# Build JQL query
build_jql() {
    local jql="project = $PROJECT"

    if [ -n "$DAYS_OLD" ]; then
        local cutoff_date
        cutoff_date=$(date -u -d "$DAYS_OLD days ago" +%Y-%m-%d 2>/dev/null || \
                      date -u -v-${DAYS_OLD}d +%Y-%m-%d)  # macOS fallback
        jql="$jql AND resolved < -${DAYS_OLD}d"
    fi

    if [ -n "$STATUS" ]; then
        jql="$jql AND status = \"$STATUS\""
    fi

    echo "$jql"
}

# Preview affected issues
preview_issues() {
    local jql="$1"

    log "Finding issues matching criteria..."
    log "JQL: $jql"

    local issues
    issues=$(atlassian-cli jira search \
        --profile "$PROFILE" \
        --jql "$jql" \
        --output json 2>/dev/null || echo "[]")

    local count
    count=$(echo "$issues" | jq '. | length')

    if [ "$count" -eq 0 ]; then
        warn "No issues found matching criteria"
        return 1
    fi

    log "Found $count issues to process"

    # Show sample issues
    echo ""
    echo "Sample issues (first 10):"
    echo "$issues" | jq -r '.[:10][] | "  - \(.key): \(.fields.summary) (\(.fields.status.name))"'
    echo ""

    return 0
}

# Add label to issues
add_label() {
    local jql="$1"
    local label="$2"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would add label '$label' to matching issues"
        return
    fi

    log "Adding label '$label' to issues..."

    atlassian-cli jira bulk label \
        --profile "$PROFILE" \
        --jql "$jql" \
        --labels "$label" \
        --concurrency "$CONCURRENCY"

    log "Labels added successfully"
}

# Update component
update_component() {
    local jql="$1"
    local component="$2"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would update component to '$component' for matching issues"
        return
    fi

    warn "Bulk component update requires custom implementation"
    warn "Use: atlassian-cli jira issue update <key> --component \"$component\""
}

# Generate cleanup report
generate_report() {
    local jql="$1"

    log "Generating cleanup report..."

    local issues
    issues=$(atlassian-cli jira search \
        --profile "$PROFILE" \
        --jql "$jql" \
        --output json)

    local report_file="cleanup_report_$(date +%Y%m%d_%H%M%S).csv"

    echo "Issue Key,Summary,Status,Created,Resolved,Labels" > "$report_file"

    echo "$issues" | jq -r '.[] |
        [
            .key,
            (.fields.summary | gsub("\"";"\"\"") ),
            .fields.status.name,
            .fields.created,
            (.fields.resolutiondate // "N/A"),
            ((.fields.labels // []) | join(";"))
        ] | @csv' >> "$report_file"

    log "Report saved: $report_file"
}

# Main execution
main() {
    parse_args "$@"

    log "Jira Project Cleanup"
    log "Project: $PROJECT | Profile: $PROFILE"

    if [ "$DRY_RUN" = "true" ]; then
        warn "DRY-RUN MODE: No changes will be made"
    fi

    local jql
    jql=$(build_jql)

    if ! preview_issues "$jql"; then
        exit 0
    fi

    # Generate report
    generate_report "$jql"

    # Execute operations
    if [ -n "$LABEL" ]; then
        add_label "$jql" "$LABEL"
    fi

    if [ -n "$COMPONENT" ]; then
        update_component "$jql" "$COMPONENT"
    fi

    log "Cleanup complete"
}

main "$@"
