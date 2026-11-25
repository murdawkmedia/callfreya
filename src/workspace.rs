use crate::{Failure, Speech, require_call_header, unavailable};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const APPOINTMENT_SECONDS: u64 = 30 * 60;

#[derive(Clone)]
pub struct Workspace(Arc<Store>);

struct Store {
    path: PathBuf,
    records: Mutex<Records>,
    _lock: File,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Records {
    bookings: Vec<Booking>,
    handoffs: Vec<Handoff>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Booking {
    id: usize,
    name: String,
    email: String,
    slot: u64,
    status: BookingStatus,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum BookingStatus {
    Requested,
    Confirmed,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Handoff {
    id: usize,
    name: String,
    email: String,
    reason: String,
    status: HandoffStatus,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum HandoffStatus {
    Open,
    Resolved,
}

#[derive(Deserialize)]
pub struct BookingRequest {
    name: String,
    email: String,
    slot: u64,
}

#[derive(Deserialize)]
pub struct HandoffRequest {
    name: String,
    email: String,
    reason: String,
}

impl Workspace {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let parent = parent(&path);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path.with_added_extension("lock"))?;
        lock.try_lock()?;
        let records = match File::open(&path) {
            Ok(file) => serde_json::from_reader(file).map_err(io::Error::other)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Records::default(),
            Err(error) => return Err(error),
        };
        Ok(Self(Arc::new(Store {
            path,
            records: Mutex::new(records),
            _lock: lock,
        })))
    }

    fn update<T>(
        &self,
        apply: impl FnOnce(&mut Records) -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let mut records = self.0.records.lock().map_err(unavailable)?;
        let mut revised = records.clone();
        let result = apply(&mut revised)?;
        let mut temp =
            tempfile::NamedTempFile::new_in(parent(&self.0.path)).map_err(unavailable)?;
        serde_json::to_writer(&mut temp, &revised).map_err(unavailable)?;
        temp.as_file().sync_all().map_err(unavailable)?;
        temp.persist(&self.0.path).map_err(unavailable)?;
        *records = revised;
        Ok(result)
    }
}

fn parent(path: &FsPath) -> &FsPath {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(FsPath::new("."))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn text(value: String, limit: usize) -> Result<String, Failure> {
    let value = value.trim();
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "Required field is empty or invalid.",
        ));
    }
    Ok(value.to_owned())
}

fn email(value: String) -> Result<String, Failure> {
    let value = text(value, 254)?;
    let valid = value.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && domain.contains('.') && !domain.contains('@')
    });
    if !valid || value.chars().any(char::is_whitespace) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "Enter a contact email."));
    }
    Ok(value)
}

pub async fn list(
    State(speech): State<Speech>,
    headers: HeaderMap,
) -> Result<Json<Records>, Failure> {
    require_call_header(&headers)?;
    Ok(Json(
        speech
            .workspace
            .0
            .records
            .lock()
            .map_err(unavailable)?
            .clone(),
    ))
}

pub async fn book(
    State(speech): State<Speech>,
    headers: HeaderMap,
    Json(request): Json<BookingRequest>,
) -> Result<Json<Booking>, Failure> {
    require_call_header(&headers)?;
    let name = text(request.name, 120)?;
    let email = email(request.email)?;
    let current_time = now();
    if request.slot <= current_time || request.slot > current_time + 366 * 86_400 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "Choose a future appointment within one year.",
        ));
    }
    speech.workspace.update(|records| {
        let booking = Booking {
            id: records.bookings.len() + 1,
            name,
            email,
            slot: request.slot,
            status: BookingStatus::Requested,
        };
        records.bookings.push(booking.clone());
        Ok(Json(booking))
    })
}

pub async fn confirm(
    State(speech): State<Speech>,
    Path(id): Path<usize>,
    headers: HeaderMap,
) -> Result<Json<Booking>, Failure> {
    require_call_header(&headers)?;
    speech.workspace.update(|records| {
        let index = records
            .bookings
            .iter()
            .position(|booking| booking.id == id)
            .ok_or((StatusCode::NOT_FOUND, "Booking request not found."))?;
        let booking = &records.bookings[index];
        if booking.status == BookingStatus::Confirmed {
            return Ok(Json(booking.clone()));
        }
        if booking.slot <= now()
            || records.bookings.iter().any(|other| {
                other.status == BookingStatus::Confirmed
                    && booking.slot.abs_diff(other.slot) < APPOINTMENT_SECONDS
            })
        {
            return Err((
                StatusCode::CONFLICT,
                "The requested time is past or overlaps a confirmed appointment.",
            ));
        }
        let booking = &mut records.bookings[index];
        booking.status = BookingStatus::Confirmed;
        Ok(Json(booking.clone()))
    })
}

pub async fn handoff(
    State(speech): State<Speech>,
    headers: HeaderMap,
    Json(request): Json<HandoffRequest>,
) -> Result<Json<Handoff>, Failure> {
    require_call_header(&headers)?;
    let name = text(request.name, 120)?;
    let email = email(request.email)?;
    let reason = text(request.reason, 500)?;
    speech.workspace.update(|records| {
        let handoff = Handoff {
            id: records.handoffs.len() + 1,
            name,
            email,
            reason,
            status: HandoffStatus::Open,
        };
        records.handoffs.push(handoff.clone());
        Ok(Json(handoff))
    })
}

pub async fn resolve(
    State(speech): State<Speech>,
    Path(id): Path<usize>,
    headers: HeaderMap,
) -> Result<Json<Handoff>, Failure> {
    require_call_header(&headers)?;
    speech.workspace.update(|records| {
        let handoff = records
            .handoffs
            .iter_mut()
            .find(|handoff| handoff.id == id)
            .ok_or((StatusCode::NOT_FOUND, "Handoff request not found."))?;
        handoff.status = HandoffStatus::Resolved;
        Ok(Json(handoff.clone()))
    })
}

pub async fn export(
    State(speech): State<Speech>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, Failure> {
    require_call_header(&headers)?;
    let records = speech.workspace.0.records.lock().map_err(unavailable)?;
    let mut csv = String::from("type,id,name,email,status,appointment_start_unix_utc,reason\r\n");
    for booking in &records.bookings {
        let status = match booking.status {
            BookingStatus::Requested => "requested",
            BookingStatus::Confirmed => "confirmed",
        };
        csv.push_str(&format!(
            "booking,{},{},{},{},{},\r\n",
            booking.id,
            cell(&booking.name),
            cell(&booking.email),
            status,
            booking.slot
        ));
    }
    for handoff in &records.handoffs {
        let status = match handoff.status {
            HandoffStatus::Open => "open",
            HandoffStatus::Resolved => "resolved",
        };
        csv.push_str(&format!(
            "handoff,{},{},{},{},,{}\r\n",
            handoff.id,
            cell(&handoff.name),
            cell(&handoff.email),
            status,
            cell(&handoff.reason)
        ));
    }
    Ok((
        [
            ("content-type", "text/csv; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        csv,
    ))
}

fn cell(value: &str) -> String {
    let prefix = if value.starts_with(['=', '+', '-', '@']) {
        "'"
    } else {
        ""
    };
    format!("\"{prefix}{}\"", value.replace('"', "\"\""))
}
