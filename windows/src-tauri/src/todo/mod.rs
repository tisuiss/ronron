//! Microsoft To Do through Microsoft Graph (sign-in: crate::msgraph), and Google
//! Tasks (crate::google::tasks) beside it when a Google account is signed in.
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
    let result = if msgraph::signed_in() || crate::google::signed_in() {
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
    let mut lists: Vec<Value> = Vec::new();
    let mut tasks: Vec<Value> = Vec::new();
    if msgraph::signed_in() {
        let ms = graph::lists().await?;
        let mut found = Vec::new();
        for list in ms.iter().take(graph::MAX_LISTS) {
            found.extend(graph::open_tasks(list).await?);
            if found.len() >= graph::MAX_TASKS {
                break;
            }
        }
        found.truncate(graph::MAX_TASKS);
        lists.extend(ms.iter().map(|l| json!({ "id": l.id, "name": l.name, "default": l.default })));
        tasks.extend(found.into_iter().filter_map(|t| serde_json::to_value(t).ok()));
    }
    if crate::google::signed_in() {
        let google = crate::google::tasks::lists().await?;
        // The account's first list is the default one, unless Microsoft's already is.
        let first_default = !msgraph::signed_in();
        for (i, list) in google.iter().enumerate() {
            lists.push(json!({ "id": format!("{}{}", crate::google::tasks::PREFIX, list.id), "name": list.name, "default": first_default && i == 0 }));
            tasks.extend(crate::google::tasks::open_tasks(list).await?);
        }
    }
    // Tasks kept here before signing in stay, in their own list, until they are done.
    let here = local_tasks();
    if !here.is_empty() {
        lists.push(local_list(false));
        tasks.extend(here);
    }
    let (account, web) = if msgraph::signed_in() {
        (msgraph::auth::account(), msgraph::auth::web_url())
    } else {
        (crate::google::auth::account(), "https://tasks.google.com/")
    };
    Ok(json!({
        "account": account,
        "webUrl": web,
        "lists": lists,
        "tasks": tasks,
    }))
}

/// Ticks a task off (or back on, with `done: false`, for Undo).
#[tauri::command]
pub async fn todo_set_done(app: AppHandle, list_id: String, task_id: String, done: bool) -> Result<(), String> {
    if list_id == local::LIST_ID {
        local::set_done(&task_id, done)?;
    } else if let Some(google) = list_id.strip_prefix(crate::google::tasks::PREFIX) {
        crate::google::tasks::set_done(google, &task_id, done).await?;
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
    let google = crate::google::signed_in();
    if (!msgraph::signed_in() && !google) || list_id.as_deref() == Some(local::LIST_ID) {
        local::add(title)?;
        poll_now(&app).await;
        return Ok(());
    }
    // A Google list, or the default list when only Google is signed in.
    let google_list = match list_id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => id.strip_prefix(crate::google::tasks::PREFIX).map(String::from),
        None if !msgraph::signed_in() => Some("@default".to_string()),
        None => None,
    };
    if let Some(list) = google_list {
        crate::google::tasks::add(&list, title).await?;
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
