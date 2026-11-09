#!/usr/bin/env bash
set -euo pipefail

# Online SQLite backup + blob copy. Keep the last 14 daily snapshots.
#   sudo bash sync-server/deploy/backup.sh [/var/backups/yebian-sync]
#
# Add to cron:
#   15 3 * * * root bash /opt/yebian-sync-src/sync-server/deploy/backup.sh >/var/log/yebian-sync-backup.log 2>&1

DATA_DIR="/var/lib/yebian-sync"
DEST="${1:-/var/backups/yebian-sync}"

mkdir -p "$DEST"
stamp=$(date +%F-%H%M%S)
sqlite3 "$DATA_DIR/sync.sqlite" ".backup '$DEST/sync-$stamp.sqlite'"
if command -v rsync >/dev/null; then
  rsync -a --delete "$DATA_DIR/blobs/" "$DEST/blobs/"
else
  mkdir -p "$DEST/blobs"
  cp -a "$DATA_DIR/blobs/." "$DEST/blobs/"
fi
find "$DEST" -maxdepth 1 -name 'sync-*.sqlite' -mtime +14 -delete
echo "backup written to $DEST (snapshot sync-$stamp.sqlite)"
