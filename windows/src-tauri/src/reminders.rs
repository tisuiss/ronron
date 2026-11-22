//! Reminders kept on this computer: a text and a time, in a plain JSON file next to
//! settings.json (no account, works offline). The Reminders tab adds and removes
//! them, « OK Ronron » adds them by voice, and the island opens on each one when
//! its time comes (src/island/alarms.ts): Done removes it, a snooze moves it on.
//!
//! Every change is sent to the island as a `reminders` event with the whole list.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const MAX_REMINDERS: usize = 200;
const MAX_TEXT: usize = 300;
/// A snooze, at most a day.
const MAX_SNOOZE_MIN: i64 = 24 * 60;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Reminder {
    pub id: String,
    pub text: String,
    /// When it rings, milliseconds since 1970 (UTC).
    pub at: i64,
    pub created: i64,
}

static LOCK: Mutex<()> = Mutex::new(());

fn path() -> PathBuf {
    crate::platform::config_dir().join("reminders.json")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn read_from(file: &Path) -> Vec<Reminder> {
    let mut list: Vec<Reminder> = std::fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    list.sort_by_key(|r| r.at);
    list
}

fn write_to(file: &Path, list: &[Reminder]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| e.to_string())
}

/// The text as kept: one line, trimmed, not too long.
pub fn clean_text(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(MAX_TEXT).collect()
}

pub fn add_in(file: &Path, text: &str, at: i64, now: i64) -> Result<Reminder, String> {
    let text = clean_text(text);
    if text.is_empty() {
        return Err(crate::i18n::t("Write what to remind you of."));
    }
    let mut list = read_from(file);
    if list.len() >= MAX_REMINDERS {
        return Err(crate::i18n::t("Too many reminders: remove some first."));
    }
    let r = Reminder { id: format!("r{now:x}{:x}", list.len()), text, at: at.max(now), created: now };
    list.push(r.clone());
    list.sort_by_key(|r| r.at);
    write_to(file, &list)?;
    Ok(r)
}

pub fn remove_in(file: &Path, id: &str) -> Result<(), String> {
    let mut list = read_from(file);
    list.retain(|r| r.id != id);
    write_to(file, &list)
}

pub fn snooze_in(file: &Path, id: &str, minutes: i64, now: i64) -> Result<(), String> {
    let mut list = read_from(file);
    let r = list.iter_mut().find(|r| r.id == id).ok_or_else(|| crate::i18n::t("This reminder is gone."))?;
    r.at = now + minutes.clamp(1, MAX_SNOOZE_MIN) * 60_000;
    list.sort_by_key(|r| r.at);
    write_to(file, &list)
}

/// "2026-11-22T15:00" (or with seconds, or a space) in this computer's time → ms since 1970.
pub fn parse_local(text: &str) -> Option<i64> {
    use chrono::{Local, NaiveDateTime, TimeZone};
    let text = text.trim().replace(' ', "T");
    let naive = NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M"))
        .ok()?;
    // A time that does not exist (clocks going forward) takes the hour after.
    let local = Local.from_local_datetime(&naive).earliest()
        .or_else(|| Local.from_local_datetime(&(naive + chrono::Duration::hours(1))).earliest())?;
    Some(local.timestamp_millis())
}

/// The list, soonest first.
pub fn list() -> Vec<Reminder> {
    let _guard = LOCK.lock().unwrap();
    read_from(&path())
}

fn changed(app: &AppHandle) {
    let _ = app.emit_to(crate::island::WINDOW_LABEL, "reminders", list());
}

/// Adds a reminder (the tab, or « OK Ronron ») and tells the island.
pub fn add(app: &AppHandle, text: &str, at: i64) -> Result<Reminder, String> {
    let r = {
        let _guard = LOCK.lock().unwrap();
        add_in(&path(), text, at, now_ms())?
    };
    changed(app);
    Ok(r)
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn reminders_list() -> Vec<Reminder> {
    list()
}

#[tauri::command]
pub fn reminders_add(app: AppHandle, text: String, at: i64) -> Result<Reminder, String> {
    add(&app, &text, at)
}

/// Done, or removed from the tab.
#[tauri::command]
pub fn reminders_remove(app: AppHandle, id: String) -> Result<(), String> {
    {
        let _guard = LOCK.lock().unwrap();
        remove_in(&path(), &id)?;
    }
    changed(&app);
    Ok(())
}

/// Rings again in `minutes`.
#[tauri::command]
pub fn reminders_snooze(app: AppHandle, id: String, minutes: i64) -> Result<(), String> {
    {
        let _guard = LOCK.lock().unwrap();
        snooze_in(&path(), &id, minutes, now_ms())?;
    }
    changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ronron-reminders-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("reminders.json")
    }

    #[test]
    fn reminders_are_kept_soonest_first_and_removed() {
        let file = scratch("keep");
        let later = add_in(&file, "Appeler Anne", 5_000, 1_000).unwrap();
        let sooner = add_in(&file, "  Sortir\n le chien  ", 2_000, 1_001).unwrap();
        assert_eq!(sooner.text, "Sortir le chien");
        let list = read_from(&file);
        assert_eq!(list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec![sooner.id.as_str(), later.id.as_str()]);
        assert_ne!(sooner.id, later.id);
        remove_in(&file, &sooner.id).unwrap();
        assert_eq!(read_from(&file), vec![later]);
        assert!(add_in(&file, "   ", 9_000, 1_002).is_err(), "nothing to remind");
        // A time already past rings at once.
        assert_eq!(add_in(&file, "Tout de suite", 10, 3_000).unwrap().at, 3_000);
    }

    #[test]
    fn a_snooze_moves_it_on_within_a_day() {
        let file = scratch("snooze");
        let r = add_in(&file, "Thé", 1_000, 1_000).unwrap();
        snooze_in(&file, &r.id, 10, 50_000).unwrap();
        assert_eq!(read_from(&file)[0].at, 50_000 + 600_000);
        snooze_in(&file, &r.id, 100_000, 0).unwrap();
        assert_eq!(read_from(&file)[0].at, MAX_SNOOZE_MIN * 60_000);
        assert!(snooze_in(&file, "gone", 10, 0).is_err());
    }

    #[test]
    fn local_times_are_read() {
        let a = parse_local("2026-11-22T15:00").unwrap();
        assert_eq!(parse_local("2026-11-22 15:00:00"), Some(a));
        assert_eq!(parse_local("2026-11-22T16:00").unwrap() - a, 3_600_000);
        assert_eq!(parse_local("demain 9h"), None);
        assert_eq!(parse_local(""), None);
    }

    #[test]
    fn long_text_is_cut() {
        assert_eq!(clean_text(&"a".repeat(1000)).len(), MAX_TEXT);
    }
}
