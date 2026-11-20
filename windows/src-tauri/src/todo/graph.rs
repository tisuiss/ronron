//! The few Microsoft Graph calls the To Do pill makes.

use serde::Serialize;
use serde_json::{json, Value};

pub const MAX_LISTS: usize = 25;
pub const MAX_TASKS: usize = 300;
const PAGE: usize = 100;
const MAX_PAGES: usize = 5;

pub struct List {
    pub id: String,
    pub name: String,
    pub default: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub list_id: String,
    pub list: String,
    pub title: String,
    /// "low", "normal" or "high".
    pub importance: String,
    /// The due day, "YYYY-MM-DD", when there is one.
    pub due: Option<String>,
}

fn invalid() -> String {
    crate::i18n::t("Unexpected API response.")
}

fn url(segments: &[&str]) -> Result<reqwest::Url, String> {
    let mut all = vec!["todo", "lists"];
    all.extend_from_slice(segments);
    crate::msgraph::url(&all)
}

/// The user's lists, the default one ("Tasks") first.
pub async fn lists() -> Result<Vec<List>, String> {
    let value = crate::msgraph::get(url(&[])?).await?;
    let mut lists = parse_lists(&value)?;
    lists.sort_by_key(|l| !l.default);
    Ok(lists)
}

fn parse_lists(value: &Value) -> Result<Vec<List>, String> {
    let items = value.get("value").and_then(Value::as_array).ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|l| {
        Some(List {
            id: l.get("id")?.as_str()?.to_string(),
            name: l.get("displayName").and_then(Value::as_str).unwrap_or("").to_string(),
            default: l.get("wellknownListName").and_then(Value::as_str) == Some("defaultList"),
        })
    }).collect())
}

/// The tasks of `list` that are not done yet, following Graph's pages.
pub async fn open_tasks(list: &List) -> Result<Vec<Task>, String> {
    let mut next = url(&[&list.id, "tasks"])?;
    next.query_pairs_mut()
        .append_pair("$filter", "status ne 'completed'")
        .append_pair("$top", &PAGE.to_string());
    let mut tasks = Vec::new();
    for _ in 0..MAX_PAGES {
        let value = crate::msgraph::get(next.clone()).await?;
        tasks.extend(parse_tasks(&value, list)?);
        match crate::msgraph::next_link(&value) {
            Some(link) => next = link,
            None => break,
        }
    }
    Ok(tasks)
}

fn parse_tasks(value: &Value, list: &List) -> Result<Vec<Task>, String> {
    let items = value.get("value").and_then(Value::as_array).ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|t| {
        if t.get("status").and_then(Value::as_str) == Some("completed") {
            return None;
        }
        Some(Task {
            id: t.get("id")?.as_str()?.to_string(),
            list_id: list.id.clone(),
            list: list.name.clone(),
            title: t.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
            importance: t.get("importance").and_then(Value::as_str).unwrap_or("normal").to_string(),
            due: t.get("dueDateTime").and_then(|d| d.get("dateTime")).and_then(Value::as_str).and_then(due_day),
        })
    }).collect())
}

/// To Do keeps a due *day*, written as that day's midnight: keep the day.
fn due_day(date_time: &str) -> Option<String> {
    let day = date_time.get(..10)?;
    let ok = day.len() == 10 && day.chars().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
    ok.then(|| day.to_string())
}

pub async fn set_status(list_id: &str, task_id: &str, done: bool) -> Result<(), String> {
    let status = if done { "completed" } else { "notStarted" };
    let url = url(&[list_id, "tasks", task_id])?;
    crate::msgraph::call(|h, t| h.patch(url).bearer_auth(t).json(&json!({ "status": status }))).await?;
    Ok(())
}

pub async fn add(list_id: &str, title: &str) -> Result<(), String> {
    let title: String = title.chars().take(255).collect();
    let url = url(&[list_id, "tasks"])?;
    crate::msgraph::call(|h, t| h.post(url).bearer_auth(t).json(&json!({ "title": title }))).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_path_segments() {
        let u = url(&["AAMk/=?#x", "tasks", "id 1"]).unwrap();
        assert!(u.as_str().starts_with("https://graph.microsoft.com/v1.0/me/todo/lists/AAMk%2F=%3F%23x/tasks/id%201"), "{u}");
        assert!(u.query().is_none());
    }

    #[test]
    fn lists_and_tasks_are_read() {
        let lists = parse_lists(&json!({ "value": [
            { "id": "L2", "displayName": "Courses", "wellknownListName": "none" },
            { "id": "L1", "displayName": "Tâches", "wellknownListName": "defaultList" },
            { "displayName": "no id" },
        ] })).unwrap();
        assert_eq!(lists.len(), 2);
        assert!(lists[1].default && !lists[0].default);

        let tasks = parse_tasks(&json!({ "value": [
            { "id": "t1", "title": "Relancer devis", "importance": "high", "status": "notStarted",
              "dueDateTime": { "dateTime": "2026-11-21T00:00:00.0000000", "timeZone": "UTC" } },
            { "id": "t2", "title": "Done already", "status": "completed" },
            { "id": "t3", "title": "Sans date", "status": "inProgress" },
        ] }), &lists[1]).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].due.as_deref(), Some("2026-11-21"));
        assert_eq!(tasks[0].list, "Tâches");
        assert_eq!(tasks[0].importance, "high");
        assert_eq!(tasks[1].due, None);
        assert_eq!(tasks[1].importance, "normal");
        assert!(parse_tasks(&json!({}), &lists[0]).is_err());
    }

    #[test]
    fn due_days_are_checked() {
        assert_eq!(due_day("2026-01-05T00:00:00").as_deref(), Some("2026-01-05"));
        assert_eq!(due_day("2026-1-5"), None);
        assert_eq!(due_day("garbage-garbage"), None);
    }
}
