#!/bin/bash
# Confluence Bulk Cleanup Script
#
# Performs bulk operations on Confluence pages:
#   - Find old/archived pages using CQL
#   - Add "archived" labels for tracking
#   - Optionally bulk delete pages
#
# Usage:
#   # Dry run (preview what would be affected)
#   ./bulk-cleanup.sh --space DOCS --days 365 --dry-run
#
#   # Add archived labels
#   ./bulk-cleanup.sh --space DOCS --days 365 --label
#
#   # Delete old pages (requires confirmation)
#   ./bulk-cleanup.sh --space DOCS --days 365 --delete
#
# Requirements:
#   - atlassian-cli installed and configured

set -euo pipefail

# Default configuration
SPACE_KEY=""
DAYS_OLD=365
PROFILE="default"
DRY_RUN=true
ADD_LABELS=false
DELETE_PAGES=false
LABEL_NAME="archived"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --space)
                SPACE_KEY="$2"
                shift 2
                ;;
            --days)
                DAYS_OLD="$2"
                shift 2
                ;;
            --profile)
                PROFILE="$2"
                shift 2
                ;;
            --label)
                ADD_LABELS=true
                DRY_RUN=false
                shift
                ;;
            --delete)
                DELETE_PAGES=true
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

    if [ -z "$SPACE_KEY" ]; then
        error "Space key is required (--space SPACE_KEY)"
        exit 1
    fi
}

# Build CQL query for old pages
build_cql() {
    local cutoff_date
    cutoff_date=$(date -u -d "$DAYS_OLD days ago" +%Y-%m-%d 2>/dev/null || \
                  date -u -v-${DAYS_OLD}d +%Y-%m-%d)  # macOS fallback

    local cql="space = $SPACE_KEY AND type = page AND lastModified < $cutoff_date"

    echo "$cql"
}

# Preview affected pages
preview_pages() {
    local cql="$1"

    log "Finding pages matching criteria..."
    log "CQL: $cql"

    local results
    results=$(atlassian-cli confluence search cql \
        --profile "$PROFILE" \
        --output json \
        "$cql" 2>/dev/null || echo "[]")

    local count
    count=$(echo "$results" | jq '. | length')

    if [ "$count" -eq 0 ]; then
        warn "No pages found matching criteria"
        return 1
    fi

    log "Found $count pages to process"

    # Show sample pages
    echo ""
    echo "Sample pages (first 10):"
    echo "$results" | jq -r '.[:10][] | "  - \(.content.title) (ID: \(.content.id))"'
    echo ""

    return 0
}

# Add labels to matching pages
add_labels_bulk() {
    local cql="$1"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would add '$LABEL_NAME' label to matching pages"
        return
    fi

    log "Adding '$LABEL_NAME' label to pages..."

    atlassian-cli confluence bulk add-labels \
        --profile "$PROFILE" \
        --cql "$cql" \
        --labels "$LABEL_NAME" \
        --concurrency 4

    log "Labels added successfully"
}

# Delete matching pages
delete_pages_bulk() {
    local cql="$1"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would delete matching pages"
        return
    fi

    # Safety confirmation
    local count
    count=$(atlassian-cli confluence search cql \
        --profile "$PROFILE" \
        --output json \
        "$cql" | jq '. | length')

    warn "This will permanently delete $count pages!"
    read -rp "Type 'DELETE' to confirm: " confirm

    if [ "$confirm" != "DELETE" ]; then
        log "Deletion cancelled"
        return
    fi

    log "Deleting pages..."

    atlassian-cli confluence bulk delete \
        --profile "$PROFILE" \
        --cql "$cql" \
        --concurrency 2

    log "Pages deleted"
}

# Main execution
main() {
    parse_args "$@"

    log "Confluence Bulk Cleanup"
    log "Space: $SPACE_KEY | Age: $DAYS_OLD days | Profile: $PROFILE"

    if [ "$DRY_RUN" = "true" ]; then
        warn "DRY-RUN MODE: No changes will be made"
    fi

    # Build and preview
    local cql
    cql=$(build_cql)

    if ! preview_pages "$cql"; then
        exit 0
    fi

    # Execute operations
    if [ "$ADD_LABELS" = "true" ]; then
        add_labels_bulk "$cql"
    fi

    if [ "$DELETE_PAGES" = "true" ]; then
        delete_pages_bulk "$cql"
    fi

    log "Cleanup complete"
}

main "$@"
