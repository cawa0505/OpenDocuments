#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB_PKG="$ROOT_DIR/packages/web"
DIST_SRC="$WEB_PKG/dist"
DIST_DST="${OPENDOCUMENTS_DIST:-$HOME/.opendocuments/dist}"

echo "=== OpenDocuments Frontend Deploy ==="

# 1. Build
echo "[1/3] Building frontend..."
npm run build --workspace=@opendocuments/web

if [[ ! -d "$DIST_SRC" ]]; then
  echo "[!!] Build output not found: $DIST_SRC"
  exit 1
fi

# 2. Deploy
echo "[2/3] Deploying to $DIST_DST ..."
mkdir -p "$DIST_DST"
rsync -a --delete "$DIST_SRC/" "$DIST_DST/"

echo "[3/3] Done. Frontend files:"
ls -lh "$DIST_DST/index.html" 2>/dev/null || true

# Optional: restart service if running
if systemctl --user is-active opendoc-server.service &>/dev/null; then
  echo "[+] Restarting opendoc-server.service ..."
  systemctl --user restart opendoc-server.service
  echo "[+] Service restarted."
else
  echo "[~] opendoc-server.service not running, skipping restart."
fi

echo "=== Deploy complete ==="
