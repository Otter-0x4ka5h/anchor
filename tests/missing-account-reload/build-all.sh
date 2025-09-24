#!/usr/bin/env bash

set -euo pipefail

# Build and check "good-one"
echo "==> Checking good-one"
cd good-one
OUTPUT=$(anchor build 2>&1 || true)

if echo "$OUTPUT" | grep -q 'Finished `release` profile' && echo "$OUTPUT" | grep -q 'Finished `test` profile'; then
  echo "[OK] good-one: Found release+test finished markers"
else
  echo "[ERROR] good-one: Missing release/test finished markers"
  echo "$OUTPUT"
  exit 1
fi
cd ..

# Build and check "bad-one"
echo "==> Checking bad-one"
cd bad-one
OUTPUT=$(anchor build 2>&1 || true)

if echo "$OUTPUT" | grep -Eq '(Safety checks failed|Missing account reload|custom attribute panicked)'; then
  echo "[OK] bad-one: Detected expected safety check failure"
else
  echo "[ERROR] bad-one: Did not detect expected safety check failure"
  echo "$OUTPUT"
  exit 1
fi
cd ..

# Build and check "good-two"
echo "==> Checking good-two"
cd good-two
OUTPUT=$(anchor build 2>&1 || true)

if echo "$OUTPUT" | grep -q 'Finished `release` profile' && echo "$OUTPUT" | grep -q 'Finished `test` profile'; then
  echo "[OK] good-two: Found release+test finished markers"
else
  echo "[ERROR] good-two: Missing release/test finished markers"
  echo "$OUTPUT"
  exit 1
fi
cd ..

# Build and check "bad-two"
echo "==> Checking bad-two"
cd bad-two
OUTPUT=$(anchor build 2>&1 || true)

if echo "$OUTPUT" | grep -Eq '(Safety checks failed|Missing account reload|custom attribute panicked)'; then
  echo "[OK] bad-two: Detected expected safety check failure"
else
  echo "[ERROR] bad-two: Did not detect expected safety check failure"
  echo "$OUTPUT"
  exit 1
fi
cd ..

echo "All checks passed."
