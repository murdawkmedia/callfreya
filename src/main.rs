mod campaign;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::Html,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::{env, io, os::unix::fs::PermissionsExt, path::Path, sync::Arc, time::Duration};
use tokio::{process::Command, sync::Semaphore, time::timeout};

const GREETING: &str = "Hi, I'm Freya, a demo voice agent. Are you interested in a product demo?";
type Failure = (StatusCode, &'static str);

#[derive(Clone)]
struct Speech {
    transcriber: String,
    config: String,
    synthesizer: String,
    inference_slot: Arc<Semaphore>,
}

#[derive(Serialize)]
struct Turn<'a> {
    speaker: &'static str,
    text: &'a str,
}

#[derive(Serialize)]
struct Greeting {
    audio: String,
    text: &'static str,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let speech = Speech {
        transcriber: env::var("FREYALINE_ASR")?,
        config: env::var("FREYALINE_ASR_CONFIG")?,
        synthesizer: env::var("FREYALINE_TTS").unwrap_or_else(|_| "espeak-ng".into()),
        inference_slot: Arc::new(Semaphore::new(1)),
    };
    let app = Router::new()
        .route("/", get(|| async { Html(include_str!("index.html")) }))
        .route(
            "/callfreya-logo.png",
            get(|| async {
                (
                    [("content-type", "image/png")],
                    include_bytes!("callfreya-logo.png").as_slice(),
                )
            }),
        )
        .route("/api/greeting", get(greeting))
        .route("/api/call", post(call))
        .route("/api/campaigns/preview", post(campaign::preview))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(speech);
    let port: u16 = env::var("FREYALINE_PORT")
        .unwrap_or_else(|_| "7878".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("CallFreya running at http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn greeting(State(speech): State<Speech>) -> Result<Json<Greeting>, Failure> {
    let work = tempfile::tempdir().map_err(unavailable)?;
    let audio = speak(&speech, work.path(), GREETING).await?;
    Ok(Json(Greeting {
        audio,
        text: GREETING,
    }))
}

async fn call(
    State(speech): State<Speech>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, Failure> {
    if headers
        .get("x-freyaline-call")
        .is_none_or(|value| value != "1")
    {
        return Err((StatusCode::FORBIDDEN, "Missing call request header."));
    }
    let _permit = speech.inference_slot.try_acquire().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            "A call is being processed. Try again shortly.",
        )
    })?;
    let work = tempfile::tempdir().map_err(unavailable)?;
    std::fs::set_permissions(work.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(unavailable)?;
    let source = work.path().join("source.audio");
    let input = work.path().join("caller.wav");
    let result = work.path().join("result.json");
    std::fs::write(&source, body).map_err(unavailable)?;
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg
        .args([
            "-nostdin",
            "-v",
            "error",
            "-protocol_whitelist",
            "file",
            "-format_whitelist",
            "wav,matroska,webm,ogg,mov,mp3,flac",
            "-i",
        ])
        .arg(source)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-t",
            "20",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&input);
    if !run(&mut ffmpeg)
        .await
        .map_err(unavailable)?
        .status
        .success()
    {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "Upload or record a short audio clip.",
        ));
    }

    let mut recognize = Command::new(&speech.transcriber);
    recognize
        .arg("--config")
        .arg(&speech.config)
        .args(["--quiet", "--no-auto-submit", "transcribe"])
        .arg(input)
        .args(["--engine", "parakeet", "--result-json"])
        .arg(&result);
    checked(&mut recognize).await?;
    let recognition: serde_json::Value =
        serde_json::from_slice(&std::fs::read(result).map_err(unavailable)?)
            .map_err(unavailable)?;
    let heard = recognition
        .get("text")
        .and_then(serde_json::Value::as_str)
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "Unexpected transcription result.",
        ))?
        .trim();
    if heard.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "No speech recognized. Please try again.",
        ));
    }
    let (outcome, reply) = respond(heard);
    let audio = speak(&speech, work.path(), reply).await?;
    Ok(Json(serde_json::json!({
        "audio": audio,
        "transcript": [
            Turn { speaker: "agent", text: GREETING },
            Turn { speaker: "caller", text: heard },
            Turn { speaker: "agent", text: reply }
        ],
        "outcome": outcome
    })))
}

fn respond(heard: &str) -> (&'static str, &'static str) {
    let normalized = heard.to_lowercase();
    let words = normalized
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let contains = |phrase: &str| format!(" {words} ").contains(&format!(" {phrase} "));
    if [
        "no",
        "not interested",
        "do not",
        "don t",
        "stop",
        "unsubscribe",
    ]
    .iter()
    .any(|p| contains(p))
    {
        (
            "declined",
            "Understood. Thanks for letting me know. Goodbye.",
        )
    } else if [
        "yes",
        "book a demo",
        "schedule a demo",
        "interested",
        "see a demo",
    ]
    .iter()
    .any(|p| contains(p))
    {
        (
            "interested",
            "Thanks. I noted that you'd like a demo. Goodbye.",
        )
    } else {
        (
            "needs_review",
            "Thanks for your time. I'll leave your response for review. Goodbye.",
        )
    }
}

async fn speak(speech: &Speech, dir: &Path, text: &str) -> Result<String, Failure> {
    let target = dir.join("agent.wav");
    let mut synthesize = Command::new(&speech.synthesizer);
    synthesize.arg("-w").arg(&target).arg(text);
    checked(&mut synthesize).await?;
    let audio = std::fs::read(target).map_err(unavailable)?;
    Ok(format!("data:audio/wav;base64,{}", STANDARD.encode(audio)))
}

async fn run(command: &mut Command) -> io::Result<std::process::Output> {
    timeout(Duration::from_secs(45), command.kill_on_drop(true).output()).await?
}

async fn checked(command: &mut Command) -> Result<(), Failure> {
    let output = run(command).await.map_err(unavailable)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(unavailable(format_args!(
            "speech subprocess exited with {}",
            output.status
        )))
    }
}

fn unavailable(error: impl std::fmt::Display) -> Failure {
    eprintln!("CallFreya: {error}");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "The local speech service is unavailable.",
    )
}
