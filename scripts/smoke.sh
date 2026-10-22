#!/usr/bin/env bash
# Local end-to-end smoke test for node + router (file config, no control plane).
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/gen-dev-certs.sh >/dev/null
cargo build --workspace

target/debug/su8-egress-node --config deploy/node/config.dev.toml \
  >/tmp/su8-egress-node.log 2>&1 &
NODE_PID=$!
target/debug/su8-egress-router --config deploy/router/config.dev.toml \
  >/tmp/su8-egress-router.log 2>&1 &
ROUTER_PID=$!
trap 'kill "$NODE_PID" "$ROUTER_PID" 2>/dev/null || true' EXIT

sleep 2

echo "allowed target:"
curl -fsS -m 15 -x http://127.0.0.1:3128 -I https://example.com | head -1

echo "blocked target:"
blocked=$(curl -sS -m 8 -x http://127.0.0.1:3128 -I https://example.org 2>&1 || true)
echo "$blocked" | grep -q '403 Forbidden'
echo "smoke ok"
