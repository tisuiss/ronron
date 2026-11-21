//! A Google account: one sign-in for Gmail (the Mail tab), Google Calendar (the
//! Calendar tab, read only) and Google Tasks (the To Do tab).
//!
//! Beside Microsoft 365, not instead of it: signed in to both, each tab shows
//! both accounts. Each user brings their own Google Cloud project (Settings →
//! Integrations → Google), so no Ronron app has to pass Google's review. Ronron
//! only ever talks to accounts.google.com, oauth2.googleapis.com and the three
//! APIs, and only once signed in.

pub mod auth;
pub mod calendar;
pub mod tasks;

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_BODY: usize = 16 * 1024 * 1024;

pub const GMAIL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
pub const CALENDAR: &str = "https://www.googleapis.com/calendar/v3";
pub const TASKS: &str = "https://tasks.googleapis.com/tasks/v1";

pub fn signed_in() -> bool {
    auth::signed_in()
}

pub(crate) fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Ronron")
        .build()
        .map_err(|_| crate::i18n::t("Not available"))
}

fn invalid() -> String {
    crate::i18n::t("Unexpected API response.")
}

/// `base` + each segment escaped (Google's ids may hold `@`, `#`, `/`).
pub fn url(base: &str, segments: &[&str]) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(base).map_err(|_| invalid())?;
    url.path_segments_mut().map_err(|_| invalid())?.extend(segments);
    Ok(url)
}

/// Sends a request with a fresh token and reads Google's JSON answer (Null when empty).
pub async fn call(build: impl FnOnce(&reqwest::Client, &str) -> reqwest::RequestBuilder) -> Result<Value, String> {
    let token = auth::access_token().await?;
    let http = http()?;
    let mut response = build(&http, &token).send().await.map_err(|_| crate::i18n::t("Not available"))?;
    let status = response.status().as_u16();
    if status == 401 {
        return Err(crate::i18n::t("Sign in to Google again."));
    }
    if !(200..300).contains(&status) {
        // 403: usually an API not turned on in the user's Google Cloud project.
        return Err(crate::i18n::tf("API error {code}", &[("code", &status.to_string())]));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
        if body.len() + chunk.len() > MAX_BODY {
            return Err(crate::i18n::t("The server's answer is too large."));
        }
        body.extend_from_slice(&chunk);
    }
    if body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&body).map_err(|_| invalid())
}

pub async fn get(url: reqwest::Url) -> Result<Value, String> {
    call(|h, t| h.get(url).bearer_auth(t)).await
}

pub async fn post(url: reqwest::Url, body: Value) -> Result<Value, String> {
    call(|h, t| h.post(url).bearer_auth(t).json(&body)).await
}

pub async fn patch(url: reqwest::Url, body: Value) -> Result<Value, String> {
    call(|h, t| h.patch(url).bearer_auth(t).json(&body)).await
}

/// Refreshes every tab a Google account feeds, after a sign-in or a sign-out.
async fn refresh_all(app: &AppHandle) {
    crate::todo::poll_now(app).await;
    crate::calendar365::poll_now(app).await;
    crate::mail::poll(app).await;
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Who is signed in ("" when the address is unknown), or None.
#[tauri::command]
pub fn google_account() -> Option<String> {
    auth::signed_in().then(|| auth::account().unwrap_or_default())
}

/// The sign-in, in the browser. Returns at once; the outcome arrives as a `google-auth` event.
#[tauri::command]
pub fn google_sign_in(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let outcome = auth::browser().await;
        let payload = match &outcome {
            Ok(account) => json!({ "ok": true, "account": account }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        let _ = app.emit("google-auth", payload);
        if outcome.is_ok() {
            refresh_all(&app).await;
        }
    });
}

#[tauri::command]
pub fn google_sign_in_cancel() {
    auth::cancel();
}

/// Forgets the account: the refresh token leaves the credential store.
#[tauri::command]
pub async fn google_sign_out(app: AppHandle) {
    auth::sign_out();
    refresh_all(&app).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_path_segments() {
        let u = url(CALENDAR, &["calendars", "anne@gmail.com", "events"]).unwrap();
        assert_eq!(u.as_str(), "https://www.googleapis.com/calendar/v3/calendars/anne@gmail.com/events");
        let u = url(CALENDAR, &["calendars", "a#b/c?d", "events"]).unwrap();
        assert_eq!(u.as_str(), "https://www.googleapis.com/calendar/v3/calendars/a%23b%2Fc%3Fd/events");
        let u = url(GMAIL, &["messages", "18c0f"]).unwrap();
        assert_eq!(u.as_str(), "https://gmail.googleapis.com/gmail/v1/users/me/messages/18c0f");
    }
}
