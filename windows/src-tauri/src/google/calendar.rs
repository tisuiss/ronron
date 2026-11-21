//! Google Calendar, read only: the account's calendars and their events, in the
//! shape calendar365.rs hands the Calendar tab.

use serde_json::{json, Value};

use super::{get, url, CALENDAR};

const MAX_PAGES: usize = 4;

fn invalid() -> String {
    crate::i18n::t("Unexpected API response.")
}

/// The calendars the account shows: id, name, colour, primary.
pub async fn calendars() -> Result<Vec<(String, String, String, bool)>, String> {
    let mut u = url(CALENDAR, &["users", "me", "calendarList"])?;
    u.query_pairs_mut().append_pair("maxResults", "50").append_pair("minAccessRole", "reader");
    parse_calendars(&get(u).await?)
}

fn parse_calendars(value: &Value) -> Result<Vec<(String, String, String, bool)>, String> {
    let items = value.get("items").and_then(Value::as_array).ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|c| {
        if c.get("deleted").and_then(Value::as_bool) == Some(true) || c.get("hidden").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let name = c.get("summaryOverride").or_else(|| c.get("summary")).and_then(Value::as_str).unwrap_or("");
        Some((
            c.get("id")?.as_str()?.to_string(),
            name.to_string(),
            c.get("backgroundColor").and_then(Value::as_str).unwrap_or("").to_string(),
            c.get("primary").and_then(Value::as_bool).unwrap_or(false),
        ))
    }).collect())
}

/// The events of `calendar` between two instants (ms), repeats expanded.
pub async fn events(calendar: &str, from: i64, until: i64) -> Result<Vec<Value>, String> {
    let iso = crate::whatsapp::gateway::iso_utc;
    let mut events = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut u = url(CALENDAR, &["calendars", calendar, "events"])?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("timeMin", &iso(from))
                .append_pair("timeMax", &iso(until))
                .append_pair("singleEvents", "true")
                .append_pair("orderBy", "startTime")
                .append_pair("maxResults", "250")
                // Every time in UTC, so the island's own clock decides what "today" is.
                .append_pair("timeZone", "UTC");
            if let Some(token) = &page {
                q.append_pair("pageToken", token);
            }
        }
        let value = get(u).await?;
        events.extend(parse_events(&value)?);
        match value.get("nextPageToken").and_then(Value::as_str) {
            Some(token) => page = Some(token.to_string()),
            None => break,
        }
    }
    Ok(events)
}

/// "2026-11-20T09:30:00Z", "2026-11-20T10:30:00+01:00" or the all-day "2026-11-20" → UTC "…Z".
pub fn utc(when: &Value) -> Option<(String, bool)> {
    if let Some(day) = when.get("date").and_then(Value::as_str) {
        let ok = day.len() == 10 && crate::mail::graph::parse_utc(&format!("{day}T00:00:00Z")).is_some();
        return ok.then(|| (format!("{day}T00:00:00Z"), true));
    }
    let text = when.get("dateTime")?.as_str()?;
    let base = text.get(..19)?;
    let mut ms = crate::mail::graph::parse_utc(&format!("{base}Z"))?;
    let rest = text[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
        let h: i64 = rest.get(1..3)?.parse().ok()?;
        let m: i64 = rest.get(4..6)?.parse().ok()?;
        let offset = (h * 60 + m) * 60_000;
        ms += if sign == '+' { -offset } else { offset };
    }
    Some((crate::whatsapp::gateway::iso_utc(ms), false))
}

fn https(v: Option<&Value>) -> &str {
    v.and_then(Value::as_str).filter(|u| u.starts_with("https://")).unwrap_or("")
}

pub fn parse_events(value: &Value) -> Result<Vec<Value>, String> {
    let items = value.get("items").and_then(Value::as_array).ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|e| {
        if e.get("status").and_then(Value::as_str) == Some("cancelled") {
            return None;
        }
        let (start, all_day) = utc(e.get("start")?)?;
        let (end, _) = utc(e.get("end")?)?;
        let video = e.pointer("/conferenceData/entryPoints").and_then(Value::as_array)
            .and_then(|points| points.iter().find(|p| p.get("entryPointType").and_then(Value::as_str) == Some("video")))
            .and_then(|p| p.get("uri"));
        let join = match https(e.get("hangoutLink")) {
            "" => https(video),
            link => link,
        };
        Some(json!({
            "id": e.get("id")?.as_str()?,
            "title": e.get("summary").and_then(Value::as_str).unwrap_or(""),
            "start": start,
            "end": end,
            "allDay": all_day,
            "attendeeName": e.pointer("/organizer/displayName").and_then(Value::as_str).unwrap_or(""),
            "attendeeEmail": e.pointer("/organizer/email").and_then(Value::as_str).unwrap_or(""),
            "attendeeNotes": e.get("location").and_then(Value::as_str).unwrap_or(""),
            "joinUrl": join,
            "webUrl": https(e.get("htmlLink")),
        }))
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_become_utc() {
        assert_eq!(utc(&json!({ "dateTime": "2026-11-20T09:30:00Z" })), Some(("2026-11-20T09:30:00Z".into(), false)));
        assert_eq!(utc(&json!({ "dateTime": "2026-11-20T10:30:00+01:00" })), Some(("2026-11-20T09:30:00Z".into(), false)));
        assert_eq!(utc(&json!({ "dateTime": "2026-11-20T04:30:00.000-05:00" })), Some(("2026-11-20T09:30:00Z".into(), false)));
        assert_eq!(utc(&json!({ "date": "2026-11-21" })), Some(("2026-11-21T00:00:00Z".into(), true)));
        assert_eq!(utc(&json!({ "date": "nope" })), None);
        assert_eq!(utc(&json!({})), None);
    }

    #[test]
    fn events_are_read_for_the_calendar_card() {
        let events = parse_events(&json!({ "items": [
            { "id": "e1", "summary": "Point client", "status": "confirmed",
              "start": { "dateTime": "2026-11-20T09:30:00Z" }, "end": { "dateTime": "2026-11-20T10:00:00Z" },
              "location": "Salle 2", "organizer": { "displayName": "Anne", "email": "anne@gmail.com" },
              "hangoutLink": "https://meet.google.com/abc-defg-hij", "htmlLink": "https://www.google.com/calendar/event?eid=x" },
            { "id": "e2", "status": "cancelled", "start": { "date": "2026-11-20" }, "end": { "date": "2026-11-21" } },
            { "id": "e3", "summary": "Congé", "start": { "date": "2026-11-21" }, "end": { "date": "2026-11-22" },
              "conferenceData": { "entryPoints": [{ "entryPointType": "phone", "uri": "tel:+41" }, { "entryPointType": "video", "uri": "https://zoom.us/j/1" }] } },
            { "id": "e4", "summary": "Lien douteux", "hangoutLink": "javascript:alert(1)",
              "start": { "dateTime": "2026-11-22T08:00:00Z" }, "end": { "dateTime": "2026-11-22T09:00:00Z" } },
        ] })).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["joinUrl"], "https://meet.google.com/abc-defg-hij");
        assert_eq!(events[0]["attendeeName"], "Anne");
        assert_eq!(events[0]["attendeeNotes"], "Salle 2");
        assert_eq!(events[1]["allDay"], true);
        assert_eq!(events[1]["joinUrl"], "https://zoom.us/j/1");
        assert_eq!(events[2]["joinUrl"], "");
        assert!(parse_events(&json!({})).is_err());
    }

    #[test]
    fn hidden_and_deleted_calendars_stay_out() {
        let list = parse_calendars(&json!({ "items": [
            { "id": "anne@gmail.com", "summary": "Anne", "backgroundColor": "#9fc6e7", "primary": true },
            { "id": "fr.ch#holiday@group.v.calendar.google.com", "summary": "Fêtes", "summaryOverride": "Jours fériés" },
            { "id": "x", "summary": "Caché", "hidden": true },
            { "id": "y", "deleted": true },
        ] })).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], ("anne@gmail.com".into(), "Anne".into(), "#9fc6e7".into(), true));
        assert_eq!(list[1].1, "Jours fériés");
    }
}
