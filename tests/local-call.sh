#!/usr/bin/env bash
set -euo pipefail
url=${1:-http://127.0.0.1:7878}
work=$(mktemp -d)
trap 'rm -r -- "$work"' EXIT

curl -fsS "$url/" | grep -q 'alt="CallFreya Business Receptionist"'
curl -fsS "$url/api/greeting" > "$work/greeting.json"
jq -e '.audio | startswith("data:audio/wav;base64,")' "$work/greeting.json" >/dev/null
for case_name in interested declined needs_review; do
    case "$case_name" in
        interested) phrase="I would like to book a demo next week.";;
        needs_review) phrase="What time is it where you are?";;
        declined) phrase="No thank you, please do not call me again.";;
    esac
    "${FREYALINE_TTS:-espeak-ng}" -w "$work/caller.wav" "$phrase"
    input="$work/caller.wav"
    if [[ $case_name == declined ]]; then
        ffmpeg -nostdin -v error -i "$input" -c:a libopus "$work/caller.webm"
        input="$work/caller.webm"
    fi
    curl -fsS -H 'X-FreyaLine-Call: 1' --data-binary @"$input" "$url/api/call" > "$work/result.json"
    jq -e --arg outcome "$case_name" '
      .outcome == $outcome and
      (.transcript | map(.speaker) == ["agent", "caller", "agent"]) and
      (.transcript[1].text | length > 0) and
      (.audio | startswith("data:audio/wav;base64,"))
    ' "$work/result.json" >/dev/null
    jq -r '[.outcome, .transcript[1].text] | @tsv' "$work/result.json"
done
status=$(curl -sS -o /dev/null -w '%{http_code}' -H 'X-FreyaLine-Call: 1' --data-binary bad-audio "$url/api/call")
test "$status" = 422
