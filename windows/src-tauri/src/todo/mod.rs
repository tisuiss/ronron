//! Microsoft To Do through Microsoft Graph (sign-in: crate::msgraph).
//!
//! The pill polls every two minutes while it is declared (integrations.rs). The
//! card can complete a task, undo that, and add one to the default list.
//!
//! Without a Microsoft 365 account, the tasks are kept on this computer (local.rs),
//! in a list of their own; signed in, that list stays beside the account's while it
//! holds tasks.

mod graph;
mod local;

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
    let result = if msgraph::signed_in() {
        tokio::time::timeout(Duration::from_secs(45), fetch()).await
            .unwrap_or_else(|_| Err(crate::i18n::t("No response text.")))
    } else {
        Ok(local_only())
    };
    let (data, error) = match result {
        Ok(data) => (data, None),
        Err(error) => (json!({}), Some(error)),
    };
    integrations::emit(app, IntegrationUpdate { id: ID, data, error, event: None });
}

fn local_list(default: bool) -> Value {
    json!({ "id": local::LIST_ID, "name": crate::i18n::t("On this computer"), "default": default })
}

fn local_tasks() -> Vec<Value> {
    let name = crate::i18n::t("On this computer");
    local::open().into_iter().map(|t| json!({
        "id": t.id, "listId": local::LIST_ID, "list": name, "title": t.title,
        "importance": t.importance, "due": t.due,
    })).collect()
}

/// Not signed in: the tasks kept on this computer.
fn local_only() -> Value {
    json!({ "local": true, "webUrl": "https://to-do.office.com/tasks/", "lists": [local_list(true)], "tasks": local_tasks() })
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
    let mut lists: Vec<Value> = lists.iter().map(|l| json!({ "id": l.id, "name": l.name, "default": l.default })).collect();
    let mut tasks: Vec<Value> = tasks.into_iter().filter_map(|t| serde_json::to_value(t).ok()).collect();
    // Tasks kept here before signing in stay, in their own list, until they are done.
    let here = local_tasks();
    if !here.is_empty() {
        lists.push(local_list(false));
        tasks.extend(here);
    }
    Ok(json!({
        "account": msgraph::auth::account(),
        "webUrl": msgraph::auth::web_url(),
        "lists": lists,
        "tasks": tasks,
    }))
}

/// Ticks a task off (or back on, with `done: false`, for Undo).
#[tauri::command]
pub async fn todo_set_done(app: AppHandle, list_id: String, task_id: String, done: bool) -> Result<(), String> {
    if list_id == local::LIST_ID {
        local::set_done(&task_id, done)?;
    } else {
        graph::set_status(&list_id, &task_id, done).await?;
    }
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
    // Not signed in, or the local list asked for: kept on this computer.
    if !msgraph::signed_in() || list_id.as_deref() == Some(local::LIST_ID) {
        local::add(title)?;
        poll_now(&app).await;
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
