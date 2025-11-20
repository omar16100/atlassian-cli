#!/bin/bash
# Automated Documentation Pipeline: Markdown → Confluence
#
# This script syncs markdown documentation from a Git repository to Confluence.
# It converts markdown files to Confluence storage format and creates/updates pages.
#
# Usage:
#   ./doc-pipeline.sh --space DOCS --profile prod
#
# Requirements:
#   - atlassian-cli installed and configured
#   - pandoc (for markdown → HTML conversion)
#   - jq (for JSON processing)

set -euo pipefail

# Configuration
SPACE_KEY="${1:-DOCS}"
PROFILE="${2:-default}"
DOCS_DIR="./docs"
DRY_RUN="${DRY_RUN:-false}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

# Check dependencies
check_dependencies() {
    local missing=0

    for cmd in atlassian-cli pandoc jq; do
        if ! command -v "$cmd" &> /dev/null; then
            error "Required command not found: $cmd"
            missing=$((missing + 1))
        fi
    done

    if [ $missing -gt 0 ]; then
        error "$missing required dependencies missing. Please install them first."
        exit 1
    fi
}

# Convert markdown to Confluence storage format
md_to_confluence() {
    local md_file="$1"

    # Use pandoc to convert markdown to HTML
    pandoc -f markdown -t html "$md_file" | \
        # Basic cleanup for Confluence
        sed 's/<h1>/<h1 style="margin-top: 20px;">/g' | \
        sed 's/<code>/<code class="code-inline">/g'
}

# Get or create page by title
get_or_create_page() {
    local space_key="$1"
    local title="$2"
    local parent_id="${3:-}"

    # Search for existing page
    local page_id
    page_id=$(atlassian-cli confluence search cql \
        --output json \
        "space = $space_key AND title = \"$title\"" 2>/dev/null | \
        jq -r '.results[0].content.id // empty')

    if [ -n "$page_id" ]; then
        echo "$page_id"
        return
    fi

    # Create new page if not found
    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would create page: $title"
        echo "DRY_RUN_PAGE_ID"
        return
    fi

    log "Creating new page: $title"

    local create_args=(
        "confluence" "page" "create"
        "--profile" "$PROFILE"
        "--space" "$space_key"
        "--title" "$title"
    )

    if [ -n "$parent_id" ]; then
        create_args+=("--parent" "$parent_id")
    fi

    atlassian-cli "${create_args[@]}" --output json | jq -r '.id'
}

# Update page content
update_page() {
    local page_id="$1"
    local title="$2"
    local content="$3"

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would update page $page_id: $title"
        return
    fi

    # Save content to temp file
    local temp_file
    temp_file=$(mktemp)
    echo "$content" > "$temp_file"

    log "Updating page $page_id: $title"

    atlassian-cli confluence page update \
        --profile "$PROFILE" \
        "$page_id" \
        --title "$title" \
        --body "$temp_file"

    rm -f "$temp_file"
}

# Add labels to page
add_labels() {
    local page_id="$1"
    shift
    local labels=("$@")

    if [ "$DRY_RUN" = "true" ]; then
        warn "[DRY-RUN] Would add labels to $page_id: ${labels[*]}"
        return
    fi

    for label in "${labels[@]}"; do
        log "Adding label '$label' to page $page_id"
        atlassian-cli confluence page add-label \
            --profile "$PROFILE" \
            "$page_id" \
            "$label" || warn "Failed to add label: $label"
    done
}

# Main pipeline
main() {
    log "Starting documentation pipeline"
    log "Space: $SPACE_KEY | Profile: $PROFILE | Dry-run: $DRY_RUN"

    check_dependencies

    # Find all markdown files
    if [ ! -d "$DOCS_DIR" ]; then
        error "Documentation directory not found: $DOCS_DIR"
        exit 1
    fi

    local processed=0
    local failed=0

    # Process each markdown file
    while IFS= read -r md_file; do
        log "Processing: $md_file"

        # Extract title from first heading
        local title
        title=$(grep -m 1 '^# ' "$md_file" | sed 's/^# //' || echo "$(basename "$md_file" .md)")

        # Convert to Confluence format
        local content
        content=$(md_to_confluence "$md_file")

        # Get or create page
        local page_id
        page_id=$(get_or_create_page "$SPACE_KEY" "$title")

        if [ -z "$page_id" ]; then
            error "Failed to get/create page for: $title"
            failed=$((failed + 1))
            continue
        fi

        # Update page content
        update_page "$page_id" "$title" "$content"

        # Add auto-generated label
        add_labels "$page_id" "auto-generated" "documentation"

        processed=$((processed + 1))

    done < <(find "$DOCS_DIR" -name "*.md" -type f)

    log "Pipeline complete: $processed processed, $failed failed"

    if [ $failed -gt 0 ]; then
        exit 1
    fi
}

main "$@"
