//! Google Tasks for the To Do tab: the lists, the tasks not done yet, ticking one
//! off (and back on, for Undo), and adding one.
//!
//! Their list ids go to the island with a `g:` in front, so todo/mod.rs knows
//! which account a click is for.

use serde_json::{json, Value};

use super::{get, patch, post, url, TASKS};

pub const PREFIX: &str = "g:";
const MAX_LISTS: usize = 25;
const MAX_TASKS: usize = 300;

fn invalid() -> String {
    crate::i18n::t("Unexpected API response.")
}

pub struct List {
    pub id: String,
    pub name: String,
}

/// The account's lists, its default one ("My Tasks") first, as Google gives them.
pub async fn lists() -> Result<Vec<List>, String> {
    let mut u = url(TASKS, &["users", "@me", "lists"])?;
    u.query_pairs_mut().append_pair("maxResults", "100");
    parse_lists(&get(u).await?)
}

fn parse_lists(value: &Value) -> Result<Vec<List>, String> {
    let items = value.get("items").and_then(Value::as_array).ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|l| Some(List {
        id: l.get("id")?.as_str()?.to_string(),
        name: l.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
    })).take(MAX_LISTS).collect())
}

/// The tasks of `list` that are not done yet, in the To Do tab's shape.
pub async fn open_tasks(list: &List) -> Result<Vec<Value>, String> {
    let mut u = url(TASKS, &["lists", &list.id, "tasks"])?;
    u.query_pairs_mut()
        .append_pair("showCompleted", "false")
        .append_pair("showHidden", "false")
        .append_pair("maxResults", "100");
    parse_tasks(&get(u).await?, list)
}

pub fn parse_tasks(value: &Value, list: &List) -> Result<Vec<Value>, String> {
    // A list with no task comes back without "items".
    let Some(items) = value.get("items") else { return Ok(Vec::new()) };
    let items = items.as_array().ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|t| {
        if t.get("status").and_then(Value::as_str) == Some("completed") || t.get("deleted").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let title = t.get("title").and_then(Value::as_str).unwrap_or("").trim();
        if title.is_empty() {
            return None;
        }
        // "2026-11-20T00:00:00.000Z": Google keeps only the day.
        let due = t.get("due").and_then(Value::as_str).and_then(|d| d.get(..10)).map(String::from);
        Some(json!({
            "id": t.get("id")?.as_str()?,
            "listId": format!("{PREFIX}{}", list.id),
            "list": list.name,
            "title": title,
            "importance": "normal",
            "due": due,
        }))
    }).take(MAX_TASKS).collect())
}

/// Ticks a task off, or back on with `done: false`.
pub async fn set_done(list: &str, task: &str, done: bool) -> Result<(), String> {
    let body = if done { json!({ "status": "completed" }) } else { json!({ "status": "needsAction", "completed": null }) };
    patch(url(TASKS, &["lists", list, "tasks", task])?, body).await.map(|_| ())
}

/// Adds a task to `list` ("@default": the account's default list).
pub async fn add(list: &str, title: &str) -> Result<(), String> {
    post(url(TASKS, &["lists", list, "tasks"])?, json!({ "title": title })).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_are_read_for_the_todo_tab() {
        let list = List { id: "MDE2".into(), name: "My Tasks".into() };
        let tasks = parse_tasks(&json!({ "items": [
            { "id": "t1", "title": "Rappeler Anne", "status": "needsAction", "due": "2026-11-20T00:00:00.000Z" },
            { "id": "t2", "title": "Fait", "status": "completed" },
            { "id": "t3", "title": "   ", "status": "needsAction" },
            { "id": "t4", "title": "Sans date", "status": "needsAction" },
        ] }), &list).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["listId"], "g:MDE2");
        assert_eq!(tasks[0]["due"], "2026-11-20");
        assert_eq!(tasks[1]["due"], Value::Null);
        assert!(parse_tasks(&json!({}), &list).unwrap().is_empty());
        assert_eq!(parse_lists(&json!({ "items": [{ "id": "MDE2", "title": "My Tasks" }] })).unwrap()[0].name, "My Tasks");
    }
}
