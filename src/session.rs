use crate::{
    Failure, GREETING, Speech, require_call_header, respond, speak, transcribe, unavailable,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const RETENTION: Duration = Duration::from_secs(30 * 60);
const CAPACITY: usize = 64;
const MAX_REPLIES: usize = 6;

#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<HashMap<String, Entry>>>);

struct Entry {
    started: Instant,
    session: Arc<tokio::sync::Mutex<Session>>,
}

struct Session {
    confirming: bool,
    finished: bool,
    transcript: Vec<Turn>,
}

#[derive(Clone, Serialize)]
struct Turn {
    speaker: &'static str,
    text: String,
}

#[derive(Serialize)]
pub struct Reply {
    id: String,
    transcript: Vec<Turn>,
    outcome: &'static str,
    audio: String,
}

pub async fn start(
    State(speech): State<Speech>,
    headers: HeaderMap,
) -> Result<Json<Reply>, Failure> {
    require_call_header(&headers)?;
    let mut random = [0; 24];
    getrandom::fill(&mut random).map_err(unavailable)?;
    let id = URL_SAFE_NO_PAD.encode(random);
    let work = tempfile::tempdir().map_err(unavailable)?;
    let audio = speak(&speech, work.path(), GREETING).await?;
    let transcript = vec![Turn {
        speaker: "agent",
        text: GREETING.to_owned(),
    }];
    let mut entries = speech.sessions.0.lock().map_err(unavailable)?;
    entries.retain(|_, entry| entry.started.elapsed() < RETENTION);
    if entries.len() >= CAPACITY {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Session capacity reached. Try again later.",
        ));
    }
    entries.insert(
        id.clone(),
        Entry {
            started: Instant::now(),
            session: Arc::new(tokio::sync::Mutex::new(Session {
                confirming: false,
                finished: false,
                transcript: transcript.clone(),
            })),
        },
    );
    Ok(Json(Reply {
        id,
        transcript,
        outcome: "in_progress",
        audio,
    }))
}

pub async fn turn(
    State(speech): State<Speech>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Reply>, Failure> {
    require_call_header(&headers)?;
    let session = {
        let mut entries = speech.sessions.0.lock().map_err(unavailable)?;
        entries.retain(|_, entry| entry.started.elapsed() < RETENTION);
        entries.get(&id).map(|entry| entry.session.clone())
    }
    .ok_or((
        StatusCode::NOT_FOUND,
        "Call session expired or was not found.",
    ))?;
    let mut session = session.try_lock().map_err(|_| {
        (
            StatusCode::CONFLICT,
            "The previous reply is still being processed.",
        )
    })?;
    if session.finished {
        return Err((StatusCode::CONFLICT, "This call has already ended."));
    }
    let heard = transcribe(&speech, body).await?;
    let (intent, closing_reply) = respond(&heard);
    let (outcome, response) = match intent {
        "declined" => (intent, closing_reply),
        "interested" if session.confirming => (intent, closing_reply),
        _ if session.transcript.len() / 2 + 1 >= MAX_REPLIES => (
            "needs_review",
            "Thanks for your time. I'll leave your response for review. Goodbye.",
        ),
        "interested" => (
            "in_progress",
            "Would you like someone to follow up about a demo?",
        ),
        _ if session.confirming => (
            "in_progress",
            "Please say yes or no to a follow-up about the demo.",
        ),
        _ => (
            "in_progress",
            "I can take a note about a demo. Would you like one?",
        ),
    };
    let work = tempfile::tempdir().map_err(unavailable)?;
    let audio = speak(&speech, work.path(), response).await?;
    session.confirming |= intent == "interested";
    session.finished = outcome != "in_progress";
    session.transcript.extend([
        Turn {
            speaker: "caller",
            text: heard,
        },
        Turn {
            speaker: "agent",
            text: response.to_owned(),
        },
    ]);
    Ok(Json(Reply {
        id,
        transcript: session.transcript.clone(),
        outcome,
        audio,
    }))
}
