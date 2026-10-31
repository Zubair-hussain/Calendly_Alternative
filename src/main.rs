mod circuit;
mod google;
mod slots;

use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use circuit::Circuit;
use google::{Booking, Google};
use serde::Deserialize;
use serde_json::json;
use slots::Rules;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

const MAX_BOOKINGS_PER_HOUR: usize = 5;

struct App {
    google: Google,
    rules: Rules,
    /// Serialises check-then-insert so two guests cannot take one slot.
    /// Requires a single instance: deploy with --max-instances=1.
    booking: Mutex<()>,
    attempts: Mutex<HashMap<String, Vec<Instant>>>,
    circuit: Circuit,
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("missing env var {key}"))
}
fn env_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let tz = env_or("HOST_TIMEZONE", chrono_tz::Asia::Karachi);
    let origin: HeaderValue = env("ALLOWED_ORIGIN").parse().expect("bad ALLOWED_ORIGIN");
    let app = Arc::new(App {
        google: Google::new(
            env("GOOGLE_CLIENT_ID"),
            env("GOOGLE_CLIENT_SECRET"),
            env("GOOGLE_REFRESH_TOKEN"),
            env_or("GOOGLE_CALENDAR_ID", "primary".to_owned()),
        )
        .expect("http client"),
        rules: Rules {
            tz,
            start_hour: env_or("WORK_START_HOUR", 9),
            end_hour: env_or("WORK_END_HOUR", 17),
            duration_min: env_or("MEETING_MINUTES", 30),
            min_notice_hours: env_or("MIN_NOTICE_HOURS", 2),
            max_days_ahead: env_or("MAX_DAYS_AHEAD", 60),
        },
        booking: Mutex::new(()),
        attempts: Mutex::new(HashMap::new()),
        circuit: Circuit::default(),
    });

    let router = Router::new()
        .route("/healthz", get(health))
        .route("/", get(list_slots).post(create_booking))
        .layer(DefaultBodyLimit::max(8 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([header::CONTENT_TYPE]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(app);

    let port: u16 = env_or("PORT", 8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("bind");
    tracing::info!(port, "listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server");
}

async fn health(State(app): State<Arc<App>>) -> Json<serde_json::Value> {
    let google = if app.circuit.is_open() { "down" } else { "ok" };
    Json(json!({ "status": "ok", "google": google }))
}

/// Records the outcome of a Google call on the circuit breaker.
fn track<T>(app: &App, result: Result<T, reqwest::Error>) -> Result<T, reqwest::Error> {
    match &result {
        Ok(_) => app.circuit.success(),
        Err(_) => app.circuit.failure(),
    }
    result
}

const BUSY_MESSAGE: &str = "Booking is temporarily unavailable, please try again shortly";

#[derive(Deserialize)]
struct SlotQuery {
    date: NaiveDate,
}

async fn list_slots(State(app): State<Arc<App>>, Query(q): Query<SlotQuery>) -> Response {
    let Some((open, close)) = slots::day_bounds(&app.rules, q.date) else {
        return Json(json!({ "slots": [] })).into_response();
    };
    if !app.circuit.allow() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, BUSY_MESSAGE);
    }
    match track(&app, app.google.busy(open, close).await) {
        Ok(busy) => {
            let free = slots::free_slots(&app.rules, q.date, &busy, Utc::now());
            Json(json!({ "slots": free })).into_response()
        }
        Err(err) => {
            tracing::error!(status = ?err.status(), "freeBusy failed");
            fail(StatusCode::BAD_GATEWAY, "Could not load times")
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookingRequest {
    start: DateTime<Utc>,
    name: String,
    email: String,
    #[serde(default)]
    notes: String,
    // Sent by the widget for display; the server's own values always win.
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    tz: Option<String>,
    #[serde(default)]
    place: Option<String>,
}

fn fail(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "ok": false, "error": message }))).into_response()
}

fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    email.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

async fn over_limit(app: &App, headers: &HeaderMap) -> bool {
    // Cloud Run appends the real client address as the last entry.
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_owned();
    let mut attempts = app.attempts.lock().await;
    attempts.retain(|_, hits| {
        hits.retain(|t| t.elapsed().as_secs() < 3600);
        !hits.is_empty()
    });
    let hits = attempts.entry(ip).or_default();
    hits.push(Instant::now());
    hits.len() > MAX_BOOKINGS_PER_HOUR
}

async fn create_booking(State(app): State<Arc<App>>, headers: HeaderMap, body: String) -> Response {
    // The widget posts JSON as text/plain, so parse the body ourselves.
    let Ok(req) = serde_json::from_str::<BookingRequest>(&body) else {
        return fail(StatusCode::BAD_REQUEST, "Invalid request");
    };
    let _ = (&req.duration, &req.tz, &req.place);
    let name = req.name.trim();
    let email = req.email.trim();
    if name.is_empty()
        || name.chars().count() > 100
        || req.notes.chars().count() > 2000
        || !valid_email(email)
    {
        return fail(StatusCode::BAD_REQUEST, "Please check your details");
    }
    if over_limit(&app, &headers).await {
        return fail(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts, try later",
        );
    }

    if !app.circuit.allow() {
        return fail(StatusCode::SERVICE_UNAVAILABLE, BUSY_MESSAGE);
    }
    let _guard = app.booking.lock().await;
    let minutes = app.rules.duration_min;
    let end = req.start + Duration::minutes(minutes);
    let busy = match track(&app, app.google.busy(req.start, end).await) {
        Ok(busy) => busy,
        Err(err) => {
            tracing::error!(status = ?err.status(), "freeBusy failed");
            return fail(StatusCode::BAD_GATEWAY, "Please try again");
        }
    };
    if !slots::is_offered(&app.rules, req.start, &busy, Utc::now()) {
        return fail(StatusCode::CONFLICT, "That time is no longer available");
    }

    let booked = app
        .google
        .book(Booking {
            start: req.start,
            minutes,
            name,
            email,
            notes: &req.notes,
            request_id: format!(
                "{}-{}",
                req.start.timestamp(),
                Utc::now().timestamp_millis()
            ),
        })
        .await;
    match track(&app, booked) {
        Ok(link) => Json(json!({ "ok": true, "meetLink": link })).into_response(),
        Err(err) => {
            tracing::error!(status = ?err.status(), "event insert failed");
            fail(StatusCode::BAD_GATEWAY, "Please try again")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_email;

    #[test]
    fn email_validation() {
        assert!(valid_email("a.b+c@example.co"));
        for bad in [
            "",
            "a",
            "a@b",
            "@b.co",
            "a b@c.co",
            "a@b@c.co",
            "a@.co",
            "a@b.",
            "a@b.co\nBcc:x",
        ] {
            assert!(!valid_email(bad), "{bad}");
        }
    }
}
