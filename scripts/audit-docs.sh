#!/bin/bash
# scripts/audit-docs.sh
# Standardized documentation alignment checker for OpenDocuments.
# Ensure zero discrepancies between actual Rust code capabilities and docs-site/README.md.

set -eo pipefail

DOCS_DIR="docs-site"
README_FILE="README.md"
STRUCTURE_FILE="docs/en/structure.md"
ROADMAP_FILE="docs/en/roadmap.md"

echo "🔍 Running OpenDocuments Documentation Alignment Audit..."

# 1. Compile cli to verify help commands
echo "   - Building opendoc CLI..."
cargo build --quiet

OPENDOC_BIN="./target/debug/opendoc"

# Get active subcommands from binary
ACTIVE_CMDS=$($OPENDOC_BIN --help | grep -E "^\s+[a-z-]+" | awk '{print $1}' | tr '\n' ' ')
echo "   - Active CLI subcommands: $ACTIVE_CMDS"

# 2. Check for banned/obsolete keywords across all docs
echo "   - Checking for obsolete keywords..."

BANNED_PATTERNS=(
  "npm install -g opendocuments"
  "npx opendocuments"
  "opendocuments init"
  "opendocuments start"
  "GraphifyOpt"
  "saaslab/loomcowork"
  "LoomCowork (閉源商用模組)"
)

VIOLATIONS=0

for pattern in "${BANNED_PATTERNS[@]}"; do
  # Search in docs-site and markdown files
  MATCHES=$(grep -rn "$pattern" "$DOCS_DIR" "$README_FILE" "$STRUCTURE_FILE" "$ROADMAP_FILE" 2>/dev/null || true)
  if [ -n "$MATCHES" ] && [ "$MATCHES" != "" ]; then
    echo "❌ ERROR: Found banned pattern '$pattern' in files:"
    echo "$MATCHES"
    VIOLATIONS=$((VIOLATIONS + 1))
  fi
done

# 3. Check for mentions of legacy docker examples that we do not support anymore
DOCKER_RUN=$(grep -rn "docker run" "$DOCS_DIR" "$README_FILE" 2>/dev/null || true)
if [ -n "$DOCKER_RUN" ] && [ "$DOCKER_RUN" != "" ]; then
  echo "⚠️ WARNING: Found 'docker run' references in docs:"
  echo "$DOCKER_RUN"
fi

# 4. Check for invalid or legacy cli binary name "opendocuments" in commands
OPENDOCUMENTS_CMD=$(grep -rnE "(\s|^)opendocuments\s+(start|index|ask|search|workspace|document|install-opencode)" "$DOCS_DIR" "$README_FILE" 2>/dev/null || true)
if [ -n "$OPENDOCUMENTS_CMD" ] && [ "$OPENDOCUMENTS_CMD" != "" ]; then
  echo "❌ ERROR: Found legacy CLI command executor name 'opendocuments' (should be 'opendoc'):"
  echo "$OPENDOCUMENTS_CMD"
  VIOLATIONS=$((VIOLATIONS + 1))
fi

if [ $VIOLATIONS -gt 0 ]; then
  echo "💥 Audit FAILED with $VIOLATIONS error(s). Please align the docs with the actual Rust code capabilities."
  exit 1
else
  echo "✅ Audit PASSED! All docs are correctly aligned with active Rust capabilities."
fi
