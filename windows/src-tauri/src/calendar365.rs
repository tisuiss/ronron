//! The calendars, read only: the Microsoft 365 account's calendars through Graph
//! (sign-in: crate::msgraph), the Google account's (crate::google::calendar) and the
//! iCal ones (crate::ical), from four weeks back
//! to nine weeks ahead, in the shape the Cal.com card and its month view already read
//! (`bookings`, `rangeStart`, `rangeEnd`), with `calendars` (each one's name,
//! colour, whether it is shown and its reminder) and each event's `calendarId`.

use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::integrations::{self, IntegrationUpdate};
use crate::settings::{CalendarPref, IcalCalendar};
use crate::msgraph;

pub const ID: &str = "integration_calendar";
/// Read from four weeks back to nine weeks ahead: the Calendar space moves week by week inside it.
const PAST_DAYS: i64 = 28;
const WINDOW_DAYS: i64 = 63;
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
    let (prefs, icals) = calendar_settings(app);
    let icals: Vec<IcalCalendar> = icals.into_iter().filter(|c| (c.slot as usize) < crate::ical::SLOTS && crate::secrets::present(&crate::ical::key(c.slot as usize))).collect();
    if !msgraph::signed_in() && !crate::google::signed_in() && icals.is_empty() {
        integrations::emit(app, IntegrationUpdate { id: ID, data: json!({}), error: None, event: None });
        return;
    }
    let result = tokio::time::timeout(Duration::from_secs(60), fetch(&prefs, &icals)).await
        .unwrap_or_else(|_| Err(crate::i18n::t("No response text.")));
    let (data, error) = match result {
        Ok(data) => (data, None),
        Err(error) => (json!({}), Some(error)),
    };
    integrations::emit(app, IntegrationUpdate { id: ID, data, error, event: None });
}

/// The calendars changed in Settings (shown, reminders, an iCal address): read them again.
#[tauri::command]
pub async fn calendar_refresh(app: AppHandle) {
    if active(&app) {
        poll_now(&app).await;
    }
}

/// The signed-in accounts' calendars, for Settings: id, name, colour, default, kind.
#[tauri::command]
pub async fn calendar_sources() -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    if msgraph::signed_in() {
        out.extend(microsoft_calendars().await?.into_iter().enumerate().map(|(i, (id, name, color, default))| {
            let color = if color.starts_with('#') && color.len() == 7 { color } else { PALETTE[i % PALETTE.len()].to_string() };
            json!({ "id": id, "name": name, "color": color, "default": default, "kind": "microsoft" })
        }));
    }
    if crate::google::signed_in() {
        out.extend(crate::google::calendar::calendars().await?.into_iter().enumerate().map(|(i, (id, name, color, primary))| {
            let color = if color.starts_with('#') && color.len() == 7 { color } else { PALETTE[(i + 2) % PALETTE.len()].to_string() };
            json!({ "id": google_id(&id), "name": name, "color": color, "default": primary, "kind": "google" })
        }));
    }
    Ok(out)
}

/// A Google calendar's id as the island and the preferences know it: apart from
/// Microsoft's and the iCal ones.
fn google_id(id: &str) -> String {
    format!("google:{id}")
}

/// Checks an iCal address before it is kept: how many events it has in the coming month.
#[tauri::command]
pub async fn ical_check(url: String) -> Result<usize, String> {
    let text = crate::ical::fetch(&url).await?;
    let now = now_ms();
    Ok(crate::ical::read(&text, "check", now, now + 30 * DAY_MS).len())
}

/// "webcal://…" as the https address Ronron keeps, or an error to show.
#[tauri::command]
pub fn ical_normalize(url: String) -> Result<String, String> {
    crate::ical::normalize_url(&url).ok_or_else(|| crate::i18n::t("The iCal address must start with https:// or webcal://."))
}

fn calendar_settings(app: &AppHandle) -> (Vec<CalendarPref>, Vec<IcalCalendar>) {
    use tauri::Manager;
    app.try_state::<crate::Shared>()
        .map(|shared| {
            let s = shared.settings.lock().unwrap();
            (s.calendar_prefs.clone(), s.ical_calendars.clone())
        })
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Colours for calendars that come without one, in turn.
const PALETTE: [&str; 6] = ["#0F78D4", "#E879A9", "#34D399", "#A78BFA", "#F0913C", "#F5A524"];

/// Shown unless the user said otherwise; for Microsoft 365, only the default calendar until then.
fn pref<'a>(prefs: &'a [CalendarPref], id: &str) -> Option<&'a CalendarPref> {
    prefs.iter().find(|p| p.id == id)
}

async fn fetch(prefs: &[CalendarPref], icals: &[IcalCalendar]) -> Result<Value, String> {
    let now = now_ms();
    let (from, until) = (now - PAST_DAYS * DAY_MS, now + WINDOW_DAYS * DAY_MS);
    let mut calendars = Vec::new();
    let mut events = Vec::new();
    let mut errors = Vec::new();
    let mut read_one = false;

    if msgraph::signed_in() {
        match microsoft_calendars().await {
            Ok(list) => {
                for (i, (id, name, color, default)) in list.into_iter().enumerate() {
                    let p = pref(prefs, &id);
                    let shown = p.map(|p| p.shown).unwrap_or(default);
                    let color = if color.starts_with('#') && color.len() == 7 { color } else { PALETTE[i % PALETTE.len()].to_string() };
                    calendars.push(json!({ "id": id, "name": name, "color": color, "kind": "microsoft", "default": default,
                        "shown": shown, "remind": p.map(|p| p.remind).unwrap_or(0) }));
                    if !shown {
                        continue;
                    }
                    match microsoft_events(&id, from, until).await {
                        Ok(mut list) => {
                            for e in &mut list {
                                e["calendarId"] = json!(id);
                            }
                            events.extend(list);
                            read_one = true;
                        }
                        Err(error) => errors.push(json!({ "id": id, "error": error })),
                    }
                }
            }
            Err(error) => errors.push(json!({ "id": "microsoft", "error": error })),
        }
    }

    if crate::google::signed_in() {
        match crate::google::calendar::calendars().await {
            Ok(list) => {
                for (i, (raw, name, color, primary)) in list.into_iter().enumerate() {
                    let id = google_id(&raw);
                    let p = pref(prefs, &id);
                    // Only the account's own calendar until the user picks others.
                    let shown = p.map(|p| p.shown).unwrap_or(primary);
                    let color = if color.starts_with('#') && color.len() == 7 { color } else { PALETTE[(i + 2) % PALETTE.len()].to_string() };
                    calendars.push(json!({ "id": id, "name": name, "color": color, "kind": "google", "default": primary,
                        "shown": shown, "remind": p.map(|p| p.remind).unwrap_or(0) }));
                    if !shown {
                        continue;
                    }
                    match crate::google::calendar::events(&raw, from, until).await {
                        Ok(mut list) => {
                            for e in &mut list {
                                e["calendarId"] = json!(id);
                            }
                            events.extend(list);
                            read_one = true;
                        }
                        Err(error) => errors.push(json!({ "id": id, "error": error })),
                    }
                }
            }
            Err(error) => errors.push(json!({ "id": "google", "error": error })),
        }
    }

    for (i, cal) in icals.iter().enumerate() {
        let id = format!("ical-{}", cal.slot);
        let p = pref(prefs, &id);
        let shown = p.map(|p| p.shown).unwrap_or(true);
        let color = if cal.color.starts_with('#') && cal.color.len() == 7 { cal.color.clone() } else { PALETTE[(i + 1) % PALETTE.len()].to_string() };
        let name = if cal.name.trim().is_empty() { format!("iCal {}", cal.slot + 1) } else { cal.name.clone() };
        calendars.push(json!({ "id": id, "name": name, "color": color, "kind": "ical", "default": false,
            "shown": shown, "remind": p.map(|p| p.remind).unwrap_or(0) }));
        if !shown {
            continue;
        }
        let Some(url) = crate::secrets::get(&crate::ical::key(cal.slot as usize)) else { continue };
        match crate::ical::fetch(&url).await {
            Ok(text) => {
                events.extend(crate::ical::read(&text, &id, from, until));
                read_one = true;
            }
            Err(error) => errors.push(json!({ "id": id, "error": error })),
        }
    }

    if !read_one && !errors.is_empty() && calendars.iter().all(|c| c["shown"] != true || errors.iter().any(|e| e["id"] == c["id"])) {
        return Err(errors[0]["error"].as_str().unwrap_or("").to_string());
    }
    events.sort_by(|a, b| a["start"].as_str().cmp(&b["start"].as_str()));
    Ok(json!({ "calendars": calendars, "bookings": events, "rangeStart": from, "rangeEnd": until, "errors": errors }))
}

/// The account's calendars: id, name, colour, default.
async fn microsoft_calendars() -> Result<Vec<(String, String, String, bool)>, String> {
    let mut url = msgraph::url(&["calendars"])?;
    url.query_pairs_mut().append_pair("$top", "50").append_pair("$select", "id,name,hexColor,isDefaultCalendar");
    let value = msgraph::get(url).await?;
    let items = value.get("value").and_then(Value::as_array).ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    Ok(items.iter().filter_map(|c| Some((
        c.get("id")?.as_str()?.to_string(),
        c.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        c.get("hexColor").and_then(Value::as_str).unwrap_or("").to_string(),
        c.get("isDefaultCalendar").and_then(Value::as_bool).unwrap_or(false),
    ))).collect())
}

async fn microsoft_events(calendar: &str, from: i64, until: i64) -> Result<Vec<Value>, String> {
    let mut next = msgraph::url(&["calendars", calendar, "calendarView"])?;
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
            None => break,
        }
    }
    Ok(events)
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
