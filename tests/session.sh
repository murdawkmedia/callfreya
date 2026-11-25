#!/usr/bin/env bash
set -euo pipefail
url=${1:-http://127.0.0.1:7878}
work=$(mktemp -d)
trap 'rm -r -- "$work"' EXIT
request=(-H 'X-FreyaLine-Call: 1')
start=$(curl -fsS "${request[@]}" -X POST "$url/api/sessions")
id=$(jq -er '.id' <<<"$start")
jq -e '.outcome == "in_progress" and (.transcript | length == 1)' <<<"$start"
synth=${FREYALINE_TTS:-espeak-ng}
"$synth" -w "$work/reply.wav" 'Yes, I would like a demo.'
reply=$(curl -fsS "${request[@]}" --data-binary @"$work/reply.wav" "$url/api/sessions/$id/turn")
jq -e '.outcome == "in_progress" and (.transcript | length == 3)' <<<"$reply"
"$synth" -w "$work/reply.wav" 'Yes, please follow up.'
reply=$(curl -fsS "${request[@]}" --data-binary @"$work/reply.wav" "$url/api/sessions/$id/turn")
jq -e '.outcome == "interested" and (.transcript | length == 5)' <<<"$reply"
status=$(curl -sS -o /dev/null -w '%{http_code}' "${request[@]}" --data-binary @"$work/reply.wav" "$url/api/sessions/$id/turn")
test "$status" = 409
