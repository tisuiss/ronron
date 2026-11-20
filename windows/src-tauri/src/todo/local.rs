//! Tasks kept on this computer, for To Do without a Microsoft 365 account (or beside
//! it): a plain JSON file next to settings.json. Not a secret: task titles only.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The list's id in the data and in todo_add / todo_set_done.
pub const LIST_ID: &str = "local";
/// Done tasks are kept this long (Undo, and a little history), then dropped.
const KEEP_DONE_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const MAX_TASKS: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LocalTask {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub created: i64,
    pub done_at: Option<i64>,
    /// "YYYY-MM-DD", when there is one.
    pub due: Option<String>,
    pub importance: String,
}

static LOCK: Mutex<()> = Mutex::new(());

fn path() -> PathBuf {
    crate::platform::config_dir().join("todo-local.json")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn read_from(file: &std::path::Path) -> Vec<LocalTask> {
    std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_to(file: &std::path::Path, tasks: &[LocalTask]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(tasks).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

/// The tasks still to do.
pub fn open() -> Vec<LocalTask> {
    let _guard = LOCK.lock().unwrap();
    read_from(&path()).into_iter().filter(|t| !t.done).collect()
}

pub fn add(title: &str) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap();
    add_in(&path(), title, now_ms())
}

pub fn set_done(id: &str, done: bool) -> Result<(), String> {
    let _guard = LOCK.lock().unwrap();
    set_done_in(&path(), id, done, now_ms())
}

fn add_in(file: &std::path::Path, title: &str, now: i64) -> Result<(), String> {
    let mut tasks = read_from(file);
    // Done tasks past their time go; the file never grows without end.
    tasks.retain(|t| !t.done || t.done_at.is_some_and(|at| now - at < KEEP_DONE_MS));
    if tasks.len() >= MAX_TASKS {
        return Err(crate::i18n::t("Too many tasks on this computer."));
    }
    tasks.push(LocalTask {
        id: format!("l{now:x}{:x}", tasks.len()),
        title: title.chars().take(255).collect(),
        done: false,
        created: now,
        done_at: None,
        due: None,
        importance: "normal".into(),
    });
    write_to(file, &tasks)
}

fn set_done_in(file: &std::path::Path, id: &str, done: bool, now: i64) -> Result<(), String> {
    let mut tasks = read_from(file);
    let task = tasks.iter_mut().find(|t| t.id == id).ok_or_else(|| crate::i18n::t("Unexpected API response."))?;
    task.done = done;
    task.done_at = done.then_some(now);
    write_to(file, &tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_are_added_ticked_off_and_back() {
        let dir = std::env::temp_dir().join(format!("ronron-todo-{}", std::process::id()));
        let file = dir.join("todo-local.json");
        let _ = std::fs::remove_file(&file);
        add_in(&file, "Acheter du pain", 1_000).unwrap();
        add_in(&file, "Appeler le garage", 2_000).unwrap();
        let tasks = read_from(&file);
        assert_eq!(tasks.len(), 2);
        assert_ne!(tasks[0].id, tasks[1].id);
        set_done_in(&file, &tasks[0].id, true, 3_000).unwrap();
        let open: Vec<_> = read_from(&file).into_iter().filter(|t| !t.done).collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].title, "Appeler le garage");
        set_done_in(&file, &tasks[0].id, false, 4_000).unwrap();
        assert!(read_from(&file).iter().all(|t| !t.done));
        // A done task older than a month goes at the next add.
        set_done_in(&file, &tasks[0].id, true, 5_000).unwrap();
        add_in(&file, "Plus tard", 5_000 + KEEP_DONE_MS + 1).unwrap();
        assert_eq!(read_from(&file).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
