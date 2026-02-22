#!/usr/bin/env sh
set -e

FIRST_NAME="${1:?Usage: create-user.sh <first_name> <last_name>}"
LAST_NAME="${2:?Usage: create-user.sh <first_name> <last_name>}"

curl -s -X POST http://localhost:3000/user \
  -H "Content-Type: application/json" \
  -d "{\"first_name\":\"$FIRST_NAME\",\"last_name\":\"$LAST_NAME\"}"
