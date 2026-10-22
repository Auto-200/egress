#!/usr/bin/env bash
# Generates a local CA, a node server certificate (localhost / 127.0.0.1),
# and a router client certificate. For local development only.
set -euo pipefail

out="${1:-certs}"
mkdir -p "$out"

openssl req -x509 -newkey rsa:4096 -nodes -days 3650 \
  -keyout "$out/ca.key" -out "$out/ca.crt" \
  -subj "/CN=su8-egress-dev-ca"

openssl req -newkey rsa:2048 -nodes \
  -keyout "$out/node.key" -out "$out/node.csr" \
  -subj "/CN=localhost"
openssl x509 -req -in "$out/node.csr" \
  -CA "$out/ca.crt" -CAkey "$out/ca.key" -CAcreateserial \
  -out "$out/node.crt" -days 3650 \
  -extfile <(printf "basicConstraints=CA:FALSE\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n")

openssl req -newkey rsa:2048 -nodes \
  -keyout "$out/router.key" -out "$out/router.csr" \
  -subj "/CN=su8-egress-router"
openssl x509 -req -in "$out/router.csr" \
  -CA "$out/ca.crt" -CAkey "$out/ca.key" -CAcreateserial \
  -out "$out/router.crt" -days 3650 \
  -extfile <(printf "basicConstraints=CA:FALSE\nextendedKeyUsage=clientAuth\n")

rm -f "$out"/*.csr "$out"/*.srl
chmod 600 "$out"/*.key

echo "wrote $out/{ca,node,router}.{crt,key}"
echo "run:"
echo "  cargo run -p egress-node   -- --config deploy/node/config.dev.toml"
echo "  cargo run -p egress-router -- --config deploy/router/config.dev.toml"
