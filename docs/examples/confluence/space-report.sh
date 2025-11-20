#!/bin/bash
# Confluence Space Analytics Report
#
# Generates a CSV report with space statistics and page view data.
# Useful for understanding content usage and identifying popular/stale pages.
#
# Usage:
#   ./space-report.sh SPACE_KEY [PROFILE] [OUTPUT_FILE]
#   ./space-report.sh DOCS prod report.csv
#
# Output CSV columns:
#   - Space Key, Page ID, Title, Views, Last Modified, Labels
#
# Requirements:
#   - atlassian-cli installed and configured
#   - jq for JSON processing

set -euo pipefail

# Configuration
SPACE_KEY="${1:?Space key required}"
PROFILE="${2:-default}"
OUTPUT_FILE="${3:-space_report_$(date +%Y%m%d).csv}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() {
    echo -e "${GREEN}[$(date +'%H:%M:%S')]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

# Get space statistics
get_space_stats() {
    log "Fetching space statistics for $SPACE_KEY"

    local stats
    stats=$(atlassian-cli confluence analytics space-stats \
        --profile "$PROFILE" \
        "$SPACE_KEY" \
        --output json)

    echo "$stats" | jq -r '
        "Space: \(.space_key)",
        "Total Pages: \(.total_pages)",
        "Total Blogs: \(.total_blogs)",
        ""
    '
}

# Get all pages in space
get_all_pages() {
    log "Fetching all pages from $SPACE_KEY"

    local pages
    pages=$(atlassian-cli confluence page list \
        --profile "$PROFILE" \
        --space "$SPACE_KEY" \
        --limit 1000 \
        --output json)

    echo "$pages"
}

# Get page views for a specific page
get_page_views() {
    local page_id="$1"

    # Try to get page views, return 0 if not available
    atlassian-cli confluence analytics page-views \
        --profile "$PROFILE" \
        "$page_id" \
        --output json 2>/dev/null | \
        jq -r '.view_count // 0' || echo "0"
}

# Get page details including labels
get_page_details() {
    local page_id="$1"

    atlassian-cli confluence page get \
        --profile "$PROFILE" \
        "$page_id" \
        --output json
}

# Generate CSV report
generate_report() {
    local pages="$1"

    log "Generating report: $OUTPUT_FILE"

    # Write CSV header
    echo "Space,Page ID,Title,Status,Views,Created,Last Modified,Labels" > "$OUTPUT_FILE"

    local total
    total=$(echo "$pages" | jq '. | length')
    local processed=0

    # Process each page
    echo "$pages" | jq -r '.[] | @json' | while IFS= read -r page_json; do
        local page_id
        page_id=$(echo "$page_json" | jq -r '.id')
        local title
        title=$(echo "$page_json" | jq -r '.title')
        local status
        status=$(echo "$page_json" | jq -r '.status // "unknown"')

        processed=$((processed + 1))
        log "Processing [$processed/$total]: $title"

        # Get page views
        local views
        views=$(get_page_views "$page_id")

        # Get detailed page info
        local details
        details=$(get_page_details "$page_id")

        # Extract metadata
        local created
        created=$(echo "$details" | jq -r '.createdAt // "N/A"')
        local modified
        modified=$(echo "$details" | jq -r '.version.createdAt // "N/A"')

        # Extract labels (simplified - would need label API for full info)
        local labels="N/A"

        # Escape title for CSV
        local csv_title
        csv_title=$(echo "$title" | sed 's/"/""/g')

        # Write CSV row
        echo "$SPACE_KEY,\"$page_id\",\"$csv_title\",$status,$views,$created,$modified,\"$labels\"" >> "$OUTPUT_FILE"

    done
}

# Generate summary statistics
generate_summary() {
    log "Generating summary statistics"

    local total_pages
    total_pages=$(tail -n +2 "$OUTPUT_FILE" | wc -l | tr -d ' ')

    local total_views
    total_views=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f5 | \
        awk '{sum+=$1} END {print sum}')

    local avg_views
    if [ "$total_pages" -gt 0 ]; then
        avg_views=$((total_views / total_pages))
    else
        avg_views=0
    fi

    echo ""
    log "Report Summary:"
    echo "  Total Pages: $total_pages"
    echo "  Total Views: $total_views"
    echo "  Average Views: $avg_views"
    echo ""

    # Top 5 most viewed pages
    log "Top 5 Most Viewed Pages:"
    tail -n +2 "$OUTPUT_FILE" | \
        sort -t',' -k5 -nr | \
        head -5 | \
        awk -F',' '{printf "  %5d views - %s\n", $5, $3}' | \
        sed 's/"//g'

    echo ""
}

# Main execution
main() {
    log "Starting space analytics report"
    log "Space: $SPACE_KEY | Profile: $PROFILE"

    # Get space overview
    get_space_stats

    # Get all pages
    local pages
    pages=$(get_all_pages)

    if [ "$(echo "$pages" | jq '. | length')" -eq 0 ]; then
        warn "No pages found in space $SPACE_KEY"
        exit 0
    fi

    # Generate CSV report
    generate_report "$pages"

    # Show summary
    generate_summary

    log "Report saved to: $OUTPUT_FILE"
}

main
