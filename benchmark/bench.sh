#!/usr/bin/env bash

set -euo pipefail

ENDPOINT="$1"
AUTH_HEADER="$2"

if [ -z "$ENDPOINT" ] || [ -z "$AUTH_HEADER" ]; then
  echo "Usage: $0 <endpoint> <auth-header>"
  echo "Example: $0 http://localhost:8000/users/1 'Bearer eyJ...'"
  exit 1
fi

echo "Benchmarking $ENDPOINT"

wrk2 -c 256 -t 12 -d 180s --latency \
  -H "Authorization: $AUTH_HEADER" \
  -H "Content-Type: application/json" \
  "$ENDPOINT"
