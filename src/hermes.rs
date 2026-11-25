use crate::{Failure, Speech, require_call_header, run};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{io::Write, path::PathBuf};
use tokio::process::Command;

#[derive(Clone)]
pub struct Hermes {
    port: u16,
    key_file: PathBuf,
    model: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    DemoFollowup,
    AppointmentRequest,
    HumanHandoff,
}

#[derive(Serialize)]
pub struct Draft {
    draft: String,
}

impl Hermes {
    pub fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let Some(port) = std::env::var_os("CALLFREYA_HERMES_PORT") else {
            return Ok(None);
        };
        let port: u16 = port
            .into_string()
            .map_err(|_| "Invalid Hermes port")?
            .parse()?;
        if port == 0 {
            return Err("Invalid Hermes port".into());
        }
        Ok(Some(Self {
            port,
            key_file: std::env::var_os("CALLFREYA_HERMES_KEY_FILE")
                .ok_or("CALLFREYA_HERMES_KEY_FILE is required with a Hermes port")?
                .into(),
            model: std::env::var("CALLFREYA_HERMES_MODEL").unwrap_or_else(|_| "hermes".into()),
        }))
    }
}

pub async fn draft(
    State(speech): State<Speech>,
    headers: HeaderMap,
    Json(scenario): Json<Scenario>,
) -> Result<Json<Draft>, Failure> {
    require_call_header(&headers)?;
    let hermes = speech.hermes.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Hermes is disabled. Configure a local gateway first.",
    ))?;
    let prompt = match scenario {
        Scenario::DemoFollowup => {
            "Draft a short email offering a demo of Harbor Desk, a fictional appointment tool. The fictional caller asked for a follow-up. Ask for their availability."
        }
        Scenario::AppointmentRequest => {
            "Draft a short email acknowledging a fictional request for a 30-minute appointment with Harbor Desk. Make clear the time is awaiting confirmation."
        }
        Scenario::HumanHandoff => {
            "Draft a short note for a fictional human receptionist. A caller asked to speak to a person about Harbor Desk and gave no further details."
        }
    };
    let key = std::fs::read_to_string(&hermes.key_file).map_err(gateway_error)?;
    let key = key.trim();
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(gateway_error("Invalid Hermes API key"));
    }
    let mut header = tempfile::NamedTempFile::new().map_err(gateway_error)?;
    writeln!(header, "Authorization: Bearer {key}").map_err(gateway_error)?;
    let mut payload = tempfile::NamedTempFile::new().map_err(gateway_error)?;
    serde_json::to_writer(&mut payload, &json!({
        "model": hermes.model,
        "stream": false,
        "messages": [
            {"role": "system", "content": "Write draft text for review. Do not send messages, use tools, or make commitments. Use only the fictional details provided."},
            {"role": "user", "content": prompt}
        ]
    })).map_err(gateway_error)?;
    let mut command = Command::new("curl");
    command
        .args([
            "-q",
            "--silent",
            "--fail",
            "--noproxy",
            "*",
            "--max-time",
            "35",
            "--max-filesize",
            "262144",
            "-H",
            "Content-Type: application/json",
            "-H",
        ])
        .arg(format!("@{}", header.path().display()))
        .arg("--data-binary")
        .arg(format!("@{}", payload.path().display()))
        .arg(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            hermes.port
        ));
    let output = run(&mut command).await.map_err(gateway_error)?;
    if !output.status.success() {
        return Err(gateway_error("Hermes request failed"));
    }
    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(gateway_error)?;
    let draft = result
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 8192)
        .ok_or_else(|| gateway_error("Missing or oversized Hermes draft"))?;
    Ok(Json(Draft {
        draft: draft.to_owned(),
    }))
}

fn gateway_error(_: impl std::fmt::Display) -> Failure {
    (
        StatusCode::BAD_GATEWAY,
        "The local Hermes gateway did not return a usable draft.",
    )
}
