//! Microsoft 365 through Microsoft Graph: one sign-in for the To Do, Calendar
//! and Mail pills.
//!
//! The usual sign-in opens Microsoft's page in the browser and comes back to a
//! one-shot server on localhost (authorization code + PKCE); the device code
//! flow is there for a browser on another device. The app is the build's own
//! (RONRON_MS_CLIENT_ID) or the user's (Settings). Ronron only ever talks to
//! login.microsoftonline.com and graph.microsoft.com, and only once signed in.

pub mod auth;

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

const ME: &str = "https://graph.microsoft.com/v1.0/me";
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BODY: usize = 8 * 1024 * 1024;

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

/// `/me/…` with each segment escaped (IDs may hold `/`, `=`, `+`).
pub fn url(segments: &[&str]) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(ME).map_err(|_| invalid())?;
    url.path_segments_mut().map_err(|_| invalid())?.extend(segments);
    Ok(url)
}

/// A next page Graph handed back, only if it stays on Graph.
pub fn next_link(value: &Value) -> Option<reqwest::Url> {
    let link = reqwest::Url::parse(value.get("@odata.nextLink")?.as_str()?).ok()?;
    (link.scheme() == "https" && link.host_str() == Some("graph.microsoft.com")).then_some(link)
}

/// Sends a request with a fresh token and reads Graph's JSON answer (Null when empty).
pub async fn call(build: impl FnOnce(&reqwest::Client, &str) -> reqwest::RequestBuilder) -> Result<Value, String> {
    let token = auth::access_token().await?;
    let http = http()?;
    let mut response = build(&http, &token).send().await.map_err(|_| crate::i18n::t("Not available"))?;
    let status = response.status().as_u16();
    if status == 401 {
        return Err(crate::i18n::t("Sign in to Microsoft again."));
    }
    if status == 403 {
        // Usually a permission the account or the app has not been given.
        return Err(crate::i18n::tf("API error {code}", &[("code", "403")]));
    }
    if !(200..300).contains(&status) {
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

/// Refreshes every Microsoft 365 pill after a sign-in or a sign-out.
async fn refresh_all(app: &AppHandle) {
    crate::todo::poll_now(app).await;
    crate::calendar365::poll_now(app).await;
    crate::mail::poll(app).await;
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Who is signed in ("" when the name is unknown), or None.
#[tauri::command]
pub fn ms_account() -> Option<String> {
    auth::signed_in().then(|| auth::account().unwrap_or_default())
}

/// Whether this build carries its own Entra app, so no client ID is needed.
#[tauri::command]
pub fn ms_builtin_app() -> bool {
    auth::has_builtin()
}

fn report(app: &AppHandle, outcome: &Result<String, String>) {
    let payload = match outcome {
        Ok(account) => json!({ "ok": true, "account": account }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    let _ = app.emit("ms-auth", payload);
}

/// The usual sign-in, in the browser. Returns at once; the outcome arrives as an `ms-auth` event.
#[tauri::command]
pub fn ms_sign_in_browser(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let outcome = auth::browser().await;
        report(&app, &outcome);
        if outcome.is_ok() {
            refresh_all(&app).await;
        }
    });
}

/// The device code sign-in: returns the code and where to enter it.
#[tauri::command]
pub async fn ms_sign_in_code(app: AppHandle) -> Result<auth::DeviceCode, String> {
    let code = auth::start().await?;
    let device = code.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = auth::wait(&device).await;
        report(&app, &outcome);
        if outcome.is_ok() {
            refresh_all(&app).await;
        }
    });
    Ok(code)
}

#[tauri::command]
pub fn ms_sign_in_cancel() {
    auth::cancel();
}

/// Forgets the account: the refresh token leaves the credential store.
#[tauri::command]
pub async fn ms_sign_out(app: AppHandle) {
    auth::sign_out();
    refresh_all(&app).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_path_segments_and_links_stay_on_graph() {
        let u = url(&["messages", "AAMk/=?#x"]).unwrap();
        assert_eq!(u.as_str(), "https://graph.microsoft.com/v1.0/me/messages/AAMk%2F=%3F%23x");
        assert!(next_link(&json!({ "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/messages?$skip=10" })).is_some());
        assert!(next_link(&json!({ "@odata.nextLink": "https://evil.example/v1.0" })).is_none());
        assert!(next_link(&json!({ "@odata.nextLink": "http://graph.microsoft.com/v1.0" })).is_none());
        assert!(next_link(&json!({})).is_none());
    }
}
