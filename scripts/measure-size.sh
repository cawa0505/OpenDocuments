#!/usr/bin/env bash
# Measure stripped release binary size as a CI gate / local audit.
# Usage: ./scripts/measure-size.sh [threshold_mib]
#   threshold_mib: stripped binary must be <= this value (default 210).
# Exit codes: 0 = passes gate; 1 = exceeds gate.
set -euo pipefail

THRESHOLD_MIB="${1:-210}"
THRESHOLD_BYTES=$((THRESHOLD_MIB * 1024 * 1024))

TARGET="target/release/opendoc"

if [ ! -f "$TARGET" ]; then
  echo "ERROR: $TARGET not found. Run: cargo build --release -p opendoc"
  exit 1
fi

STRIPPED="$(mktemp)"
cp "$TARGET" "$STRIPPED"
strip --strip-all "$STRIPPED"

SIZE=$(stat -c%s "$STRIPPED")
STRIPPED_MIB=$((SIZE / 1024 / 1024))
echo "stripped_bytes=$SIZE"
echo "stripped_mib=$STRIPPED_MIB"
echo "threshold_mib=$THRESHOLD_MIB"

rm -f "$STRIPPED"

if [ "$SIZE" -gt "$THRESHOLD_BYTES" ]; then
  echo "FAIL: stripped binary ${SIZE} bytes exceeds ${THRESHOLD_MIB} MiB gate"
  exit 1
fi

echo "PASS: stripped binary ${SIZE} bytes within ${THRESHOLD_MIB} MiB gate"
