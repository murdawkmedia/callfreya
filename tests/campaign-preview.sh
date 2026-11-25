#!/usr/bin/env bash
set -euo pipefail
url=${1:-http://127.0.0.1:7878}
curl -fsS "$url/api/campaigns/preview" \
    -H 'Content-Type: application/json' \
    -d '{
        "script":"Ask if the contact wants a demo",
        "calling_window_open":true,
        "max_retries":1,
        "contacts":[
            {"phone":"+12025550101","consent":true},
            {"phone":"+12025550101","consent":true},
            {"phone":"+12025550102","consent":false},
            {"phone":"+12025550103","consent":true,"opted_out":true},
            {"phone":"+12025550104","consent":true,"attempts":2},
            {"phone":"bad number","consent":true}
        ]
    }' | jq -e '.contacts | map(.decision) == ["ready","duplicate","no_consent","opted_out","retry_limit","invalid_number"]'
curl -fsS "$url/api/campaigns/preview" \
    -H 'Content-Type: application/json' \
    -d '{
        "script":"Ask if the contact wants a demo",
        "calling_window_open":false,
        "max_retries":0,
        "contacts":[
            {"phone":"+12025550101","consent":true},
            {"phone":"+12025550102","consent":true},
            {"phone":"+12025550101","consent":false,"opted_out":true}
        ]
    }' | jq -e '.ready == 0 and (.contacts | map(.decision) == ["opted_out","outside_window","opted_out"])'
