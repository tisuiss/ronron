//! The Outlook calendar through Microsoft Graph (sign-in: crate::msgraph), read
//! only: the events from yesterday to 30 days ahead, in the shape the Cal.com card
//! and its month view already read (`bookings`, `rangeStart`, `rangeEnd`).

use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::integrations::{self, IntegrationUpdate};
use crate::msgraph;

pub const ID: &str = "integration_calendar";
const WINDOW_DAYS: i64 = 30;
const PAGE: usize = 100;
const MAX_PAGES: usize = 5;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn active(app: &AppHandle) -> bool {
    !integrations::PAUSED.load(std::sync::atomic::Ordering::Relaxed) && integrations::enabled(app, ID)
}

pub async fn poll(app: AppHandle) {
    if active(&app) {
        poll_now(&app).await;
    }
}

pub async fn poll_now(app: &AppHandle) {
    if !msgraph::signed_in() {
        integrations::emit(app, IntegrationUpdate { id: ID, data: json!({}), error: None, event: None });
        return;
    }
    let result = tokio::time::timeout(Duration::from_secs(45), fetch()).await
        .unwrap_or_else(|_| Err(crate::i18n::t("No response text.")));
    let (data, error) = match result {
        Ok(data) => (data, None),
        Err(error) => (json!({}), Some(error)),
    };
    integrations::emit(app, IntegrationUpdate { id: ID, data, error, event: None });
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

async fn fetch() -> Result<Value, String> {
    let now = now_ms();
    let (from, until) = (now - DAY_MS, now + WINDOW_DAYS * DAY_MS);
    let mut next = msgraph::url(&["calendarView"])?;
    next.query_pairs_mut()
        .append_pair("startDateTime", &crate::whatsapp::gateway::iso_utc(from))
        .append_pair("endDateTime", &crate::whatsapp::gateway::iso_utc(until))
        .append_pair("$top", &PAGE.to_string())
        .append_pair("$orderby", "start/dateTime")
        .append_pair("$select", "id,subject,start,end,isAllDay,isCancelled,location,onlineMeeting,organizer,webLink,showAs");
    let mut events = Vec::new();
    for _ in 0..MAX_PAGES {
        let url = next.clone();
        // Every time in UTC, so the island's own clock decides what "today" is.
        let value = msgraph::call(|h, t| h.get(url).bearer_auth(t).header("Prefer", "outlook.timezone=\"UTC\"")).await?;
        events.extend(parse(&value)?);
        match msgraph::next_link(&value) {
            Some(link) => next = link,
            None => return Ok(json!({ "bookings": events, "rangeStart": from, "rangeEnd": until })),
        }
    }
    Ok(json!({ "bookings": events, "rangeStart": from, "rangeEnd": until }))
}

/// Graph's "2026-11-20T09:30:00.0000000" in UTC → "2026-11-20T09:30:00Z".
fn utc(value: &Value) -> Option<String> {
    let text = value.get("dateTime")?.as_str()?;
    let base = text.split('.').next()?;
    (base.len() == 19 && base.as_bytes()[10] == b'T').then(|| format!("{base}Z"))
}

fn parse(value: &Value) -> Result<Vec<Value>, String> {
    let items = value.get("value").and_then(Value::as_array).ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    Ok(items.iter().filter_map(|e| {
        if e.get("isCancelled").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let id = e.get("id")?.as_str()?;
        let (start, end) = (utc(e.get("start")?)?, utc(e.get("end")?)?);
        let location = e.pointer("/location/displayName").and_then(Value::as_str).unwrap_or("");
        let organizer = e.pointer("/organizer/emailAddress/name").and_then(Value::as_str).unwrap_or("");
        let organizer_email = e.pointer("/organizer/emailAddress/address").and_then(Value::as_str).unwrap_or("");
        let join = e.pointer("/onlineMeeting/joinUrl").and_then(Value::as_str).filter(|u| u.starts_with("https://")).unwrap_or("");
        let web = e.get("webLink").and_then(Value::as_str).filter(|u| u.starts_with("https://")).unwrap_or("");
        Some(json!({
            "id": id,
            "title": e.get("subject").and_then(Value::as_str).unwrap_or(""),
            "start": start,
            "end": end,
            "allDay": e.get("isAllDay").and_then(Value::as_bool).unwrap_or(false),
            "attendeeName": organizer,
            "attendeeEmail": organizer_email,
            "attendeeNotes": location,
            "joinUrl": join,
            "webUrl": web,
        }))
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_read_for_the_calendar_card() {
        let events = parse(&json!({ "value": [
            { "id": "e1", "subject": "Point client", "isAllDay": false, "isCancelled": false,
              "start": { "dateTime": "2026-11-20T09:30:00.0000000", "timeZone": "UTC" },
              "end": { "dateTime": "2026-11-20T10:00:00.0000000", "timeZone": "UTC" },
              "location": { "displayName": "Teams" },
              "organizer": { "emailAddress": { "name": "Anne", "address": "anne@contoso.ch" } },
              "onlineMeeting": { "joinUrl": "https://teams.microsoft.com/l/meetup-join/x" },
              "webLink": "https://outlook.office365.com/owa/?itemid=x" },
            { "id": "e2", "subject": "Annulé", "isCancelled": true,
              "start": { "dateTime": "2026-11-20T11:00:00" }, "end": { "dateTime": "2026-11-20T12:00:00" } },
            { "id": "e3", "subject": "Bad", "start": { "dateTime": "nope" }, "end": { "dateTime": "nope" } },
            { "id": "e4", "subject": "Lien douteux", "onlineMeeting": { "joinUrl": "javascript:alert(1)" },
              "start": { "dateTime": "2026-11-21T00:00:00.0000000" }, "end": { "dateTime": "2026-11-22T00:00:00.0000000" }, "isAllDay": true },
        ] })).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["start"], "2026-11-20T09:30:00Z");
        assert_eq!(events[0]["attendeeName"], "Anne");
        assert_eq!(events[0]["attendeeNotes"], "Teams");
        assert_eq!(events[0]["joinUrl"], "https://teams.microsoft.com/l/meetup-join/x");
        assert_eq!(events[1]["joinUrl"], "");
        assert_eq!(events[1]["allDay"], true);
        assert!(parse(&json!({})).is_err());
    }
}
