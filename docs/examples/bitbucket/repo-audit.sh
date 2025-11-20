#!/bin/bash
# Bitbucket Repository Audit Script
#
# Generates a comprehensive CSV audit report of all repositories in a workspace.
# Includes repository details, permissions, and activity metrics.
#
# Usage:
#   ./repo-audit.sh --workspace myworkspace --output repos.csv
#   ./repo-audit.sh --workspace myworkspace --profile prod
#
# Output CSV columns:
#   - Repo Name, Full Name, Size, Language, Public, Fork, Has Wiki,
#     Has Issues, Created, Updated, Default Branch
#
# Requirements:
#   - atlassian-cli installed and configured
#   - jq for JSON processing

set -euo pipefail

# Configuration
WORKSPACE="${1:?Workspace slug required}"
PROFILE="${2:-default}"
OUTPUT_FILE="${3:-repos_audit_$(date +%Y%m%d).csv}"

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

# Get all repositories
get_all_repos() {
    log "Fetching all repositories from workspace: $WORKSPACE"

    atlassian-cli bitbucket repo list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        --output json
}

# Get repository permissions
get_repo_permissions() {
    local repo="$1"

    atlassian-cli bitbucket permissions list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$repo" \
        --output json 2>/dev/null || echo "[]"
}

# Generate CSV report
generate_report() {
    local repos="$1"

    log "Generating audit report: $OUTPUT_FILE"

    # Write CSV header
    echo "Repo Name,Full Name,Size (MB),Language,Public,Fork,Has Wiki,Has Issues,Created,Updated,Default Branch,Permission Count" > "$OUTPUT_FILE"

    local total
    total=$(echo "$repos" | jq '. | length')
    local processed=0

    echo "$repos" | jq -r '.[] | @json' | while IFS= read -r repo_json; do
        local name
        name=$(echo "$repo_json" | jq -r '.slug')
        local full_name
        full_name=$(echo "$repo_json" | jq -r '.full_name' | sed 's/"/""/g')
        local size
        size=$(echo "$repo_json" | jq -r '.size // 0')
        # Convert bytes to MB
        size=$((size / 1024 / 1024))
        local language
        language=$(echo "$repo_json" | jq -r '.language // "N/A"')
        local is_public
        is_public=$(echo "$repo_json" | jq -r '.is_private | if . then "No" else "Yes" end')
        local is_fork
        is_fork=$(echo "$repo_json" | jq -r 'if has("parent") then "Yes" else "No" end')
        local has_wiki
        has_wiki=$(echo "$repo_json" | jq -r '.has_wiki // false | if . then "Yes" else "No" end')
        local has_issues
        has_issues=$(echo "$repo_json" | jq -r '.has_issues // false | if . then "Yes" else "No" end')
        local created
        created=$(echo "$repo_json" | jq -r '.created_on')
        local updated
        updated=$(echo "$repo_json" | jq -r '.updated_on')
        local default_branch
        default_branch=$(echo "$repo_json" | jq -r '.mainbranch.name // "N/A"')

        processed=$((processed + 1))
        log "Processing [$processed/$total]: $name"

        # Get permissions count
        local permissions
        permissions=$(get_repo_permissions "$name")
        local perm_count
        perm_count=$(echo "$permissions" | jq '. | length')

        # Write CSV row
        echo "\"$name\",\"$full_name\",$size,\"$language\",$is_public,$is_fork,$has_wiki,$has_issues,\"$created\",\"$updated\",\"$default_branch\",$perm_count" >> "$OUTPUT_FILE"
    done
}

# Generate summary statistics
generate_summary() {
    log "Generating summary statistics"

    local total_repos
    total_repos=$(tail -n +2 "$OUTPUT_FILE" | wc -l | tr -d ' ')

    local total_size
    total_size=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f3 | \
        awk '{sum+=$1} END {print int(sum)}')

    local public_repos
    public_repos=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f5 | \
        grep -c "Yes" || echo "0")

    local forks
    forks=$(tail -n +2 "$OUTPUT_FILE" | cut -d',' -f6 | \
        grep -c "Yes" || echo "0")

    echo ""
    log "Audit Summary:"
    echo "  Total Repositories: $total_repos"
    echo "  Public Repositories: $public_repos"
    echo "  Forks: $forks"
    echo "  Total Size: ${total_size} MB"
    echo ""

    # Language breakdown
    log "Repositories by Language:"
    tail -n +2 "$OUTPUT_FILE" | cut -d',' -f4 | \
        sed 's/"//g' | sort | uniq -c | \
        awk '{printf "  %s: %d\n", $2, $1}'

    echo ""
}

# Main execution
main() {
    log "Starting repository audit"
    log "Workspace: $WORKSPACE | Profile: $PROFILE"

    local repos
    repos=$(get_all_repos)

    if [ "$(echo "$repos" | jq '. | length')" -eq 0 ]; then
        warn "No repositories found in workspace: $WORKSPACE"
        exit 0
    fi

    generate_report "$repos"
    generate_summary

    log "Audit report saved to: $OUTPUT_FILE"
}

main
