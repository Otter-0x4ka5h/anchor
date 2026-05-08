#!/usr/bin/env bash
#
# One-shot end-to-end reproduction of PR #4258's claimed bug, run against
# this repo's wrapper-based codegen (the no-fix path the PR says is broken).
#
# Expected result: all 5 tests pass, including
#   "can realloc via CPI (caller -> callee)"
# which the PR says must fail without its codegen change.
#
# Run from repo root:   tests/realloc-cpi-bug/repro.sh
#
# What the script does (and reverts on exit):
#   - starts surfpool
#   - patches Anchor.toml's `test` script to use npx instead of `yarn run`
#     (yarn berry locally doesn't resolve binaries the way yarn v1 does)
#   - patches package.json to add a portal dep on the local @anchor-lang/core
#   - runs `anchor keys sync` so declare_id! lines up with freshly-generated
#     keypairs (because no committed test keypair exists)
#   - anchor build / deploy / test
# All file mutations are reverted at the end via `git checkout`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_DIR="$REPO_ROOT/tests/realloc-cpi-bug"
RPC=http://127.0.0.1:8899
WALLET="${ANCHOR_WALLET:-$HOME/.config/solana/id.json}"

cd "$REPO_ROOT"

PATHS_TO_RESTORE=(
  tests/realloc-cpi-bug/Anchor.toml
  tests/realloc-cpi-bug/package.json
  tests/realloc-cpi-bug/programs/callee/src/lib.rs
  tests/realloc-cpi-bug/programs/caller/src/lib.rs
  tests/yarn.lock
)

cleanup() {
  echo "==> Cleaning up"
  pkill -9 -f surfpool >/dev/null 2>&1 || true
  ( cd "$REPO_ROOT" && git checkout -- "${PATHS_TO_RESTORE[@]}" 2>/dev/null || true )
}
trap cleanup EXIT

echo "==> Confirming we're on the no-fix codegen path"
if grep -q 'system_program::transfer' \
     "$REPO_ROOT/lang/syn/src/codegen/accounts/constraints.rs"; then
  echo "    OK: lang/syn still uses the wrapper (PR fix not applied)"
else
  echo "    ERROR: codegen has been modified — this is no longer the no-fix branch"
  exit 1
fi

echo "==> Starting surfpool (after killing any stale instance)"
pkill -9 -f surfpool >/dev/null 2>&1 || true
sleep 2
# wipe stale build artifacts so anchor build/keys sync starts from a known state
rm -rf "$TEST_DIR/target/deploy"
surfpool start --ci --offline > /tmp/surfpool.log 2>&1 &
for _ in {1..30}; do
  if curl -sf "$RPC" -X POST -H "Content-Type: application/json" \
       -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' > /dev/null; then
    echo "    surfpool up"
    break
  fi
  sleep 1
done

echo "==> Patching Anchor.toml (yarn run → npx)"
sed -i.bak 's|yarn run ts-mocha|npx ts-mocha|' "$TEST_DIR/Anchor.toml"
rm -f "$TEST_DIR/Anchor.toml.bak"

echo "==> Patching package.json (portal dep on local @anchor-lang/core)"
node -e '
  const fs = require("fs"), p = process.argv[1];
  const j = JSON.parse(fs.readFileSync(p));
  j.dependencies = Object.assign({}, j.dependencies, {
    "@anchor-lang/core": "portal:../../ts/packages/anchor"
  });
  fs.writeFileSync(p, JSON.stringify(j, null, 2) + "\n");
' "$TEST_DIR/package.json"

echo "==> yarn install (workspace; picks up the portal)"
( cd "$REPO_ROOT/tests" && yarn install --silent )

echo "==> anchor keys sync (aligns declare_id! with freshly-generated keypair)"
( cd "$TEST_DIR" && anchor keys sync )

echo "==> anchor build"
( cd "$TEST_DIR" && anchor build )

echo "==> Funding wallet & deploying"
solana airdrop 100 -u "$RPC" > /dev/null
( cd "$TEST_DIR" && anchor deploy )

echo "==> Running test (the moment PR #4258 claims a CPI realloc must fail)"
cd "$TEST_DIR"
NODE_OPTIONS=--preserve-symlinks \
  ANCHOR_PROVIDER_URL="$RPC" \
  ANCHOR_WALLET="$WALLET" \
  anchor test --skip-build --skip-deploy --skip-local-validator
