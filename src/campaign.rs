use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Deserialize)]
pub struct Campaign {
    script: String,
    calling_window_open: bool,
    max_retries: u8,
    contacts: Vec<Contact>,
}

#[derive(Deserialize)]
struct Contact {
    phone: String,
    consent: bool,
    #[serde(default)]
    opted_out: bool,
    #[serde(default)]
    attempts: u16,
}

#[derive(Serialize)]
pub struct Preview {
    ready: usize,
    contacts: Vec<ContactPreview>,
}

#[derive(Serialize)]
struct ContactPreview {
    phone: String,
    decision: Decision,
}

#[derive(Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Ready,
    InvalidNumber,
    OptedOut,
    NoConsent,
    Duplicate,
    OutsideWindow,
    RetryLimit,
}

pub async fn preview(
    Json(campaign): Json<Campaign>,
) -> Result<Json<Preview>, (StatusCode, &'static str)> {
    if campaign.script.trim().is_empty() || campaign.contacts.len() > 1_000 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "Provide a script and at most 1,000 contacts.",
        ));
    }
    let opted_out: HashSet<_> = campaign
        .contacts
        .iter()
        .filter(|contact| contact.opted_out)
        .map(|contact| contact.phone.trim())
        .collect();
    let mut seen = HashSet::new();
    let contacts: Vec<_> = campaign
        .contacts
        .iter()
        .map(|contact| {
            let phone = contact.phone.trim();
            let decision = if !valid_phone(phone) {
                Decision::InvalidNumber
            } else if opted_out.contains(phone) {
                Decision::OptedOut
            } else if !contact.consent {
                Decision::NoConsent
            } else if !seen.insert(phone) {
                Decision::Duplicate
            } else if !campaign.calling_window_open {
                Decision::OutsideWindow
            } else if contact.attempts > u16::from(campaign.max_retries) {
                Decision::RetryLimit
            } else {
                Decision::Ready
            };
            ContactPreview {
                phone: phone.to_owned(),
                decision,
            }
        })
        .collect();
    let ready = contacts
        .iter()
        .filter(|row| row.decision == Decision::Ready)
        .count();
    Ok(Json(Preview { ready, contacts }))
}

fn valid_phone(phone: &str) -> bool {
    let Some(digits) = phone.strip_prefix('+') else {
        return false;
    };
    (2..=15).contains(&digits.len())
        && !digits.starts_with('0')
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}
