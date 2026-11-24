//! Clockify: the running timer, the last days' time entries and the projects,
//! with the user's own API key (Clockify → Profile settings → API), kept in the
//! credential store. Read every 30 s while the pill is on; a timer is started
//! or stopped only from a click or « OK Ronron » (`clockify_start`,
//! `clockify_stop`, the voice's `timer` tool) — the island's suggestions are
//! buttons, never actions of their own.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::integrations::{self, IntegrationUpdate};
use crate::whatsapp::gateway::{iso_utc, rfc3339_ms};

const ID: &str = "integration_clockify";
pub const KEY: &str = "clockify-api-key";
/// Another API address (Clockify's regional servers, a self-hosted one); empty is the global one.
pub const KEY_URL: &str = "clockify-url";
const DEFAULT_BASE: &str = "https://api.clockify.me/api/v1";
/// The entries read: the week whatever day it is, and a little more.
const WINDOW_MS: i64 = 8 * 86_400_000;
const MAX_ENTRIES: usize = 500;
const PROJECTS_EVERY: Duration = Duration::from_secs(600);
const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_BODY: usize = 8 * 1024 * 1024;

/// What the last poll learnt: who the user is, their workspace, the projects
/// (read every 10 minutes) and the data the island got (for the voice).
#[derive(Default)]
struct Cache {
    user: String,
    workspace: String,
    projects: Vec<Value>,
    projects_at: Option<Instant>,
    /// Each project's active tasks, read when the project is picked, for 10 minutes.
    tasks: std::collections::HashMap<String, (Instant, Vec<Value>)>,
    data: Value,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn active(app: &AppHandle) -> bool {
    !integrations::PAUSED.load(std::sync::atomic::Ordering::Relaxed) && integrations::enabled(app, ID)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The API's address: the one stored, if it is an https one, else the global one.
fn base() -> String {
    crate::secrets::get(KEY_URL)
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| u.starts_with("https://") && reqwest::Url::parse(u).is_ok())
        .unwrap_or_else(|| DEFAULT_BASE.into())
}

struct Api {
    http: reqwest::Client,
    base: String,
    key: String,
}

impl Api {
    fn new() -> Result<Self, String> {
        let key = crate::secrets::get(KEY).filter(|k| !k.trim().is_empty()).ok_or_else(|| crate::i18n::t("Key not configured"))?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .user_agent("Ronron")
            .build()
            .map_err(|_| crate::i18n::t("Not available"))?;
        Ok(Self { http, base: base(), key: key.trim().to_string() })
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Value, String> {
        let mut response = request.header("X-Api-Key", &self.key).send().await.map_err(|_| crate::i18n::t("Not available"))?;
        let status = response.status().as_u16();
        if status == 401 || status == 403 {
            return Err(crate::i18n::t("Clockify refused the API key."));
        }
        if !response.status().is_success() {
            return Err(crate::i18n::tf("API error {code}", &[("code", &status.to_string())]));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| crate::i18n::t("Unexpected API response."))? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err(crate::i18n::t("The server's answer is too large."));
            }
            body.extend_from_slice(&chunk);
        }
        if body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&body).map_err(|_| crate::i18n::t("Unexpected API response."))
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        let mut url = reqwest::Url::parse(&format!("{}{path}", self.base)).map_err(|e| e.to_string())?;
        for (k, v) in query {
            url.query_pairs_mut().append_pair(k, v);
        }
        self.send(self.http.get(url)).await
    }

    /// The user's id and active workspace, from the cache or the API.
    async fn who(&self) -> Result<(String, String), String> {
        if let Some(c) = CACHE.lock().unwrap().as_ref().filter(|c| !c.user.is_empty()) {
            return Ok((c.user.clone(), c.workspace.clone()));
        }
        let me = self.get("/user", &[]).await?;
        let user = me["id"].as_str().unwrap_or("").to_string();
        let workspace = me["activeWorkspace"].as_str().or_else(|| me["defaultWorkspace"].as_str()).unwrap_or("").to_string();
        if user.is_empty() || workspace.is_empty() {
            return Err(crate::i18n::t("Unexpected API response."));
        }
        let mut cache = CACHE.lock().unwrap();
        let c = cache.get_or_insert_with(Cache::default);
        c.user = user.clone();
        c.workspace = workspace.clone();
        Ok((user, workspace))
    }
}

pub async fn poll(app: AppHandle) {
    if !active(&app) {
        return;
    }
    let result = fetch().await;
    if !active(&app) {
        return;
    }
    match result {
        Ok(data) => {
            if let Some(c) = CACHE.lock().unwrap().as_mut() {
                c.data = data.clone();
            }
            integrations::emit(&app, IntegrationUpdate { id: ID, data, error: None, event: None });
        }
        Err(error) => {
            // A key changed or refused: ask who the user is again next time.
            *CACHE.lock().unwrap() = None;
            integrations::emit(&app, IntegrationUpdate { id: ID, data: json!({}), error: Some(error), event: None });
        }
    }
}

async fn fetch() -> Result<Value, String> {
    let api = Api::new()?;
    let (user, ws) = api.who().await?;
    let now = now_ms();
    let raw = api
        .get(
            &format!("/workspaces/{ws}/user/{user}/time-entries"),
            &[("start", iso_utc(now - WINDOW_MS)), ("hydrated", "true".into()), ("page-size", MAX_ENTRIES.to_string())],
        )
        .await?;
    let entries = parse_entries(&raw)?;
    let stale = CACHE.lock().unwrap().as_ref().is_none_or(|c| c.projects_at.is_none_or(|at| at.elapsed() > PROJECTS_EVERY));
    if stale {
        let raw = api.get(&format!("/workspaces/{ws}/projects"), &[("archived", "false".into()), ("page-size", "500".into())]).await?;
        let projects = parse_projects(&raw);
        let mut cache = CACHE.lock().unwrap();
        let c = cache.get_or_insert_with(Cache::default);
        c.projects = projects;
        c.projects_at = Some(Instant::now());
    }
    let projects = CACHE.lock().unwrap().as_ref().map(|c| c.projects.clone()).unwrap_or_default();
    let running = entries.iter().find(|e| e["end"].is_null()).cloned().unwrap_or(Value::Null);
    Ok(json!({ "running": running, "entries": entries, "projects": projects }))
}

/// GET …/time-entries?hydrated=true → the entries, newest first, as the island reads them.
fn parse_entries(raw: &Value) -> Result<Vec<Value>, String> {
    let list = raw.as_array().ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    let mut out: Vec<Value> = list
        .iter()
        .filter_map(|e| {
            let start = rfc3339_ms(e["timeInterval"]["start"].as_str())?;
            let end = rfc3339_ms(e["timeInterval"]["end"].as_str());
            Some(json!({
                "id": e["id"].as_str()?,
                "description": e["description"].as_str().unwrap_or(""),
                "projectId": e["projectId"].as_str().or_else(|| e["project"]["id"].as_str()),
                "projectName": e["project"]["name"].as_str().unwrap_or(""),
                "projectColor": e["project"]["color"].as_str().unwrap_or(""),
                "taskId": e["taskId"].as_str().or_else(|| e["task"]["id"].as_str()),
                "taskName": e["task"]["name"].as_str().unwrap_or(""),
                "start": start,
                "end": end,
            }))
        })
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e["start"].as_i64().unwrap_or(0)));
    Ok(out)
}

fn parse_projects(raw: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = raw
        .as_array()
        .map(|l| l.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|p| p["archived"] != true)
        .filter_map(|p| {
            Some(json!({
                "id": p["id"].as_str()?,
                "name": p["name"].as_str().unwrap_or(""),
                "color": p["color"].as_str().unwrap_or(""),
                "client": p["clientName"].as_str().unwrap_or(""),
            }))
        })
        .collect();
    out.sort_by_key(|p| p["name"].as_str().unwrap_or("").to_lowercase());
    out
}

// ── Starting and stopping ─────────────────────────────────────────────────────

/// Starts a timer now (Clockify stops the one running, if any).
pub async fn start(description: &str, project: Option<&str>, task: Option<&str>) -> Result<(), String> {
    let api = Api::new()?;
    let (_, ws) = api.who().await?;
    let mut body = json!({ "start": iso_utc(now_ms()), "description": description.trim() });
    if let Some(p) = project.map(str::trim).filter(|p| !p.is_empty()) {
        body["projectId"] = json!(p);
        // A task only goes with its project.
        if let Some(t) = task.map(str::trim).filter(|t| !t.is_empty()) {
            body["taskId"] = json!(t);
        }
    }
    let url = format!("{}/workspaces/{ws}/time-entries", api.base);
    api.send(api.http.post(url).json(&body)).await.map(|_| ())
}

/// Stops the running timer now.
pub async fn stop() -> Result<(), String> {
    let api = Api::new()?;
    let (user, ws) = api.who().await?;
    let url = format!("{}/workspaces/{ws}/user/{user}/time-entries", api.base);
    api.send(api.http.patch(url).json(&json!({ "end": iso_utc(now_ms()) }))).await.map(|_| ())
}

#[tauri::command]
pub async fn clockify_start(
    app: AppHandle,
    description: String,
    project_id: Option<String>,
    task_id: Option<String>,
) -> Result<(), String> {
    start(&description, project_id.as_deref(), task_id.as_deref()).await?;
    poll(app).await;
    Ok(())
}

/// A project's active tasks (id and name, sorted), for the card's task list.
#[tauri::command]
pub async fn clockify_tasks(project_id: String) -> Result<Vec<Value>, String> {
    let project = project_id.trim().to_string();
    if project.is_empty() || !project.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Ok(Vec::new());
    }
    let cached = CACHE
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| c.tasks.get(&project).filter(|(at, _)| at.elapsed() < PROJECTS_EVERY).map(|(_, t)| t.clone()));
    if let Some(tasks) = cached {
        return Ok(tasks);
    }
    let api = Api::new()?;
    let (_, ws) = api.who().await?;
    let raw = api
        .get(&format!("/workspaces/{ws}/projects/{project}/tasks"), &[("is-active", "true".into()), ("page-size", "500".into())])
        .await?;
    let tasks = parse_tasks(&raw);
    let mut cache = CACHE.lock().unwrap();
    cache.get_or_insert_with(Cache::default).tasks.insert(project, (Instant::now(), tasks.clone()));
    Ok(tasks)
}

fn parse_tasks(raw: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = raw
        .as_array()
        .map(|l| l.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|t| t["status"].as_str() != Some("DONE"))
        .filter_map(|t| Some(json!({ "id": t["id"].as_str()?, "name": t["name"].as_str().unwrap_or("") })))
        .collect();
    out.sort_by_key(|t| t["name"].as_str().unwrap_or("").to_lowercase());
    out
}

#[tauri::command]
pub async fn clockify_stop(app: AppHandle) -> Result<(), String> {
    stop().await?;
    poll(app).await;
    Ok(())
}

// ── « OK Ronron » ─────────────────────────────────────────────────────────────

/// The project whose name best matches what was said: the same name, else one
/// that contains it, else one it contains.
pub fn find_project<'a>(projects: &'a [Value], said: &str) -> Option<&'a Value> {
    let said = said.trim().to_lowercase();
    if said.is_empty() {
        return None;
    }
    let name = |p: &Value| p["name"].as_str().unwrap_or("").to_lowercase();
    projects
        .iter()
        .find(|p| name(p) == said)
        .or_else(|| projects.iter().find(|p| name(p).contains(&said)))
        .or_else(|| projects.iter().find(|p| !name(p).is_empty() && said.contains(&name(p))))
}

/// The voice's `timer` tool: start, stop or status. What goes back to the model, and whether something changed.
pub fn voice(app: &AppHandle, input: &Value) -> (String, bool) {
    if crate::secrets::get(KEY).is_none() {
        return ("Clockify is not set up: the user adds their API key in Settings → Integrations → Clockify.".into(), false);
    }
    match input["action"].as_str().unwrap_or("status") {
        "start" => {
            let description = input["description"].as_str().unwrap_or("").to_string();
            let said = input["project"].as_str().unwrap_or("").to_string();
            let projects = CACHE.lock().unwrap().as_ref().map(|c| c.projects.clone()).unwrap_or_default();
            let project = find_project(&projects, &said).cloned();
            if !said.trim().is_empty() && project.is_none() {
                let names: Vec<&str> = projects.iter().filter_map(|p| p["name"].as_str()).take(30).collect();
                return (format!("No project named {said:?}. The projects: {}", names.join(", ")), false);
            }
            let id = project.as_ref().and_then(|p| p["id"].as_str()).map(str::to_string);
            let done = tauri::async_runtime::block_on(async {
                let r = start(&description, id.as_deref(), None).await;
                poll(app.clone()).await;
                r
            });
            match done {
                Ok(()) => (
                    format!("Timer started{}.", project.and_then(|p| p["name"].as_str().map(|n| format!(" on {n}"))).unwrap_or_default()),
                    true,
                ),
                Err(e) => (format!("Could not start the timer: {e}"), false),
            }
        }
        "stop" => match tauri::async_runtime::block_on(async {
            let r = stop().await;
            poll(app.clone()).await;
            r
        }) {
            Ok(()) => ("Timer stopped.".into(), true),
            Err(e) => (format!("Could not stop the timer: {e}"), false),
        },
        _ => {
            let data = CACHE.lock().unwrap().as_ref().map(|c| c.data.clone()).unwrap_or(Value::Null);
            (summary(&data, now_ms()), false)
        }
    }
}

/// What the voice reads: the running timer and the hours of the last days.
fn summary(data: &Value, now: i64) -> String {
    let empty = Vec::new();
    let entries = data["entries"].as_array().unwrap_or(&empty);
    let running = &data["running"];
    let mut out = String::new();
    if running.is_null() {
        out.push_str("No timer is running. ");
    } else {
        let mins = (now - running["start"].as_i64().unwrap_or(now)) / 60_000;
        out.push_str(&format!(
            "Running: {:?} on {:?} for {} h {} min. ",
            running["description"].as_str().unwrap_or(""),
            running["projectName"].as_str().unwrap_or("no project"),
            mins / 60,
            mins % 60
        ));
    }
    out.push_str("Entries of the last 8 days (start in UTC, minutes, project, description): ");
    let lines: Vec<String> = entries
        .iter()
        .take(60)
        .map(|e| {
            let start = e["start"].as_i64().unwrap_or(0);
            let end = e["end"].as_i64().unwrap_or(now);
            format!(
                "{} {}min {} {:?}",
                iso_utc(start),
                (end - start) / 60_000,
                e["projectName"].as_str().unwrap_or("-"),
                e["description"].as_str().unwrap_or("")
            )
        })
        .collect();
    out.push_str(&lines.join("; "));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_keep_their_project_and_the_running_one_has_no_end() {
        let raw = json!([
            { "id": "a", "description": "Mails", "projectId": "p1", "project": { "name": "Admin", "color": "#03A9F4" },
              "timeInterval": { "start": "2026-11-24T08:00:00Z", "end": "2026-11-24T09:30:00Z" } },
            { "id": "b", "description": "Ronron", "projectId": null,
              "timeInterval": { "start": "2026-11-24T10:00:00Z", "end": null } },
            { "id": "bad", "timeInterval": { "start": "yesterday" } }
        ]);
        let entries = parse_entries(&raw).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["id"], "b");
        assert!(entries[0]["end"].is_null());
        assert_eq!(entries[1]["projectName"], "Admin");
        assert_eq!(entries[1]["end"].as_i64().unwrap() - entries[1]["start"].as_i64().unwrap(), 90 * 60_000);
        assert!(parse_entries(&json!({})).is_err());
    }

    #[test]
    fn projects_are_sorted_and_archived_ones_left_out() {
        let raw = json!([
            { "id": "2", "name": "zeta", "color": "#fff", "clientName": "ACME" },
            { "id": "1", "name": "Alpha", "color": "#000" },
            { "id": "3", "name": "Old", "archived": true }
        ]);
        let p = parse_projects(&raw);
        assert_eq!(p.iter().map(|p| p["id"].as_str().unwrap()).collect::<Vec<_>>(), ["1", "2"]);
        assert_eq!(p[1]["client"], "ACME");
    }

    #[test]
    fn tasks_are_sorted_and_done_ones_left_out() {
        let raw = json!([
            { "id": "t2", "name": "80244-270-Ceremonies", "status": "ACTIVE" },
            { "id": "t1", "name": "149-0-004", "status": "ACTIVE" },
            { "id": "t3", "name": "Old", "status": "DONE" }
        ]);
        let t = parse_tasks(&raw);
        assert_eq!(t.iter().map(|t| t["id"].as_str().unwrap()).collect::<Vec<_>>(), ["t1", "t2"]);
        assert!(parse_tasks(&json!({})).is_empty());
    }

    #[test]
    fn a_said_project_is_found_by_its_name() {
        let p = vec![json!({ "id": "1", "name": "Ronron" }), json!({ "id": "2", "name": "Site web client" })];
        assert_eq!(find_project(&p, "ronron").unwrap()["id"], "1");
        assert_eq!(find_project(&p, "site web").unwrap()["id"], "2");
        assert_eq!(find_project(&p, "le projet Ronron").unwrap()["id"], "1");
        assert!(find_project(&p, "compta").is_none());
        assert!(find_project(&p, " ").is_none());
    }

    #[test]
    fn the_voice_summary_says_what_runs() {
        let data = json!({ "running": { "description": "Code", "projectName": "Ronron", "start": 0 }, "entries": [
            { "start": 0, "end": null, "projectName": "Ronron", "description": "Code" }
        ] });
        let s = summary(&data, 90 * 60_000);
        assert!(s.contains("Running: \"Code\" on \"Ronron\" for 1 h 30 min"));
        assert!(s.contains("90min Ronron"));
        assert!(summary(&json!({}), 0).starts_with("No timer is running."));
    }
}
