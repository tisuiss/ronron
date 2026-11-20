//! Microsoft To Do through Microsoft Graph.
//!
//! Sign-in uses the OAuth device code flow against the user's own Entra app
//! registration (a public client with the delegated Tasks.ReadWrite permission):
//! Ronron shows a code, the user enters it at microsoft.com/devicelogin, and the
//! refresh token goes to the OS credential store. Nothing is sent anywhere but
//! login.microsoftonline.com and graph.microsoft.com, and only once the user has
//! entered a client ID and signed in.
//!
//! The pill polls every two minutes while it is declared (integrations.rs). The
//! card can complete a task, undo that, and add one to the default list.

mod auth;
mod graph;

use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::integrations::{self, IntegrationUpdate};

pub const ID: &str = "integration_todo";
const TIMEOUT: Duration = Duration::from_secs(12);

fn active(app: &AppHandle) -> bool {
    !integrations::PAUSED.load(std::sync::atomic::Ordering::Relaxed) && integrations::enabled(app, ID)
}

/// One refresh, emitted to the island like every other integration.
pub async fn poll(app: AppHandle) {
    if !active(&app) || !auth::signed_in() {
        return;
    }
    let result = tokio::time::timeout(Duration::from_secs(40), fetch()).await
        .unwrap_or_else(|_| Err(crate::i18n::t("No response text.")));
    if !active(&app) {
        return;
    }
    let (data, error) = match result {
        Ok(data) => (data, None),
        Err(error) => (json!({}), Some(error)),
    };
    integrations::emit(&app, IntegrationUpdate { id: ID, data, error, event: None });
}

async fn fetch() -> Result<Value, String> {
    let token = auth::access_token().await?;
    let http = http()?;
    let lists = graph::lists(&http, &token).await?;
    let mut tasks = Vec::new();
    for list in lists.iter().take(graph::MAX_LISTS) {
        tasks.extend(graph::open_tasks(&http, &token, list).await?);
        if tasks.len() >= graph::MAX_TASKS {
            break;
        }
    }
    tasks.truncate(graph::MAX_TASKS);
    Ok(json!({
        "account": auth::account(),
        "webUrl": auth::web_url(),
        "lists": lists.iter().map(|l| json!({ "id": l.id, "name": l.name, "default": l.default })).collect::<Vec<_>>(),
        "tasks": tasks,
        "fetchedAt": now_ms(),
    }))
}

pub(crate) fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Ronron")
        .build()
        .map_err(|_| crate::i18n::t("Not available"))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Re-reads the lists after a change, so the card shows what Graph has.
async fn refresh(app: &AppHandle) {
    if auth::signed_in() {
        let result = fetch().await;
        let (data, error) = match result {
            Ok(data) => (data, None),
            Err(error) => (json!({}), Some(error)),
        };
        integrations::emit(app, IntegrationUpdate { id: ID, data, error, event: None });
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// What Settings shows: who is signed in ("" when the name is unknown), or None.
#[tauri::command]
pub fn todo_account() -> Option<String> {
    auth::signed_in().then(|| auth::account().unwrap_or_default())
}

/// Starts the device code sign-in. Returns the code and where to enter it; the
/// outcome arrives later as a `todo-auth` event.
#[tauri::command]
pub async fn todo_sign_in(app: AppHandle) -> Result<auth::DeviceCode, String> {
    let code = auth::start().await?;
    let device = code.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = auth::wait(&device).await;
        let payload = match &outcome {
            Ok(account) => json!({ "ok": true, "account": account }),
            Err(error) => json!({ "ok": false, "error": error }),
        };
        let _ = app.emit("todo-auth", payload);
        if outcome.is_ok() {
            refresh(&app).await;
        }
    });
    Ok(code)
}

/// Stops waiting for a sign-in in progress.
#[tauri::command]
pub fn todo_sign_in_cancel() {
    auth::cancel();
}

/// Forgets the account: the refresh token leaves the credential store.
#[tauri::command]
pub fn todo_sign_out(app: AppHandle) {
    auth::sign_out();
    integrations::emit(&app, IntegrationUpdate { id: ID, data: json!({}), error: None, event: None });
}

/// Ticks a task off (or back on, with `done: false`, for Undo).
#[tauri::command]
pub async fn todo_set_done(app: AppHandle, list_id: String, task_id: String, done: bool) -> Result<(), String> {
    let token = auth::access_token().await?;
    graph::set_status(&http()?, &token, &list_id, &task_id, done).await?;
    refresh(&app).await;
    Ok(())
}

/// Adds a task to `list_id`, or to the default list ("Tasks") when it is empty.
#[tauri::command]
pub async fn todo_add(app: AppHandle, title: String, list_id: Option<String>) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(());
    }
    let token = auth::access_token().await?;
    let http = http()?;
    let list = match list_id.filter(|id| !id.is_empty()) {
        Some(id) => id,
        None => graph::lists(&http, &token).await?
            .into_iter()
            .find(|l| l.default)
            .map(|l| l.id)
            .ok_or_else(|| crate::i18n::t("Unexpected API response."))?,
    };
    graph::add(&http, &token, &list, title).await?;
    refresh(&app).await;
    Ok(())
}
