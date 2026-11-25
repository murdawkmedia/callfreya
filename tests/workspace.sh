#!/usr/bin/env bash
set -euo pipefail
url=${1:?Pass a URL for a server with an empty test workspace}
headers=(-H 'X-FreyaLine-Call: 1' -H 'Content-Type: application/json')
post() { curl -fsS "${headers[@]}" -d "$2" "$url/$1"; }
slot=$(($(date +%s) + 86400))
booking=$(post api/workspace/bookings "{\"name\":\"Demo Caller\",\"email\":\"caller@example.test\",\"slot\":$slot}")
id=$(jq -er '.id | select(. > 0)' <<<"$booking")
jq -e '.status == "requested"' <<<"$booking"
post "api/workspace/bookings/$id/confirm" '{}' | jq -e '.status == "confirmed"'
second=$(post api/workspace/bookings "{\"name\":\"Second Caller\",\"email\":\"second@example.test\",\"slot\":$slot}" | jq -er '.id')
status=$(curl -sS -o /dev/null -w '%{http_code}' "${headers[@]}" -d '{}' "$url/api/workspace/bookings/$second/confirm")
test "$status" = 409
handoff=$(post api/workspace/handoffs '{"name":"=Synthetic Caller","email":"handoff@example.test","reason":"Asked to speak to a person"}')
id=$(jq -er '.id' <<<"$handoff")
post "api/workspace/handoffs/$id/resolve" '{}' | jq -e '.status == "resolved"'
curl -fsS "${headers[@]}" "$url/api/workspace/export.csv" | rg -F "\"'=Synthetic Caller\""
curl -fsS "$url/demo" | rg -F 'Example product demo'
