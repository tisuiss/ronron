//! Device code sign-in to Microsoft Entra, and the tokens that come out of it.
//!
//! The access token lives only in memory. The refresh token goes to the OS
//! credential store, split over a few entries: Windows' Credential Manager holds
//! at most 1280 characters per entry and Entra's refresh tokens can be longer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::secrets;

pub const CLIENT_ID_KEY: &str = "todo-client-id";
pub const TENANT_KEY: &str = "todo-tenant";
const ACCOUNT_KEY: &str = "todo-account";
/// The refresh token's pieces, in order. Entry 0 doubles as "signed in" for the pill.
pub const REFRESH_KEYS: [&str; 4] = ["todo-refresh-0", "todo-refresh-1", "todo-refresh-2", "todo-refresh-3"];
const CHUNK: usize = 1200;

const SCOPE: &str = "offline_access openid profile Tasks.ReadWrite";
/// Personal Microsoft accounts all sign in through this tenant.
const CONSUMER_TENANT: &str = "9188040d-6c67-4c5b-b112-36a304b66dad";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(skip)]
    device_code: String,
    #[serde(skip)]
    interval: u64,
    #[serde(skip)]
    generation: u64,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    user_code: String,
    device_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// Each sign-in gets a number; cancelling or starting again retires the old one.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACCESS: LazyLock<Mutex<Option<(String, Instant)>>> = LazyLock::new(|| Mutex::new(None));
/// One refresh at a time: Entra rotates the refresh token on each use.
static REFRESHING: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// "common" unless the user entered a tenant: its ID, or a domain.
pub fn tenant() -> String {
    secrets::get(TENANT_KEY).map(|t| t.trim().to_string()).filter(|t| valid_tenant(t))
        .unwrap_or_else(|| "common".to_string())
}

/// A GUID, a domain or one of Entra's aliases: nothing that could change the URL's shape.
pub fn valid_tenant(t: &str) -> bool {
    !t.is_empty() && t.len() <= 100
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && !t.starts_with('.') && !t.contains("..")
}

fn authority() -> String {
    format!("https://login.microsoftonline.com/{}/oauth2/v2.0", tenant())
}

fn client_id() -> Result<String, String> {
    secrets::get(CLIENT_ID_KEY).map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
        .ok_or_else(|| crate::i18n::t("Enter the application (client) ID first."))
}

pub fn signed_in() -> bool {
    secrets::present(REFRESH_KEYS[0])
}

// ── The account ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct Account {
    name: String,
    #[serde(default)]
    tid: String,
}

fn stored_account() -> Option<Account> {
    serde_json::from_str(&secrets::get(ACCOUNT_KEY)?).ok()
}

pub fn account() -> Option<String> {
    stored_account().map(|a| a.name).filter(|n| !n.is_empty())
}

/// Microsoft To Do on the web, for work or for personal accounts.
pub fn web_url() -> &'static str {
    match stored_account() {
        Some(a) if a.tid == CONSUMER_TENANT => "https://to-do.live.com/tasks/",
        _ => "https://to-do.office.com/tasks/",
    }
}

/// Who signed in, from the ID token. Display only: the token is not verified,
/// and nothing is decided on it.
fn account_from_id_token(id_token: &str) -> Option<Account> {
    let payload = id_token.split('.').nth(1)?;
    let claims: Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    let name = ["preferred_username", "email", "name"].iter()
        .find_map(|k| claims.get(*k).and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let tid = claims.get("tid").and_then(Value::as_str).unwrap_or("").to_string();
    Some(Account { name, tid })
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

// ── The refresh token ────────────────────────────────────────────────────────

fn chunks(token: &str) -> Option<Vec<&str>> {
    let pieces: Vec<&str> = token.as_bytes().chunks(CHUNK)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    (pieces.len() <= REFRESH_KEYS.len() && pieces.iter().all(|p| !p.is_empty())).then_some(pieces)
}

fn store_refresh(token: &str) -> Result<(), String> {
    // Tokens are ASCII; anything else is not one.
    if !token.is_ascii() {
        return Err(crate::i18n::t("Unexpected API response."));
    }
    let pieces = chunks(token).ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    for (i, key) in REFRESH_KEYS.iter().enumerate() {
        secrets::set(key, pieces.get(i).copied().unwrap_or(""))?;
    }
    Ok(())
}

fn load_refresh() -> Option<String> {
    let mut token = String::new();
    for key in REFRESH_KEYS {
        match secrets::get(key) {
            Some(piece) => token.push_str(&piece),
            None => break,
        }
    }
    (!token.is_empty()).then_some(token)
}

pub fn sign_out() {
    cancel();
    for key in REFRESH_KEYS.iter().chain([&ACCOUNT_KEY]) {
        let _ = secrets::set(key, "");
    }
    *ACCESS.lock().unwrap() = None;
}

// ── Talking to Entra ─────────────────────────────────────────────────────────

/// What a failed token request means.
#[derive(Debug, PartialEq)]
enum TokenError {
    Pending,
    SlowDown,
    /// Sign in again: declined, expired or revoked.
    Final(String),
}

fn token_error(body: &Value) -> TokenError {
    let error = body.get("error").and_then(Value::as_str).unwrap_or("");
    let description = body.get("error_description").and_then(Value::as_str).unwrap_or("");
    // "AADSTS7000218: …": the code alone says what to fix in the app registration.
    let code = description.split(':').next().filter(|c| c.starts_with("AADSTS")).unwrap_or(error);
    match error {
        "authorization_pending" => TokenError::Pending,
        "slow_down" => TokenError::SlowDown,
        "authorization_declined" => TokenError::Final(crate::i18n::t("Sign-in canceled.")),
        "expired_token" | "bad_verification_code" => TokenError::Final(crate::i18n::t("The code expired. Sign in again.")),
        _ => TokenError::Final(crate::i18n::tf("Microsoft refused the sign-in ({code}).", &[("code", code)])),
    }
}

async fn post_form(url: &str, form: &[(&str, &str)]) -> Result<(bool, Value), String> {
    let response = super::http()?.post(url).form(form).send().await
        .map_err(|_| crate::i18n::t("Not available"))?;
    let ok = response.status().is_success();
    let body: Value = response.json().await.map_err(|_| crate::i18n::t("Unexpected API response."))?;
    Ok((ok, body))
}

/// Asks Entra for a code to show the user.
pub async fn start() -> Result<DeviceCode, String> {
    let client = client_id()?;
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let (ok, body) = post_form(&format!("{}/devicecode", authority()), &[("client_id", &client), ("scope", SCOPE)]).await?;
    if !ok {
        let TokenError::Final(message) = token_error(&body) else { return Err(crate::i18n::t("Unexpected API response.")) };
        return Err(message);
    }
    let r: DeviceCodeResponse = serde_json::from_value(body).map_err(|_| crate::i18n::t("Unexpected API response."))?;
    Ok(DeviceCode {
        user_code: r.user_code,
        verification_uri: r.verification_uri,
        expires_in: r.expires_in,
        device_code: r.device_code,
        interval: r.interval.clamp(1, 30),
        generation,
    })
}

pub fn cancel() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Polls until the user has entered the code, then keeps the tokens. Returns
/// the account's name.
pub async fn wait(code: &DeviceCode) -> Result<String, String> {
    let client = client_id()?;
    let deadline = Instant::now() + Duration::from_secs(code.expires_in.min(1800));
    let mut interval = code.interval;
    let url = format!("{}/token", authority());
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if GENERATION.load(Ordering::SeqCst) != code.generation {
            return Err(crate::i18n::t("Sign-in canceled."));
        }
        if Instant::now() > deadline {
            return Err(crate::i18n::t("The code expired. Sign in again."));
        }
        let (ok, body) = match post_form(&url, &[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", &client),
            ("device_code", &code.device_code),
        ]).await {
            Ok(r) => r,
            Err(_) => continue, // the network blinked: try again at the next tick
        };
        if ok {
            return keep(&body);
        }
        match token_error(&body) {
            TokenError::Pending => {}
            TokenError::SlowDown => interval += 5,
            TokenError::Final(message) => return Err(message),
        }
    }
}

/// Stores what a successful token response holds. Returns the account's name.
fn keep(body: &Value) -> Result<String, String> {
    let access = body.get("access_token").and_then(Value::as_str);
    let refresh = body.get("refresh_token").and_then(Value::as_str);
    let (Some(access), Some(refresh)) = (access, refresh) else {
        return Err(crate::i18n::t("Unexpected API response."));
    };
    let expires = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
    store_refresh(refresh)?;
    *ACCESS.lock().unwrap() = Some((access.to_string(), Instant::now() + Duration::from_secs(expires.saturating_sub(120))));
    if let Some(account) = body.get("id_token").and_then(Value::as_str).and_then(account_from_id_token) {
        let _ = secrets::set(ACCOUNT_KEY, &serde_json::to_string(&account).unwrap_or_default());
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
    let refresh = load_refresh().ok_or_else(|| crate::i18n::t("Sign in to Microsoft again."))?;
    let client = client_id()?;
    let (ok, body) = post_form(&format!("{}/token", authority()), &[
        ("grant_type", "refresh_token"),
        ("client_id", &client),
        ("refresh_token", &refresh),
        ("scope", SCOPE),
    ]).await?;
    if !ok {
        if let TokenError::Final(_) = token_error(&body) {
            // Revoked, expired or the app changed: the stored token is useless.
            if body.get("error").and_then(Value::as_str) == Some("invalid_grant") {
                sign_out();
            }
        }
        return Err(crate::i18n::t("Sign in to Microsoft again."));
    }
    keep(&body)?;
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
    fn tenants_cannot_change_the_url() {
        for ok in ["common", "organizations", "contoso.onmicrosoft.com", "72f988bf-86f1-41af-91ab-2d7cd011db47"] {
            assert!(valid_tenant(ok), "{ok}");
        }
        for bad in ["", "a/b", "evil.com/x", "a?b", "..", ".x", "a..b", "x y", "é"] {
            assert!(!valid_tenant(bad), "{bad}");
        }
    }

    #[test]
    fn long_refresh_tokens_are_split_and_too_long_ones_refused() {
        let token = "x".repeat(3000);
        let pieces = chunks(&token).unwrap();
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| p.len() <= CHUNK));
        assert_eq!(pieces.concat(), token);
        assert!(chunks(&"x".repeat(CHUNK * 4 + 1)).is_none());
        // Each piece stays under the Credential Manager's 1280 UTF-16 characters.
        assert!(CHUNK <= 1280);
    }

    #[test]
    fn the_id_token_names_the_account() {
        // {"preferred_username":"greg@contoso.ch","tid":"t1"}
        let payload = "eyJwcmVmZXJyZWRfdXNlcm5hbWUiOiJncmVnQGNvbnRvc28uY2giLCJ0aWQiOiJ0MSJ9";
        let account = account_from_id_token(&format!("h.{payload}.s")).unwrap();
        assert_eq!(account.name, "greg@contoso.ch");
        assert_eq!(account.tid, "t1");
        assert!(account_from_id_token("nope").is_none());
    }

    #[test]
    fn token_errors_say_what_to_do() {
        assert_eq!(token_error(&json!({ "error": "authorization_pending" })), TokenError::Pending);
        assert_eq!(token_error(&json!({ "error": "slow_down" })), TokenError::SlowDown);
        let TokenError::Final(m) = token_error(&json!({
            "error": "invalid_client",
            "error_description": "AADSTS7000218: The request body must contain client_assertion."
        })) else { panic!() };
        assert!(m.contains("AADSTS7000218"), "{m}");
    }
}
