#!/usr/bin/env sh
set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

docker compose ps --format json | jq -r '["NAME","SERVICE","PORTS"], (.[] | [.Name, .Service, (.Publishers | map(select(.PublishedPort > 0) | "\(.PublishedPort):\(.TargetPort)/\(.Protocol)") | join(", "))]) | @tsv' | column -t
