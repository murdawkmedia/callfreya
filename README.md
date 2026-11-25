# CallFreya

CallFreya is a local voice-agent playground written in Rust. Play an AI
disclosure and greeting, record or upload one reply, and hear the agent's
response. The session returns a speaker-labelled transcript and an outcome.

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

The prototype is a one-reply browser simulation. Phone calls, streaming
conversation, calendar booking, and persistent records are future work.

## Campaign preview

`POST /api/campaigns/preview` accepts JSON with a nonblank `script`, a
`calling_window_open` boolean, a `max_retries` count, and up to 1,000
`contacts`. Contacts have a `phone` in international format, a `consent`
boolean, and optional `opted_out` and `attempts` fields. The response gives
a readiness decision for each row. Opt-outs apply to all matching rows.
The window flag is supplied by the caller for simulation. The endpoint
does not store, schedule, or dial contacts.

Run `bash tests/campaign-preview.sh` for synthetic examples.
