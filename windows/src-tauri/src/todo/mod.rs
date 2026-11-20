//! Microsoft To Do through Microsoft Graph (sign-in: crate::msgraph).
//!
//! The pill polls every two minutes while it is declared (integrations.rs). The
//! card can complete a task, undo that, and add one to the default list.

mod graph;

use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::integrations::{self, IntegrationUpdate};
use crate::msgraph;

pub const ID: &str = "integration_todo";

fn active(app: &AppHandle) -> bool {
    !integrations::PAUSED.load(std::sync::atomic::Ordering::Relaxed) && integrations::enabled(app, ID)
}

/// One refresh, emitted to the island like every other integration.
pub async fn poll(app: AppHandle) {
    if active(&app) {
        poll_now(&app).await;
    }
}

/// Refreshes now (after a change or a sign-in), or clears the card when signed out.
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

async fn fetch() -> Result<Value, String> {
    let lists = graph::lists().await?;
    let mut tasks = Vec::new();
    for list in lists.iter().take(graph::MAX_LISTS) {
        tasks.extend(graph::open_tasks(list).await?);
        if tasks.len() >= graph::MAX_TASKS {
            break;
        }
    }
    tasks.truncate(graph::MAX_TASKS);
    Ok(json!({
        "account": msgraph::auth::account(),
        "webUrl": msgraph::auth::web_url(),
        "lists": lists.iter().map(|l| json!({ "id": l.id, "name": l.name, "default": l.default })).collect::<Vec<_>>(),
        "tasks": tasks,
    }))
}

/// Ticks a task off (or back on, with `done: false`, for Undo).
#[tauri::command]
pub async fn todo_set_done(app: AppHandle, list_id: String, task_id: String, done: bool) -> Result<(), String> {
    graph::set_status(&list_id, &task_id, done).await?;
    poll_now(&app).await;
    Ok(())
}

/// Adds a task to `list_id`, or to the default list ("Tasks") when it is empty.
#[tauri::command]
pub async fn todo_add(app: AppHandle, title: String, list_id: Option<String>) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(());
    }
    let list = match list_id.filter(|id| !id.is_empty()) {
        Some(id) => id,
        None => graph::lists().await?
            .into_iter()
            .find(|l| l.default)
            .map(|l| l.id)
            .ok_or_else(|| crate::i18n::t("Unexpected API response."))?,
    };
    graph::add(&list, title).await?;
    poll_now(&app).await;
    Ok(())
}
