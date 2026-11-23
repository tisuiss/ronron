// « OK Ronron » with another model than Claude: Gemini, OpenAI or OpenRouter
// with the user's key (openai_compat.rs), or a model server on this computer —
// Ollama, LM Studio, any OpenAI-compatible one (local_chat.rs). The voice page
// picks it; the model is the one picked for that provider in the chat.
//
// brain.rs keeps the conversation in Claude's shape (content blocks, tool_use,
// tool_result); this file turns it into a chat completions request and the
// answer back into Claude's shape, so the loop stays the same for all of them.
// A call goes only to the provider the user picked. Web search: Anthropic's
// own for Claude, Google Search for Gemini (one more call to the same API, with
// the same key); the others have none.

use std::time::Duration;

use reqwest::Url;
use serde_json::{json, Value};

use crate::i18n::{t, tf};
use crate::settings::Settings;
use crate::{net, openai_compat, secrets};

pub const ANTHROPIC: &str = "anthropic";
/// Gemini's latest Flash, when no Gemini model was picked in the chat.
const GEMINI_DEFAULT: &str = "gemini-flash-latest";
/// Room for a thinking model (Gemini 2.5 and later) to think and still answer.
const MAX_TOKENS: u32 = 4096;
const TIMEOUT: Duration = Duration::from_secs(60);
/// Ollama and LM Studio want a bearer even when they check none.
const NO_KEY: &str = "ollama";

#[derive(Clone)]
pub enum Kind {
    /// claude.rs, with its key: brain.rs's own path.
    Anthropic,
    Cloud { provider: &'static openai_compat::Provider, key: String },
    Local { base: Url, key: Option<String> },
}

/// Who answers, and with which model ("" for Claude: brain.rs picks).
#[derive(Clone)]
pub struct Backend {
    pub id: String,
    pub kind: Kind,
    pub model: String,
}

impl Backend {
    /// Whether the model can look things up on the web.
    pub fn web(&self) -> bool {
        matches!(self.kind, Kind::Anthropic) || self.id == "google"
    }
}

/// The provider stored in the voice preferences, or Claude.
pub fn provider_id(stored: &str) -> &str {
    if stored.trim().is_empty() {
        ANTHROPIC
    } else {
        stored
    }
}

/// The backend the preferences ask for, or what is missing for it, in words
/// for the voice page.
pub fn backend(settings: &Settings, stored: &str) -> Result<Backend, String> {
    let id = provider_id(stored);
    let chosen = settings.chat_models.get(id).map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
    if id == ANTHROPIC {
        if !secrets::present(crate::claude::KEY) {
            return Err(t("Add your Claude key in AI agents → Chat."));
        }
        return Ok(Backend { id: id.into(), kind: Kind::Anthropic, model: String::new() });
    }
    if let Some(p) = openai_compat::provider(id) {
        let key = secrets::get(p.key)
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| tf("Add your {name} key in AI agents → Chat.", &[("name", p.name)]))?;
        let model = chosen.unwrap_or_else(|| if id == "google" { GEMINI_DEFAULT.into() } else { p.default_model.into() });
        return Ok(Backend { id: id.into(), kind: Kind::Cloud { provider: p, key }, model });
    }
    if let Some(server) = crate::local_chat::server(settings, id) {
        let name = t(server.name);
        if server.url.trim().is_empty() {
            return Err(tf("Connect {name} in AI agents → Chat.", &[("name", &name)]));
        }
        let base = net::normalise_server_url(&server.url)?;
        let model = chosen.ok_or_else(|| tf("Pick a {name} model in AI agents → Chat.", &[("name", &name)]))?;
        return Ok(Backend { id: id.into(), kind: Kind::Local { base, key: server.key }, model });
    }
    Err(tf("Unknown provider: {name}", &[("name", id)]))
}

// ── Claude's shape ⇄ chat completions ─────────────────────────────────────────

/// The web search Ronron runs itself (Gemini), as a function the model can call.
fn web_function() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "web_search",
            "description": "Search the web for anything current: weather, news, scores, prices, opening hours, facts you are not sure of.",
            "parameters": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "What to search, in the user's language." } },
                "required": ["query"]
            }
        }
    })
}

/// brain.rs's tools as functions. Anthropic's server tools (its web search)
/// are left out; `web` adds Ronron's own web search instead.
pub fn functions(tools: &[Value], web: bool) -> Vec<Value> {
    let mut out: Vec<Value> = tools
        .iter()
        .filter(|tool| tool.get("input_schema").is_some())
        .map(|tool| {
            let mut function = json!({ "name": tool["name"], "description": tool["description"] });
            // Gemini refuses an object with no properties (service_info has
            // none): such a function goes without parameters, as OpenAI allows.
            let schema = &tool["input_schema"];
            if schema["properties"].as_object().is_some_and(|p| !p.is_empty()) {
                function["parameters"] = schema.clone();
            }
            json!({ "type": "function", "function": function })
        })
        .collect();
    if web {
        out.push(web_function());
    }
    out
}

/// The conversation as chat completions messages, the system prompt first.
pub fn messages(system: &str, history: &[Value]) -> Vec<Value> {
    let mut out = vec![json!({ "role": "system", "content": system })];
    for message in history {
        let role = message["role"].as_str().unwrap_or("user");
        let Some(blocks) = message["content"].as_array() else {
            out.push(json!({ "role": role, "content": message["content"].as_str().unwrap_or("") }));
            continue;
        };
        if role == "assistant" {
            // The provider's own message, as it came, when we have it: Gemini
            // wants its thought signatures back with the tool calls.
            if let Some(raw) = blocks.iter().find(|b| b["type"] == "openai_message") {
                out.push(raw["message"].clone());
                continue;
            }
            let text: Vec<&str> = blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
            let calls: Vec<Value> = blocks
                .iter()
                .filter(|b| b["type"] == "tool_use")
                .map(|b| {
                    json!({
                        "id": b["id"],
                        "type": "function",
                        "function": { "name": b["name"], "arguments": b["input"].to_string() }
                    })
                })
                .collect();
            let mut m = json!({ "role": "assistant", "content": text.join(" ") });
            if !calls.is_empty() {
                m["tool_calls"] = Value::Array(calls);
            }
            out.push(m);
        } else {
            for block in blocks {
                match block["type"].as_str() {
                    Some("tool_result") => out.push(json!({
                        "role": "tool",
                        "tool_call_id": block["tool_use_id"],
                        "content": block["content"].as_str().map(str::to_string).unwrap_or_else(|| block["content"].to_string()),
                    })),
                    Some("text") => out.push(json!({ "role": "user", "content": block["text"] })),
                    _ => {}
                }
            }
        }
    }
    out
}

/// What a small local model thinks out loud, left out of what is said.
fn without_thinking(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => return out.trim().to_string(),
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// A chat completions answer in Claude's shape: `content` blocks and a `stop_reason`.
pub fn answer(response: &Value) -> Result<Value, String> {
    let Some(message) = response.pointer("/choices/0/message") else {
        let why = response.pointer("/error/message").and_then(Value::as_str).unwrap_or("no answer");
        return Err(why.to_string());
    };
    let mut content = Vec::new();
    let text = without_thinking(message["content"].as_str().unwrap_or(""));
    if !text.is_empty() {
        content.push(json!({ "type": "text", "text": text }));
    }
    let mut raw = message.clone();
    let mut calls = 0;
    if let Some(list) = message["tool_calls"].as_array() {
        for (i, call) in list.iter().enumerate() {
            let id = call["id"].as_str().filter(|id| !id.is_empty()).map(str::to_string).unwrap_or_else(|| format!("call_{i}"));
            raw["tool_calls"][i]["id"] = json!(id);
            let arguments = &call["function"]["arguments"];
            let input = match arguments {
                Value::String(s) if s.trim().is_empty() => json!({}),
                Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
                Value::Object(_) => arguments.clone(),
                _ => json!({}),
            };
            content.push(json!({ "type": "tool_use", "id": id, "name": call["function"]["name"], "input": input }));
            calls += 1;
        }
    }
    if raw["content"].is_null() {
        raw["content"] = json!("");
    }
    content.push(json!({ "type": "openai_message", "message": raw }));
    let stop = if calls > 0 { "tool_use" } else { "end_turn" };
    Ok(json!({ "content": content, "stop_reason": stop }))
}

/// Claude's blocks of ours that no API should see again when the history goes
/// to Claude: the provider's raw message.
pub fn is_ours(block: &Value) -> bool {
    block["type"] == "openai_message"
}

// ── Calls ─────────────────────────────────────────────────────────────────────

fn join(base: &Url, tail: &str) -> Result<Url, String> {
    Url::parse(&net::join(base, tail)).map_err(|e| e.to_string())
}

/// One turn with a provider other than Claude.
pub async fn turn(backend: &Backend, system: &str, history: &[Value], tools: &[Value]) -> Result<Value, String> {
    let (endpoint, bearer, field, name) = match &backend.kind {
        Kind::Anthropic => return Err("Claude goes through claude.rs".into()),
        Kind::Cloud { provider, key } => {
            let base = Url::parse(provider.base_url).map_err(|e| e.to_string())?;
            (join(&base, "chat/completions")?, key.clone(), provider.max_tokens_field, provider.name.to_string())
        }
        Kind::Local { base, key } => (
            join(base, "v1/chat/completions")?,
            key.clone().filter(|k| !k.is_empty()).unwrap_or_else(|| NO_KEY.into()),
            "max_tokens",
            backend.id.clone(),
        ),
    };
    let mut body = json!({
        "model": backend.model,
        "messages": messages(system, history),
        "tools": functions(tools, backend.web()),
    });
    body[field] = json!(MAX_TOKENS);
    let response = net::client(&endpoint, TIMEOUT)?
        .post(endpoint.clone())
        .bearer_auth(&bearer)
        .json(&body)
        .send()
        .await
        .map_err(|e| tf("Network error: {error}", &[("error", &e.to_string())]))?;
    let status = response.status();
    if !status.is_success() {
        let detail = net::read_capped(response, net::MAX_ERROR_BODY).await.unwrap_or_default();
        return Err(format!("{name} {status}: {}", net::error_detail(&detail)));
    }
    let bytes = net::read_capped(response, net::MAX_BODY).await?;
    let json: Value = serde_json::from_slice(&bytes).map_err(|e| tf("Bad API response: {error}", &[("error", &e.to_string())]))?;
    answer(&json).map_err(|why| format!("{name}: {why}"))
}

/// The Gemini API's own name for a model: no "models/" in front.
fn gemini_model(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

/// Ronron's web search with Gemini: the question goes to the same model with
/// Google Search turned on, and its answer comes back as the tool's result.
pub async fn web_search(backend: &Backend, query: &str) -> String {
    let Kind::Cloud { key, .. } = &backend.kind else {
        return "No web search with this model: say you cannot look it up.".into();
    };
    if backend.id != "google" {
        return "No web search with this model: say you cannot look it up.".into();
    }
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        gemini_model(&backend.model)
    );
    let Ok(endpoint) = Url::parse(&url) else {
        return "The search failed.".into();
    };
    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": format!("Search the web and answer briefly, with the facts and numbers: {query}") }] }],
        "tools": [{ "google_search": {} }],
    });
    let result = async {
        let response = net::client(&endpoint, TIMEOUT)?
            .post(endpoint.clone())
            .header("x-goog-api-key", key)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        if !status.is_success() {
            let detail = net::read_capped(response, net::MAX_ERROR_BODY).await.unwrap_or_default();
            return Err(format!("{status}: {}", net::error_detail(&detail)));
        }
        let bytes = net::read_capped(response, net::MAX_BODY).await?;
        let json: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        Ok::<String, String>(search_text(&json))
    }
    .await;
    match result {
        Ok(text) if !text.is_empty() => text,
        Ok(_) => "The search found nothing.".into(),
        Err(err) => {
            crate::log::line(format!("voice: web search failed: {err}"));
            "The search failed: say you cannot look it up right now.".into()
        }
    }
}

/// The text of a generateContent answer.
fn search_text(response: &Value) -> String {
    response
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<Value> {
        vec![
            json!({ "name": "pills", "description": "Pills.", "input_schema": { "type": "object", "properties": { "action": { "type": "string" } } } }),
            json!({ "type": "web_search_20250305", "name": "web_search", "max_uses": 3 }),
        ]
    }

    #[test]
    fn tools_become_functions_and_anthropics_web_search_is_left_out() {
        let f = functions(&tools(), false);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0]["type"], "function");
        assert_eq!(f[0]["function"]["name"], "pills");
        assert_eq!(f[0]["function"]["parameters"]["type"], "object");
        let with_web = functions(&tools(), true);
        assert_eq!(with_web.len(), 2);
        assert_eq!(with_web[1]["function"]["name"], "web_search");
    }

    #[test]
    fn a_tool_without_parameters_is_sent_without_a_schema() {
        let tools = vec![json!({ "name": "service_info", "description": "Data.", "input_schema": { "type": "object", "properties": {} } })];
        let f = functions(&tools, false);
        assert_eq!(f[0]["function"]["name"], "service_info");
        assert!(f[0]["function"].get("parameters").is_none());
    }

    #[test]
    fn a_conversation_with_a_tool_call_becomes_chat_messages() {
        let history = vec![
            json!({ "role": "user", "content": "add Stripe" }),
            json!({ "role": "assistant", "content": [
                { "type": "text", "text": "On it." },
                { "type": "tool_use", "id": "t1", "name": "pills", "input": { "action": "add", "names": ["Stripe"] } }
            ] }),
            json!({ "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "t1", "content": "added" }] }),
        ];
        let m = messages("sys", &history);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0], json!({ "role": "system", "content": "sys" }));
        assert_eq!(m[1], json!({ "role": "user", "content": "add Stripe" }));
        assert_eq!(m[2]["content"], "On it.");
        assert_eq!(m[2]["tool_calls"][0]["id"], "t1");
        assert_eq!(m[2]["tool_calls"][0]["function"]["name"], "pills");
        let args: Value = serde_json::from_str(m[2]["tool_calls"][0]["function"]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(args["names"][0], "Stripe");
        assert_eq!(m[3], json!({ "role": "tool", "tool_call_id": "t1", "content": "added" }));
    }

    #[test]
    fn an_answer_with_tool_calls_comes_back_in_claudes_shape() {
        let response = json!({ "choices": [{ "message": {
            "role": "assistant", "content": null,
            "tool_calls": [{ "id": "", "type": "function", "function": { "name": "music", "arguments": "{\"action\":\"next\"}" } }],
            "extra_content": { "google": { "thought_signature": "sig" } }
        } }] });
        let turn = answer(&response).unwrap();
        assert_eq!(turn["stop_reason"], "tool_use");
        let content = turn["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "call_0");
        assert_eq!(content[0]["input"]["action"], "next");
        // The raw message goes back as it came, with the id filled in.
        let history = vec![json!({ "role": "assistant", "content": content })];
        let m = messages("s", &history);
        assert_eq!(m[1]["tool_calls"][0]["id"], "call_0");
        assert_eq!(m[1]["extra_content"]["google"]["thought_signature"], "sig");
        assert_eq!(m[1]["content"], "");
    }

    #[test]
    fn a_plain_answer_ends_the_turn_and_its_thinking_is_not_said() {
        let response = json!({ "choices": [{ "message": { "role": "assistant", "content": "<think>hmm</think> Il fait beau." } }] });
        let turn = answer(&response).unwrap();
        assert_eq!(turn["stop_reason"], "end_turn");
        assert_eq!(turn["content"][0], json!({ "type": "text", "text": "Il fait beau." }));
        assert!(answer(&json!({ "error": { "message": "quota" } })).unwrap_err().contains("quota"));
    }

    #[test]
    fn arguments_given_as_an_object_or_empty_are_read() {
        let response = json!({ "choices": [{ "message": { "content": "", "tool_calls": [
            { "id": "a", "function": { "name": "service_info", "arguments": "" } },
            { "id": "b", "function": { "name": "pills", "arguments": { "action": "list" } } }
        ] } }] });
        let turn = answer(&response).unwrap();
        assert_eq!(turn["content"][0]["input"], json!({}));
        assert_eq!(turn["content"][1]["input"]["action"], "list");
    }

    #[test]
    fn an_empty_provider_is_claude_and_gemini_models_lose_their_prefix() {
        assert_eq!(provider_id(""), ANTHROPIC);
        assert_eq!(provider_id("ollama"), "ollama");
        assert_eq!(gemini_model("models/gemini-2.5-flash"), "gemini-2.5-flash");
        assert_eq!(gemini_model("gemini-flash-latest"), "gemini-flash-latest");
    }

    #[test]
    fn a_local_server_needs_its_address_and_a_model() {
        let mut s = Settings::default();
        assert!(backend(&s, "ollama").is_err());
        s.ollama_url = "http://127.0.0.1:11434".into();
        assert!(backend(&s, "ollama").is_err());
        s.chat_models.insert("ollama".into(), "qwen3:8b".into());
        let b = backend(&s, "ollama").unwrap();
        assert_eq!(b.model, "qwen3:8b");
        assert!(!b.web());
        assert!(matches!(b.kind, Kind::Local { .. }));
    }

    #[test]
    fn the_search_answer_is_its_text() {
        let r = json!({ "candidates": [{ "content": { "parts": [{ "text": "18 °C " }, { "text": "à Genève." }] } }] });
        assert_eq!(search_text(&r), "18 °C à Genève.");
        assert_eq!(search_text(&json!({})), "");
    }
}
