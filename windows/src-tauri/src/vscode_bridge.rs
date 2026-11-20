//! The chat with Claude Code in a remote VS Code workspace (Remote Tunnels, SSH,
//! containers), through the "Ronron for VS Code" extensions (vscode/ in the repo).
//!
//! Each VS Code window connected to a remote leaves a file in `<local_dir>/vscode/`:
//! its local pipe (a named pipe on Windows, a Unix socket on Linux) and a token only
//! Ronron can read. Ronron asks a window for its folders (the models of the "VS Code"
//! provider) and to run `claude -p` in one of them; the answer comes back through
//! VS Code's own connection. Nothing listens on the network.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::chat::{Chat, ChatReply, ModelInfo};
use crate::i18n::t;

pub const PROVIDER: &str = "vscode";
const ASK_FOLDERS: Duration = Duration::from_millis(1500);
const ASK_CHAT: Duration = Duration::from_secs(320);

#[derive(Deserialize, Clone, Debug, PartialEq)]
struct Window {
    pid: u32,
    pipe: String,
    token: String,
    #[serde(default)]
    remote: String,
    #[serde(default)]
    window: String,
}

/// The Claude Code session of each folder, to carry the conversation on: "<window>|<folder>" → id.
static SESSIONS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn dir() -> PathBuf {
    crate::settings::local_dir().join("vscode")
}

fn windows() -> Vec<(String, Window)> {
    let Ok(entries) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        if let Some(w) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Window>(&b).ok()) {
            if valid_pipe(&w.pipe) {
                out.push((stem, w));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Only the extension's own pipes: nothing else on this computer is ever opened.
fn valid_pipe(pipe: &str) -> bool {
    let name = pipe.rsplit(['\\', '/']).next().unwrap_or("");
    let ok_name = name.starts_with("ronron-vscode-")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'));
    if cfg!(windows) {
        ok_name && pipe.starts_with(r"\\.\pipe\")
    } else {
        ok_name && pipe.starts_with('/') && pipe.ends_with(".sock")
    }
}

/// One request, one answer line, within `limit`.
async fn ask(w: &Window, request: Value, limit: Duration) -> Result<Value, String> {
    let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    line.push('\n');
    let run = async {
        #[cfg(windows)]
        let stream = tokio::net::windows::named_pipe::ClientOptions::new().open(&w.pipe).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        let stream = tokio::net::UnixStream::connect(&w.pipe).await.map_err(|e| e.to_string())?;
        let (read, mut write) = tokio::io::split(stream);
        write.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        let mut reply = String::new();
        BufReader::new(read).read_line(&mut reply).await.map_err(|e| e.to_string())?;
        serde_json::from_str::<Value>(reply.trim()).map_err(|e| e.to_string())
    };
    tokio::time::timeout(limit, run).await.map_err(|_| t("VS Code took too long to answer."))?
}

/// "folder name · remote" for the picker.
fn label(folder_name: &str, w: &Window) -> String {
    let remote = w.remote.split('+').next_back().unwrap_or(&w.remote);
    match (folder_name.is_empty(), remote.is_empty()) {
        (false, false) => format!("{folder_name} · {remote}"),
        (false, true) => folder_name.to_string(),
        _ => w.window.clone(),
    }
}

/// The open remote workspaces' folders, as the "models" of the VS Code provider.
/// A window that does not answer has closed: its file goes.
pub async fn models() -> Result<Vec<ModelInfo>, String> {
    let mut out = Vec::new();
    for (stem, w) in windows() {
        match ask(&w, json!({ "token": w.token, "type": "folders" }), ASK_FOLDERS).await {
            Ok(reply) if reply["ok"] == true => {
                for f in reply["folders"].as_array().into_iter().flatten() {
                    let (Some(path), name) = (f["path"].as_str(), f["name"].as_str().unwrap_or("")) else { continue };
                    out.push(ModelInfo { id: format!("{stem}|{path}"), label: label(name, &w) });
                }
            }
            Ok(_) => {}
            Err(_) => {
                if !pid_alive(w.pid) {
                    let _ = std::fs::remove_file(dir().join(format!("{stem}.json")));
                }
            }
        }
    }
    if out.is_empty() {
        return Err(t("No remote VS Code window with Ronron for VS Code. Open your workspace in VS Code (Remote Tunnels, SSH), with both Ronron extensions installed."));
    }
    Ok(out)
}

/// Whether any remote VS Code window has registered, for showing the provider at all.
#[tauri::command]
pub fn vscode_available() -> bool {
    !windows().is_empty()
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        unsafe {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(handle) => {
                    let _ = CloseHandle(handle);
                    true
                }
                Err(_) => false,
            }
        }
    }
}

/// One chat turn: `claude -p` in the chosen folder, carrying on its session after the first turn.
pub async fn send(chat: &Chat, model: &str, query: String) -> Result<ChatReply, String> {
    let (stem, folder) = model.split_once('|').ok_or_else(|| t("Pick a VS Code workspace in the model list."))?;
    let w = windows().into_iter().find(|(s, _)| s == stem).map(|(_, w)| w)
        .ok_or_else(|| t("This VS Code window is closed. Pick another workspace."))?;
    let turn = chat.begin(&format!("{PROVIDER}:{model}"));
    let resume = if turn.first { None } else { SESSIONS.lock().unwrap().get(model).cloned() };
    let reply = ask(&w, json!({ "token": w.token, "type": "chat", "folder": folder, "prompt": query, "resume": resume }), ASK_CHAT).await?;
    if let Some(id) = reply["sessionId"].as_str() {
        SESSIONS.lock().unwrap().insert(model.to_string(), id.to_string());
    }
    if reply["ok"] != true {
        return Err(reply["error"].as_str().unwrap_or("VS Code gave no answer.").to_string());
    }
    let text = reply["text"].as_str().unwrap_or("").to_string();
    chat.commit(&turn, json!({ "role": "user", "content": query }), json!({ "role": "assistant", "content": text }), &query, &text);
    Ok(ChatReply { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_extensions_pipes_are_opened() {
        if cfg!(windows) {
            assert!(valid_pipe(r"\\.\pipe\ronron-vscode-1234-ab12cd34"));
            assert!(!valid_pipe(r"\\.\pipe\ronron-1234"));
        } else {
            assert!(valid_pipe("/run/user/1000/ronron-vscode-1234-ab12cd34.sock"));
            assert!(!valid_pipe("/run/user/1000/ronron.sock"));
            assert!(!valid_pipe("ronron-vscode-1.sock"));
            assert!(!valid_pipe("/tmp/../etc/ronron-vscode-1;rm.sock"));
        }
    }

    #[test]
    fn workspaces_are_named_by_folder_and_remote() {
        let w = Window { pid: 1, pipe: String::new(), token: String::new(), remote: "tunnel+ubusuiss".into(), window: "w".into() };
        assert_eq!(label("ronron", &w), "ronron · ubusuiss");
        let local = Window { remote: String::new(), ..w.clone() };
        assert_eq!(label("ronron", &local), "ronron");
        assert_eq!(label("", &w), "w");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_window_answers_through_its_socket() {
        let dir = std::env::temp_dir().join(format!("ronron-vscode-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ronron-vscode-1-test.sock");
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        tokio::spawn(async move {
            let (s, _) = listener.accept().await.unwrap();
            let (r, mut w) = tokio::io::split(s);
            let mut line = String::new();
            BufReader::new(r).read_line(&mut line).await.unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(req["token"], "tok");
            w.write_all(b"{\"ok\":true,\"folders\":[{\"name\":\"ronron\",\"path\":\"/home/greg/ronron\"}]}\n").await.unwrap();
        });
        let win = Window { pid: 1, pipe: path.to_string_lossy().into(), token: "tok".into(), remote: "tunnel+box".into(), window: String::new() };
        let reply = ask(&win, json!({ "token": "tok", "type": "folders" }), ASK_FOLDERS).await.unwrap();
        assert_eq!(reply["folders"][0]["path"], "/home/greg/ronron");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
