//! Aircall: the team's calls of the last two days, read with the company's API
//! ID and token (Aircall → Integrations & API → API keys), kept in the
//! credential store. Read-only: the API is only asked for calls. A new missed
//! call or voicemail lights the pill; calling back opens the number with the
//! phone app the system has for `tel:` links (the Aircall app when it is the
//! one set) — never without the user's click.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::integrations::{self, IntegrationEvent, IntegrationUpdate};

const ID: &str = "integration_aircall";
pub const KEY_ID: &str = "aircall-api-id";
pub const KEY_TOKEN: &str = "aircall-api-token";
const CALLS_URL: &str = "https://api.aircall.io/v1/calls";
/// Two days back: today's calls whatever the time zone, and yesterday's.
const WINDOW_SECS: i64 = 48 * 3600;
const PER_PAGE: usize = 50;
const MAX_PAGES: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BODY: usize = 4 * 1024 * 1024;

/// The missed calls and voicemails already seen; None until the first poll,
/// which only takes note (no alert for what was there before Ronron).
static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn active(app: &AppHandle) -> bool {
    !integrations::PAUSED.load(std::sync::atomic::Ordering::Relaxed) && integrations::enabled(app, ID)
}

pub async fn poll(app: AppHandle) {
    if !active(&app) {
        return;
    }
    let (Some(id), Some(token)) = (crate::secrets::get(KEY_ID), crate::secrets::get(KEY_TOKEN)) else { return };
    let result = fetch(id.trim(), token.trim()).await;
    if !active(&app) {
        return;
    }
    match result {
        Ok(calls) => {
            let event = news(&calls);
            integrations::emit(&app, IntegrationUpdate { id: ID, data: json!({ "calls": calls }), error: None, event });
        }
        Err(error) => integrations::emit(&app, IntegrationUpdate { id: ID, data: json!({}), error: Some(error), event: None }),
    }
}

/// The newest missed call or voicemail not seen before, as the pill's alert.
fn news(calls: &[Value]) -> Option<IntegrationEvent> {
    let mut seen = SEEN.lock().unwrap();
    let first = seen.is_none();
    let seen = seen.get_or_insert_with(HashSet::new);
    let mut newest: Option<&Value> = None;
    for call in calls.iter().filter(|c| c["missed"] == true || c["voicemail"] == true) {
        let key = format!("{}:{}", call["id"].as_str().unwrap_or(""), call["voicemail"] == true);
        if seen.insert(key) && !first && newest.is_none() {
            newest = Some(call);
        }
    }
    let call = newest?;
    let who = call["name"].as_str().filter(|n| !n.is_empty()).or_else(|| call["number"].as_str()).unwrap_or("").to_string();
    let label = if call["voicemail"] == true { crate::i18n::t("Voicemail") } else { crate::i18n::t("Missed call") };
    Some(IntegrationEvent { success: false, label, detail: (!who.is_empty()).then_some(who) })
}

async fn fetch(id: &str, token: &str) -> Result<Vec<Value>, String> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Ronron")
        .build()
        .map_err(|_| crate::i18n::t("Not available"))?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs() as i64;
    let mut calls = Vec::new();
    for page in 1..=MAX_PAGES {
        let mut url = reqwest::Url::parse(CALLS_URL).map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("from", &(now - WINDOW_SECS).to_string())
            .append_pair("order", "desc")
            .append_pair("per_page", &PER_PAGE.to_string())
            .append_pair("page", &page.to_string());
        let mut response = http
            .get(url)
            .basic_auth(id, Some(token))
            .send()
            .await
            .map_err(|_| crate::i18n::t("Not available"))?;
        let status = response.status().as_u16();
        if status == 401 || status == 403 {
            return Err(crate::i18n::t("Aircall refused the API ID or token."));
        }
        if !response.status().is_success() {
            return Err(crate::i18n::tf("API error {code}", &[("code", &status.to_string())]));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| crate::i18n::t("Unexpected API response."))? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(crate::i18n::t("The server's answer is too large."));
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| crate::i18n::t("Unexpected API response."))?;
        let (page_calls, more) = parse_page(&value)?;
        calls.extend(page_calls);
        if !more {
            break;
        }
    }
    Ok(calls)
}

/// One page of GET /v1/calls, each call cut down to what the pill shows, and
/// whether another page follows.
fn parse_page(value: &Value) -> Result<(Vec<Value>, bool), String> {
    let list = value["calls"].as_array().ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    let calls = list.iter().filter_map(call).collect();
    let more = value["meta"]["next_page_link"].as_str().is_some_and(|l| !l.is_empty());
    Ok((calls, more))
}

fn call(c: &Value) -> Option<Value> {
    let id = c["id"].as_u64().map(|n| n.to_string()).or_else(|| c["id"].as_str().map(str::to_string))?;
    let started = c["started_at"].as_i64()?;
    let inbound = c["direction"].as_str() == Some("inbound");
    let answered = c["answered_at"].as_i64().is_some();
    let done = c["status"].as_str() == Some("done");
    let missed = inbound && done && (!answered || c["missed_call_reason"].as_str().is_some_and(|r| !r.is_empty()));
    let contact = &c["contact"];
    let person = [contact["first_name"].as_str(), contact["last_name"].as_str()]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let name = if person.is_empty() { contact["company_name"].as_str().unwrap_or("").to_string() } else { person };
    Some(json!({
        "id": id,
        "direction": if inbound { "inbound" } else { "outbound" },
        "missed": missed,
        "ongoing": !done,
        "voicemail": c["voicemail"].as_str().is_some_and(|v| !v.is_empty()),
        "name": name,
        "number": c["raw_digits"].as_str().unwrap_or(""),
        "at": started * 1000,
        "duration": if answered { c["duration"].as_i64().unwrap_or(0) } else { 0 },
        "line": c["number"]["name"].as_str().unwrap_or(""),
        "agent": c["user"]["name"].as_str().unwrap_or(""),
    }))
}

/// A number as a `tel:` link takes it: digits and a leading +, nothing else.
pub fn dial_number(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let mut out: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    if out.len() < 3 || out.len() > 20 {
        return None;
    }
    if trimmed.starts_with('+') {
        out.insert(0, '+');
    }
    Some(out)
}

/// The island's "Call back": the system's phone app, with the number.
#[tauri::command]
pub fn aircall_call(number: String) -> Result<(), String> {
    let digits = dial_number(&number).ok_or_else(|| crate::i18n::t("Not a phone number."))?;
    crate::platform::open_url(&format!("tel:{digits}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(id: u64, direction: &str, answered: bool, voicemail: bool) -> Value {
        json!({
            "id": id, "direction": direction, "status": "done", "started_at": 1_700_000_000,
            "answered_at": if answered { json!(1_700_000_005) } else { Value::Null },
            "duration": 95, "raw_digits": "+33 6 12 34 56 78",
            "missed_call_reason": if answered { Value::Null } else { json!("agents_did_not_answer") },
            "voicemail": if voicemail { json!("https://example.com/vm.mp3") } else { Value::Null },
            "contact": { "first_name": "Anne", "last_name": "Martin", "company_name": "ACME" },
            "number": { "name": "Support" }, "user": { "name": "Greg" }
        })
    }

    #[test]
    fn a_page_of_calls_keeps_what_the_pill_shows() {
        let page = json!({ "calls": [raw(1, "inbound", false, true), raw(2, "outbound", true, false)],
            "meta": { "next_page_link": "https://api.aircall.io/v1/calls?page=2" } });
        let (calls, more) = parse_page(&page).unwrap();
        assert!(more);
        assert_eq!(calls[0]["id"], "1");
        assert_eq!(calls[0]["missed"], true);
        assert_eq!(calls[0]["voicemail"], true);
        assert_eq!(calls[0]["name"], "Anne Martin");
        assert_eq!(calls[0]["duration"], 0);
        assert_eq!(calls[0]["at"], 1_700_000_000_000i64);
        assert_eq!(calls[1]["direction"], "outbound");
        assert_eq!(calls[1]["missed"], false);
        assert_eq!(calls[1]["duration"], 95);
        assert_eq!(calls[1]["line"], "Support");
        let last = json!({ "calls": [], "meta": { "next_page_link": null } });
        assert!(!parse_page(&last).unwrap().1);
        assert!(parse_page(&json!({})).is_err());
    }

    #[test]
    fn a_call_without_a_contact_name_shows_the_company_or_nothing() {
        let mut c = raw(3, "inbound", true, false);
        c["contact"] = json!({ "first_name": null, "last_name": "", "company_name": "ACME" });
        assert_eq!(call(&c).unwrap()["name"], "ACME");
        c["contact"] = Value::Null;
        assert_eq!(call(&c).unwrap()["name"], "");
        c["status"] = json!("answered");
        assert_eq!(call(&c).unwrap()["ongoing"], true);
    }

    #[test]
    fn only_digits_and_a_leading_plus_are_dialled() {
        assert_eq!(dial_number("+33 6 12-34.56 78").as_deref(), Some("+33612345678"));
        assert_eq!(dial_number("01 23 45 67 89").as_deref(), Some("0123456789"));
        assert_eq!(dial_number("12"), None);
        assert_eq!(dial_number("javascript:alert(1)"), None);
    }

    #[test]
    fn the_first_poll_only_takes_note_then_a_new_missed_call_alerts() {
        let first = vec![call(&raw(10, "inbound", false, false)).unwrap()];
        assert!(news(&first).is_none());
        assert!(news(&first).is_none());
        let next = vec![call(&raw(11, "inbound", false, false)).unwrap(), first[0].clone()];
        let event = news(&next).unwrap();
        assert!(!event.success);
        assert_eq!(event.detail.as_deref(), Some("Anne Martin"));
        assert!(news(&next).is_none());
    }
}
