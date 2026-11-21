//! Sign-in to Google, and the tokens that come out of it.
//!
//! Each user signs in with their own Google Cloud project (a "Desktop app" OAuth
//! client): its client ID and secret go to the OS credential store, and Google's
//! page opens in the browser and comes back to a one-shot server on this
//! computer (authorization code + PKCE). The access token lives only in memory,
//! the refresh token in the credential store.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::msgraph::auth::{base64url, base64url_encode, pkce, random};
use crate::secrets;

pub const CLIENT_ID_KEY: &str = "google-client-id";
pub const CLIENT_SECRET_KEY: &str = "google-client-secret";
const ACCOUNT_KEY: &str = "google-account";
/// Google's refresh tokens are about a hundred characters: one entry holds one.
pub const REFRESH_KEY: &str = "google-refresh";

/// Gmail (read, label, archive, trash, drafts — never a permanent delete), the
/// calendars (read only) and the tasks.
const SCOPE: &str = "openid email https://www.googleapis.com/auth/gmail.modify https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/tasks";
const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";

static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACCESS: LazyLock<Mutex<Option<(String, Instant)>>> = LazyLock::new(|| Mutex::new(None));
static REFRESHING: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

fn stored(key: &str) -> Option<String> {
    secrets::get(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The user's OAuth client: (client ID, client secret).
fn client() -> Result<(String, String), String> {
    match (stored(CLIENT_ID_KEY), stored(CLIENT_SECRET_KEY)) {
        (Some(id), Some(secret)) => Ok((id, secret)),
        _ => Err(crate::i18n::t("Enter your Google client ID and secret first.")),
    }
}

pub fn signed_in() -> bool {
    secrets::present(REFRESH_KEY)
}

pub fn account() -> Option<String> {
    stored(ACCOUNT_KEY)
}

pub fn sign_out() {
    cancel();
    let _ = secrets::set(REFRESH_KEY, "");
    let _ = secrets::set(ACCOUNT_KEY, "");
    *ACCESS.lock().unwrap() = None;
}

pub fn cancel() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// The address the ID token names. Display only: the token is not verified.
fn email_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let claims: Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    claims.get("email").and_then(Value::as_str).map(String::from).filter(|e| !e.is_empty())
}

/// What Google's error means for the user.
fn token_error(body: &Value) -> String {
    let error = body.get("error").and_then(Value::as_str).unwrap_or("");
    match error {
        "access_denied" => crate::i18n::t("Sign-in canceled."),
        "invalid_grant" => crate::i18n::t("Sign in to Google again."),
        "" => crate::i18n::t("Unexpected API response."),
        code => crate::i18n::tf("Google refused the sign-in ({code}).", &[("code", code)]),
    }
}

async fn post_form(form: &[(&str, &str)]) -> Result<(bool, Value), String> {
    let response = super::http()?.post(TOKEN).form(form).send().await
        .map_err(|_| crate::i18n::t("Not available"))?;
    let ok = response.status().is_success();
    let body: Value = response.json().await.map_err(|_| crate::i18n::t("Unexpected API response."))?;
    Ok((ok, body))
}

pub(crate) fn authorize_url(client: &str, redirect: &str, state: &str, challenge: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(AUTHORIZE).map_err(|_| crate::i18n::t("Unexpected API response."))?;
    url.query_pairs_mut()
        .append_pair("client_id", client)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect)
        .append_pair("scope", SCOPE)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        // A refresh token every time, and the choice of account.
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent select_account");
    Ok(url.into())
}

/// What came back on the loopback: the code, or Google's error.
pub(crate) fn read_redirect(request_line: &str, state: &str) -> Option<Result<String, String>> {
    let target = request_line.strip_prefix("GET ")?.split(' ').next()?;
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/" {
        return None; // the favicon and the like
    }
    let get = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
    if get("state").as_deref() != Some(state) {
        return Some(Err(crate::i18n::t("Unexpected API response.")));
    }
    if let Some(error) = get("error") {
        return Some(Err(token_error(&serde_json::json!({ "error": error }))));
    }
    get("code").map(Ok)
}

async fn answer(stream: &mut tokio::net::TcpStream, ok: bool) {
    use tokio::io::AsyncWriteExt;
    let text = if ok {
        crate::i18n::t("Ronron is signed in to Google. You can close this tab.")
    } else {
        crate::i18n::t("Sign-in failed. Close this tab and try again in Ronron.")
    };
    let text = text.replace('&', "&amp;").replace('<', "&lt;");
    let body = format!("<!doctype html><meta charset=utf-8><title>Ronron</title><body style=\"font:16px system-ui;display:grid;place-items:center;height:90vh;background:#111;color:#eee\"><p>{text}</p>");
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Opens Google's sign-in page in the browser and waits for it to come back to a
/// one-shot server on this computer. Returns the account's address.
pub async fn browser() -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let (client, secret) = client()?;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let (verifier, challenge) = pkce()?;
    let state = base64url_encode(&random(16)?);
    // Google's desktop clients come back to the loopback address on any port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|_| crate::i18n::t("Not available"))?;
    let port = listener.local_addr().map_err(|_| crate::i18n::t("Not available"))?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    crate::platform::open_url(&authorize_url(&client, &redirect, &state, &challenge)?);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    let code = loop {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return Err(crate::i18n::t("Sign-in canceled."));
        }
        let (mut stream, _) = match tokio::time::timeout(Duration::from_secs(1), listener.accept()).await {
            Ok(Ok(s)) => s,
            Ok(Err(_)) => continue,
            Err(_) => {
                if tokio::time::Instant::now() > deadline {
                    return Err(crate::i18n::t("The code expired. Sign in again."));
                }
                continue;
            }
        };
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await.ok().and_then(|r| r.ok()).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        match read_redirect(request.lines().next().unwrap_or(""), &state) {
            None => {
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            }
            Some(result) => {
                answer(&mut stream, result.is_ok()).await;
                break result?;
            }
        }
    };

    let (ok, body) = post_form(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client),
        ("client_secret", &secret),
        ("code", &code),
        ("redirect_uri", &redirect),
        ("code_verifier", &verifier),
    ]).await?;
    if !ok {
        return Err(token_error(&body));
    }
    keep(&body, true)
}

/// Stores what a successful token response holds. Returns the account's address.
/// Google sends a refresh token only at sign-in; a refresh keeps the one stored.
fn keep(body: &Value, sign_in: bool) -> Result<String, String> {
    let access = body.get("access_token").and_then(Value::as_str).ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    if let Some(refresh) = body.get("refresh_token").and_then(Value::as_str) {
        if !refresh.is_ascii() || refresh.len() > 1200 {
            return Err(crate::i18n::t("Unexpected API response."));
        }
        secrets::set(REFRESH_KEY, refresh)?;
    } else if sign_in {
        return Err(crate::i18n::t("Unexpected API response."));
    }
    let expires = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
    *ACCESS.lock().unwrap() = Some((access.to_string(), Instant::now() + Duration::from_secs(expires.saturating_sub(120))));
    if let Some(email) = body.get("id_token").and_then(Value::as_str).and_then(email_from_id_token) {
        let _ = secrets::set(ACCOUNT_KEY, &email);
    }
    Ok(account().unwrap_or_default())
}

/// A valid access token, refreshed when it is about to expire.
pub async fn access_token() -> Result<String, String> {
    if let Some(token) = cached() {
        return Ok(token);
    }
    let _guard = REFRESHING.lock().await;
    if let Some(token) = cached() {
        return Ok(token);
    }
    let refresh = stored(REFRESH_KEY).ok_or_else(|| crate::i18n::t("Sign in to Google again."))?;
    let (client, secret) = client()?;
    let (ok, body) = post_form(&[
        ("grant_type", "refresh_token"),
        ("client_id", &client),
        ("client_secret", &secret),
        ("refresh_token", &refresh),
    ]).await?;
    if !ok {
        // Revoked, expired (a project left in "Testing" keeps them 7 days) or the client changed.
        if body.get("error").and_then(Value::as_str) == Some("invalid_grant") {
            sign_out();
        }
        return Err(crate::i18n::t("Sign in to Google again."));
    }
    keep(&body, false)?;
    cached().ok_or_else(|| crate::i18n::t("Unexpected API response."))
}

fn cached() -> Option<String> {
    let guard = ACCESS.lock().unwrap();
    guard.as_ref().filter(|(_, until)| Instant::now() < *until).map(|(t, _)| t.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_authorize_url_asks_for_a_refresh_token_with_pkce() {
        let url = authorize_url("cid.apps.googleusercontent.com", "http://127.0.0.1:5000", "st", "ch").unwrap();
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"), "{url}");
        for part in ["access_type=offline", "code_challenge_method=S256", "redirect_uri=http%3A%2F%2F127.0.0.1%3A5000", "gmail.modify", "calendar.readonly", "auth%2Ftasks"] {
            assert!(url.contains(part), "{part} in {url}");
        }
    }

    #[test]
    fn the_redirect_is_read_and_checked() {
        assert_eq!(read_redirect("GET /?state=s1&code=4/0Abc&scope=email HTTP/1.1", "s1"), Some(Ok("4/0Abc".into())));
        assert!(matches!(read_redirect("GET /?code=abc&state=other HTTP/1.1", "s1"), Some(Err(_))));
        assert_eq!(read_redirect("GET /favicon.ico HTTP/1.1", "s1"), None);
        assert_eq!(read_redirect("POST / HTTP/1.1", "s1"), None);
        assert!(matches!(read_redirect("GET /?error=access_denied&state=s1 HTTP/1.1", "s1"), Some(Err(_))));
    }

    #[test]
    fn the_id_token_names_the_account() {
        // {"email":"anne@gmail.com"}
        let payload = "eyJlbWFpbCI6ImFubmVAZ21haWwuY29tIn0";
        assert_eq!(email_from_id_token(&format!("h.{payload}.s")).as_deref(), Some("anne@gmail.com"));
        assert_eq!(email_from_id_token("nope"), None);
    }

    #[test]
    fn errors_say_what_to_do() {
        assert!(token_error(&json!({ "error": "invalid_client" })).contains("invalid_client"));
        assert_eq!(token_error(&json!({})), crate::i18n::t("Unexpected API response."));
    }
}
