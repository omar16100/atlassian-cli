#!/bin/bash
# Confluence Space Backup Script
#
# Exports all pages and attachments from a Confluence space to local storage.
# Creates a timestamped backup directory with JSON exports and downloaded attachments.
#
# Usage:
#   ./backup-space.sh SPACE_KEY [PROFILE]
#   ./backup-space.sh DOCS prod
#
# Output:
#   backups/SPACE_KEY_YYYY-MM-DD_HH-MM-SS/
#     ├── pages.json          # All pages with content
#     ├── metadata.json       # Backup metadata
#     └── attachments/        # Downloaded files
#
# Requirements:
#   - atlassian-cli installed and configured

set -euo pipefail

# Configuration
SPACE_KEY="${1:?Space key required}"
PROFILE="${2:-default}"
BACKUP_DIR="backups/${SPACE_KEY}_$(date +%Y-%m-%d_%H-%M-%S)"

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

# Create backup directory structure
setup_backup_dir() {
    log "Creating backup directory: $BACKUP_DIR"
    mkdir -p "$BACKUP_DIR/attachments"
}

# Export space metadata
export_metadata() {
    log "Exporting space metadata"

    atlassian-cli confluence space get \
        --profile "$PROFILE" \
        "$SPACE_KEY" \
        --output json > "$BACKUP_DIR/space_info.json"
}

# Export all pages using bulk export
export_pages() {
    log "Exporting all pages from space $SPACE_KEY"

    local cql="space = $SPACE_KEY AND type = page"

    atlassian-cli confluence bulk export \
        --profile "$PROFILE" \
        --cql "$cql" \
        --output "$BACKUP_DIR/pages.json" \
        --format json

    local page_count
    page_count=$(jq '. | length' "$BACKUP_DIR/pages.json")
    log "Exported $page_count pages"
}

# Export blogposts
export_blogs() {
    log "Exporting blog posts"

    local cql="space = $SPACE_KEY AND type = blogpost"

    atlassian-cli confluence bulk export \
        --profile "$PROFILE" \
        --cql "$cql" \
        --output "$BACKUP_DIR/blogposts.json" \
        --format json

    local blog_count
    blog_count=$(jq '. | length' "$BACKUP_DIR/blogposts.json")
    log "Exported $blog_count blog posts"
}

# Download all attachments
download_attachments() {
    log "Downloading attachments"

    local total=0
    local downloaded=0
    local failed=0

    # Get all page IDs
    local page_ids
    page_ids=$(jq -r '.[].id' "$BACKUP_DIR/pages.json")

    for page_id in $page_ids; do
        # List attachments for this page
        local attachments
        attachments=$(atlassian-cli confluence attachment list \
            --profile "$PROFILE" \
            "$page_id" \
            --output json 2>/dev/null || echo "[]")

        # Process each attachment
        echo "$attachments" | jq -r '.[] | @json' | while IFS= read -r att_json; do
            local att_id
            att_id=$(echo "$att_json" | jq -r '.id')
            local att_title
            att_title=$(echo "$att_json" | jq -r '.title')

            total=$((total + 1))

            # Create safe filename
            local safe_filename
            safe_filename=$(echo "${page_id}_${att_title}" | tr '/' '_' | tr ' ' '_')

            log "Downloading: $att_title"

            if atlassian-cli confluence attachment download \
                --profile "$PROFILE" \
                "$att_id" \
                --output "$BACKUP_DIR/attachments/$safe_filename"; then
                downloaded=$((downloaded + 1))
            else
                warn "Failed to download: $att_title"
                failed=$((failed + 1))
            fi
        done
    done

    log "Attachments: $downloaded downloaded, $failed failed"
}

# Create backup manifest
create_manifest() {
    log "Creating backup manifest"

    local manifest="$BACKUP_DIR/metadata.json"

    jq -n \
        --arg space "$SPACE_KEY" \
        --arg timestamp "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        --arg profile "$PROFILE" \
        --argjson page_count "$(jq '. | length' "$BACKUP_DIR/pages.json")" \
        --argjson blog_count "$(jq '. | length' "$BACKUP_DIR/blogposts.json")" \
        --argjson att_count "$(find "$BACKUP_DIR/attachments" -type f | wc -l | tr -d ' ')" \
        '{
            space_key: $space,
            backup_timestamp: $timestamp,
            profile: $profile,
            statistics: {
                pages: $page_count,
                blogposts: $blog_count,
                attachments: $att_count
            }
        }' > "$manifest"
}

# Calculate backup size
calculate_size() {
    local size
    size=$(du -sh "$BACKUP_DIR" | cut -f1)
    log "Backup size: $size"
}

# Main backup process
main() {
    log "Starting backup for space: $SPACE_KEY"

    setup_backup_dir
    export_metadata
    export_pages
    export_blogs
    download_attachments
    create_manifest
    calculate_size

    log "Backup complete: $BACKUP_DIR"
}

main
