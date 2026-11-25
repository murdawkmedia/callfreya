#!/usr/bin/env bash
set -euo pipefail
url=${1:?Pass the URL of CallFreya configured for the fixture Hermes gateway}
headers=(-H 'X-FreyaLine-Call: 1' -H 'Content-Type: application/json')
curl -fsS "${headers[@]}" -d '"demo_followup"' "$url/api/hermes/draft" |
    jq -e '.draft == "Synthetic demo follow-up."'
status=$(curl -sS -o /dev/null -w '%{http_code}' "${headers[@]}" \
    -d '"arbitrary_prompt"' "$url/api/hermes/draft")
test "$status" = 422
curl -fsS "$url/api/demo" | jq -e '.product == "Harbor Desk" and (.steps | length == 3)'
