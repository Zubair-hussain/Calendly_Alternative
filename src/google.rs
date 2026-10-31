//! Minimal Google Calendar client: OAuth refresh, freeBusy, events.insert.

use crate::slots::Busy;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

const CALENDAR_API: &str = "https://www.googleapis.com/calendar/v3";

pub struct Google {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    pub calendar_id: String,
    token: Mutex<Option<(String, DateTime<Utc>)>>,
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    expires_in: i64,
}

pub struct Booking<'a> {
    pub start: DateTime<Utc>,
    pub minutes: i64,
    pub name: &'a str,
    pub email: &'a str,
    pub notes: &'a str,
    pub request_id: String,
}

impl Google {
    pub fn new(
        client_id: String,
        client_secret: String,
        refresh_token: String,
        calendar_id: String,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()?,
            client_id,
            client_secret,
            refresh_token,
            calendar_id,
            token: Mutex::new(None),
        })
    }

    async fn access_token(&self) -> Result<String, reqwest::Error> {
        let mut cached = self.token.lock().await;
        if let Some((token, expires)) = cached.as_ref() {
            if *expires > Utc::now() + Duration::seconds(60) {
                return Ok(token.clone());
            }
        }
        let fresh: Token = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let token = fresh.access_token;
        *cached = Some((
            token.clone(),
            Utc::now() + Duration::seconds(fresh.expires_in),
        ));
        Ok(token)
    }

    pub async fn busy(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Busy>, reqwest::Error> {
        let token = self.access_token().await?;
        let body: Value = self
            .http
            .post(format!("{CALENDAR_API}/freeBusy"))
            .bearer_auth(token)
            .json(&json!({
                "timeMin": from.to_rfc3339(),
                "timeMax": to.to_rfc3339(),
                "items": [{ "id": self.calendar_id }],
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let periods = body["calendars"][&self.calendar_id]["busy"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(periods
            .iter()
            .filter_map(|p| {
                Some(Busy {
                    start: p["start"].as_str()?.parse().ok()?,
                    end: p["end"].as_str()?.parse().ok()?,
                })
            })
            .collect())
    }

    /// Creates the event, emails the guest, and returns the Meet link.
    pub async fn book(&self, b: Booking<'_>) -> Result<String, reqwest::Error> {
        let token = self.access_token().await?;
        let end = b.start + Duration::minutes(b.minutes);
        let url = format!(
            "{CALENDAR_API}/calendars/{}/events",
            encode(&self.calendar_id)
        );
        let event: Value = self
            .http
            .post(url)
            .bearer_auth(token)
            .query(&[("conferenceDataVersion", "1"), ("sendUpdates", "all")])
            .json(&json!({
                "summary": format!("Meeting with {}", b.name),
                "description": b.notes,
                "start": { "dateTime": b.start.to_rfc3339() },
                "end": { "dateTime": end.to_rfc3339() },
                "attendees": [{ "email": b.email, "displayName": b.name }],
                "conferenceData": { "createRequest": {
                    "requestId": b.request_id,
                    "conferenceSolutionKey": { "type": "hangoutsMeet" },
                }},
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(event["hangoutLink"].as_str().unwrap_or_default().to_owned())
    }
}

fn encode(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('@', "%40")
        .replace('#', "%23")
}
