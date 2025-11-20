#!/bin/bash
# Jira Sprint Report Generator
#
# Generates a CSV report for sprint issues with story points and cycle time metrics.
# Useful for retrospectives and velocity tracking.
#
# Usage:
#   ./sprint-report.sh --project PROJ --sprint 42 --output sprint-42.csv
#
# Output CSV columns:
#   - Issue Key, Summary, Type, Status, Story Points, Assignee,
#     Created, Resolved, Cycle Time (days)
#
# Requirements:
#   - atlassian-cli installed and configured
#   - jq for JSON processing

set -euo pipefail

# Configuration
PROJECT=""
SPRINT_ID=""
PROFILE="default"
OUTPUT_FILE="sprint_report_$(date +%Y%m%d).csv"

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

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --project)
                PROJECT="$2"
                shift 2
                ;;
            --sprint)
                SPRINT_ID="$2"
                shift 2
                ;;
            --profile)
                PROFILE="$2"
                shift 2
                ;;
            --output)
                OUTPUT_FILE="$2"
                shift 2
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    if [ -z "$PROJECT" ]; then
        echo "Project key required (--project PROJECT)"
        exit 1
    fi
}

# Build JQL query
build_jql() {
    local jql="project = $PROJECT"

    if [ -n "$SPRINT_ID" ]; then
        jql="$jql AND Sprint = $SPRINT_ID"
    else
        # Get active sprint if no sprint specified
        jql="$jql AND Sprint in openSprints()"
    fi

    echo "$jql"
}

# Calculate cycle time in days
calculate_cycle_time() {
    local created="$1"
    local resolved="$2"

    if [ -z "$resolved" ] || [ "$resolved" = "null" ]; then
        echo "N/A"
        return
    fi

    # Convert to timestamps (simplified - may need date parsing)
    local created_ts
    created_ts=$(date -d "$created" +%s 2>/dev/null || echo "0")
    local resolved_ts
    resolved_ts=$(date -d "$resolved" +%s 2>/dev/null || echo "0")

    if [ "$created_ts" -eq 0 ] || [ "$resolved_ts" -eq 0 ]; then
        echo "N/A"
        return
    fi

    local diff_days
    diff_days=$(( (resolved_ts - created_ts) / 86400 ))

    echo "$diff_days"
}

# Fetch sprint issues
fetch_issues() {
    local jql="$1"

    log "Fetching sprint issues..."
    log "JQL: $jql"

    atlassian-cli jira search \
        --profile "$PROFILE" \
        --jql "$jql" \
        --output json
}

# Generate CSV report
generate_report() {
    local issues="$1"

    log "Generating report: $OUTPUT_FILE"

    # Write CSV header
    echo "Issue Key,Summary,Type,Status,Story Points,Assignee,Created,Resolved,Cycle Time (days)" > "$OUTPUT_FILE"

    local total
    total=$(echo "$issues" | jq '. | length')

    if [ "$total" -eq 0 ]; then
        warn "No issues found"
        return
    fi

    log "Processing $total issues..."

    # Process each issue
    echo "$issues" | jq -r '.[] | @json' | while IFS= read -r issue_json; do
        local key
        key=$(echo "$issue_json" | jq -r '.key')
        local summary
        summary=$(echo "$issue_json" | jq -r '.fields.summary' | sed 's/"/""/g')
        local issue_type
        issue_type=$(echo "$issue_json" | jq -r '.fields.issuetype.name')
        local status
        status=$(echo "$issue_json" | jq -r '.fields.status.name')
        local story_points
        story_points=$(echo "$issue_json" | jq -r '.fields.customfield_10016 // "N/A"')
        local assignee
        assignee=$(echo "$issue_json" | jq -r '.fields.assignee.displayName // "Unassigned"')
        local created
        created=$(echo "$issue_json" | jq -r '.fields.created')
        local resolved
        resolved=$(echo "$issue_json" | jq -r '.fields.resolutiondate // "null"')

        # Calculate cycle time
        local cycle_time
        cycle_time=$(calculate_cycle_time "$created" "$resolved")

        # Write CSV row
        echo "\"$key\",\"$summary\",\"$issue_type\",\"$status\",$story_points,\"$assignee\",\"$created\",\"$resolved\",$cycle_time" >> "$OUTPUT_FILE"
    done
}

# Generate summary statistics
generate_summary() {
    log "Generating summary statistics"

    local total_issues
    total_issues=$(tail -n +2 "$OUTPUT_FILE" | wc -l | tr -d ' ')

    local total_points
    total_points=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f5 | \
        grep -v "N/A" | awk '{sum+=$1} END {print sum+0}')

    local completed_issues
    completed_issues=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f4 | \
        grep -i "done\|resolved\|closed" | wc -l | tr -d ' ')

    local avg_cycle_time
    avg_cycle_time=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f9 | \
        grep -v "N/A" | awk '{sum+=$1; count++} END {if(count>0) print int(sum/count); else print 0}')

    echo ""
    log "Sprint Summary:"
    echo "  Total Issues: $total_issues"
    echo "  Completed: $completed_issues"
    echo "  Total Story Points: $total_points"
    echo "  Avg Cycle Time: $avg_cycle_time days"
    echo ""

    # Issue breakdown by type
    log "Issues by Type:"
    tail -n +2 "$OUTPUT_FILE" | cut -d',' -f3 | \
        sed 's/"//g' | sort | uniq -c | \
        awk '{printf "  %s: %d\n", $2, $1}'

    echo ""
}

# Main execution
main() {
    parse_args "$@"

    log "Starting sprint report generation"
    log "Project: $PROJECT | Profile: $PROFILE"

    local jql
    jql=$(build_jql)

    local issues
    issues=$(fetch_issues "$jql")

    generate_report "$issues"
    generate_summary

    log "Report saved to: $OUTPUT_FILE"
}

main "$@"
