# CallFreya

CallFreya is a local voice-agent playground written in Rust. Play an AI
disclosure and greeting, record or upload each reply, and hear the agent's responses. The session returns a speaker-labelled transcript and an outcome.

The demo uses a local Parakeet transcription engine, eSpeak NG for speech
synthesis, and explicit rules for three outcomes: interested, declined, and
needs review. It binds to loopback and deletes its temporary audio and
recognition output when the request finishes normally. The transcript stays
visible in the browser until the next recording or page reload.

## Source of truth

The [CallFreya repository on GitWorkshop](https://gitworkshop.dev/npub17j3f7cuhcvjqnf7d80x6rt9hmkkhdguwg4l2th0h4qnhqketr63s6xakwv/relay.ngit.dev/callfreya) is the
source of truth. Submit changes there through ngit. The
[MurdawkMedia GitHub repository](https://github.com/murdawkmedia/callfreya)
is a distribution mirror.

## Run

Install Rust, FFmpeg, eSpeak NG, and an offline Parakeet-backed Voxtype build
with support for the private `--result-json` result file. Point the app at the
binary and a Voxtype config with an absolute path to the local model directory:

```toml
engine = "parakeet"
[hotkey]
enabled = false
[vad]
enabled = false
[whisper]
threads = 4
[parakeet]
model = "/absolute/path/to/parakeet-model"
model_type = "tdt"
streaming = false
```

```bash
export FREYALINE_ASR=/absolute/path/to/voxtype
export FREYALINE_ASR_CONFIG=/absolute/path/to/parakeet.toml
cargo run --locked
```

Open <http://127.0.0.1:7878>. `FREYALINE_PORT` overrides the port.
`FREYALINE_TTS` overrides the eSpeak NG executable. The browser supports
microphone capture on loopback in browsers with MediaRecorder support,
or you can upload a short WAV, WebM, Ogg, M4A, MP3, or FLAC clip.

## Test

With the server running, run `bash tests/local-call.sh`. The test requires
curl, jq, FFmpeg with Opus encoding, and eSpeak NG. It generates all caller
audio from synthetic English phrases and exercises the HTTP endpoints.

Run `bash tests/session.sh` against the running server to check a two-reply
call, transcript accumulation, and rejection of audio after the call ends.

## Call sessions

Select **Start call** and record or upload each turn. A request for a demo
prompts Freya to confirm whether to follow up. The session ends when the
caller confirms, declines, or reaches six replies. Outcome labels reflect
the conversation. No booking or follow-up is sent.

Session transcripts live in memory. Session IDs expire 30 minutes after
creation and the server prunes expired sessions on subsequent session
requests. Restarting the server clears them. The original one-reply endpoint
remains available for API clients.

## Planned integrations

1. Connect one opted-in inbound or outbound Twilio test call to the session
   engine. Validate webhook signatures and match call status to the transcript.
2. Add durable call results and campaign queues with consent, opt-outs,
   time-zone-aware calling windows, retry limits, and start/pause controls.
3. Connect the local receptionist workspace to calendars, CRM providers, and
   live human transfer. Add configurable browser demos.

## Campaign preview

`POST /api/campaigns/preview` accepts JSON with a nonblank `script`, a
`calling_window_open` boolean, a `max_retries` count, and up to 1,000
`contacts`. Contacts have a `phone` in international format, a `consent`
boolean, and optional `opted_out` and `attempts` fields. The response gives
a readiness decision for each row. Opt-outs apply to all matching rows.
The window flag is supplied by the caller for simulation. The endpoint
does not store, schedule, or dial contacts.

Run `bash tests/campaign-preview.sh` for synthetic examples.

## Reception workspace

Open `/workspace` to request and confirm a 30-minute appointment, queue and
resolve a human follow-up, or download the CRM-import CSV. Call results
link to these actions when applicable. Open `/demo` for the interactive
synthetic product walkthrough.

Booking confirmation checks the workspace for overlap. No calendar
provider, email notification, or live phone transfer is configured.
Handoff is an operator queue. CSV export includes the local booking and
handoff records. The demo is a browser walkthrough with sample content.

The workspace stores records in a user-local JSON file. Set
`CALLFREYA_WORKSPACE` to select a different file. Run
`bash tests/workspace.sh http://127.0.0.1:7879` against a separate server
started with an empty test workspace. Use synthetic contact details.

## Hermes draft assistant

The reception workspace can request a draft from a local Hermes Agent API
server. Use a dedicated Hermes gateway profile with tools and memory disabled
and an API key. The current integration sends only built-in fictional
scenarios. Drafts are displayed for review and never sent by CallFreya.

Set `CALLFREYA_HERMES_PORT` to the local gateway port and
`CALLFREYA_HERMES_KEY_FILE` to a private file containing the gateway key.
`CALLFREYA_HERMES_MODEL` defaults to `hermes`. The gateway adapter requires
curl and targets only the loopback chat completions endpoint. Omit the port to
disable it.

`tests/hermes-fixture.py` supplies a synthetic gateway for
`tests/hermes.sh`. The fixture checks the request path, authentication header,
and synthetic prompt without invoking a model.

## Product walkthrough content

Set `CALLFREYA_DEMO_FILE` to a JSON file shaped like
[`src/demo.json`](src/demo.json), then restart CallFreya. Keep it limited to
approved visitor-facing content. The same-origin demo API delivers the
product name, introduction, and ordered steps to the browser.
