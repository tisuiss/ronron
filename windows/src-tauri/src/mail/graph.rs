// The mailbox through Microsoft Graph (sign-in: crate::msgraph), for the Mail pill
// once the user has signed in to Microsoft 365. Each function answers with the
// same JSON the matching Outlook script prints (outlook.rs), so the rest of the
// mail module does not care which one ran.
//
// Ronron reads, flags and moves messages (Mail.ReadWrite) but never sends one:
// Graph has no Mail.Send here. A reply opens as a draft in Outlook on the web, and
// a plan's mails are saved in Drafts for the user to send.

use serde_json::{json, Value};

use super::outlook::Message;
use super::triage::{Action, Filter, Query};
use crate::msgraph;

/// What `store_id` says for a message read through Graph.
pub const STORE: &str = "graph";
const SELECT: &str = "id,subject,from,receivedDateTime,isRead,flag";
/// Graph's attachment upload limit inside one request.
const MAX_ATTACHMENT: u64 = 3 * 1024 * 1024;

fn invalid() -> String {
    crate::i18n::t("Unexpected API response.")
}

// ── Dates ────────────────────────────────────────────────────────────────────

/// Days since 1970-01-01 of a civil date (H. Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// "2026-11-20T09:30:05Z" → milliseconds since 1970.
pub fn parse_utc(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| text.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    Some(((days * 24 + n(11..13)?) * 60 + n(14..16)?) * 60_000 + n(17..19)? * 1000)
}

// ── Messages ─────────────────────────────────────────────────────────────────

fn message(m: &Value, account: &str) -> Option<Message> {
    let received = m.get("receivedDateTime").and_then(Value::as_str).unwrap_or("");
    let from = m.pointer("/from/emailAddress");
    Some(Message {
        entry_id: m.get("id")?.as_str()?.to_string(),
        store_id: STORE.to_string(),
        account: account.to_string(),
        sender_name: from.and_then(|f| f.get("name")).and_then(Value::as_str).unwrap_or("").to_string(),
        address: from.and_then(|f| f.get("address")).and_then(Value::as_str).unwrap_or("").to_string(),
        subject: m.get("subject").and_then(Value::as_str).unwrap_or("").to_string(),
        date: parse_utc(received).unwrap_or(0),
        unread: m.get("isRead").and_then(Value::as_bool) == Some(false),
        flagged: m.pointer("/flag/flagStatus").and_then(Value::as_str) == Some("flagged"),
        local: received.get(..16).map(|d| format!("{} UTC", d.replace('T', " "))).unwrap_or_default(),
    })
}

fn messages(value: &Value) -> Result<Vec<Message>, String> {
    let account = msgraph::auth::account().unwrap_or_default();
    let items = value.get("value").and_then(Value::as_array).ok_or_else(invalid)?;
    Ok(items.iter().filter_map(|m| message(m, &account)).collect())
}

fn inbox_messages() -> Result<reqwest::Url, String> {
    msgraph::url(&["mailFolders", "inbox", "messages"])
}

/// {"unread": n, "messages": [...]}: the newest `count` messages of the inbox.
pub async fn list(count: usize) -> Result<Value, String> {
    let mut url = inbox_messages()?;
    url.query_pairs_mut()
        .append_pair("$top", &count.clamp(1, 100).to_string())
        .append_pair("$orderby", "receivedDateTime desc")
        .append_pair("$select", SELECT);
    let list = messages(&msgraph::get(url).await?)?;
    let mut folder = msgraph::url(&["mailFolders", "inbox"])?;
    folder.query_pairs_mut().append_pair("$select", "unreadItemCount");
    let unread = msgraph::get(folder).await?.get("unreadItemCount").and_then(Value::as_i64).unwrap_or(0);
    Ok(json!({ "unread": unread, "messages": list }))
}

/// A KQL query for Graph's `$search`, from the card's search box.
pub fn kql(q: &Query) -> Option<String> {
    let clean = |s: &str| s.chars().filter(|c| !matches!(c, '"' | '\\' | ':' | '(' | ')')).collect::<String>();
    let mut parts: Vec<String> = Vec::new();
    for w in &q.words {
        parts.push(clean(w));
    }
    for f in &q.from {
        parts.push(format!("from:{}", clean(f)));
    }
    for s in &q.subject {
        parts.extend(s.split_whitespace().map(|w| format!("subject:{}", clean(w))));
    }
    parts.retain(|p| !p.is_empty() && !p.ends_with(':'));
    (!parts.is_empty()).then(|| format!("\"{}\"", parts.join(" ")))
}

/// Inbox messages matching a search and a quick filter, newest first.
pub async fn search(q: &Query, filter: Filter, limit: usize, start_of_today: i64) -> Result<Vec<Message>, String> {
    let mut url = inbox_messages()?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("$top", &limit.clamp(1, 100).to_string()).append_pair("$select", SELECT);
        // $search can't be combined with $filter or $orderby: the quick filter is applied here.
        match kql(q) {
            Some(search) => {
                pairs.append_pair("$search", &search);
            }
            None => {
                pairs.append_pair("$orderby", "receivedDateTime desc");
            }
        }
    }
    let mut list = messages(&msgraph::get(url).await?)?;
    list.retain(|m| match filter {
        Filter::All => true,
        Filter::Unread => m.unread,
        Filter::Flagged => m.flagged,
        Filter::Today => m.date >= start_of_today,
    });
    list.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(list)
}

/// {"text": the message's text (`limit` characters), "attachments": [names]}.
pub async fn detail(id: &str, limit: usize) -> Result<Value, String> {
    let mut url = msgraph::url(&["messages", id])?;
    url.query_pairs_mut().append_pair("$select", "body,hasAttachments");
    let m = msgraph::call(|h, t| h.get(url).bearer_auth(t).header("Prefer", "outlook.body-content-type=\"text\"")).await?;
    let text: String = m.pointer("/body/content").and_then(Value::as_str).unwrap_or("").chars().take(limit).collect();
    let mut names = Vec::new();
    if m.get("hasAttachments").and_then(Value::as_bool) == Some(true) {
        let mut url = msgraph::url(&["messages", id, "attachments"])?;
        url.query_pairs_mut().append_pair("$select", "name");
        if let Some(items) = msgraph::get(url).await?.get("value").and_then(Value::as_array) {
            names = items.iter().filter_map(|a| a.get("name")?.as_str().map(String::from)).collect();
        }
    }
    Ok(json!({ "text": text, "attachments": names }))
}

/// Outlook on the web at the message, or at a reply to it (a draft, `body` on top). Nothing is sent.
pub async fn open(id: &str, reply_body: Option<&str>) -> Result<(), String> {
    let link = match reply_body {
        None => {
            let mut url = msgraph::url(&["messages", id])?;
            url.query_pairs_mut().append_pair("$select", "webLink");
            msgraph::get(url).await?
        }
        Some(text) => {
            let url = msgraph::url(&["messages", id, "createReply"])?;
            msgraph::call(|h, t| h.post(url).bearer_auth(t).json(&json!({ "comment": text }))).await?
        }
    };
    let web = link.get("webLink").and_then(Value::as_str).filter(|l| l.starts_with("https://")).ok_or_else(invalid)?;
    crate::platform::open_url(web);
    Ok(())
}

async fn patch(id: &str, body: Value) -> Result<(), String> {
    let url = msgraph::url(&["messages", id])?;
    msgraph::call(|h, t| h.patch(url).bearer_auth(t).json(&body)).await?;
    Ok(())
}

/// Moves a message; Graph gives it a new id there, which is returned.
async fn move_to(id: &str, destination: &str) -> Result<String, String> {
    let url = msgraph::url(&["messages", id, "move"])?;
    let moved = msgraph::call(|h, t| h.post(url).bearer_auth(t).json(&json!({ "destinationId": destination }))).await?;
    Ok(moved.get("id").and_then(Value::as_str).unwrap_or(id).to_string())
}

/// An OData string literal ('…', ' doubled).
fn odata(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The folder a rule moves to: under the inbox, else at the top, else created under the inbox.
async fn folder(name: &str) -> Result<String, String> {
    for parent in [&["mailFolders", "inbox", "childFolders"][..], &["mailFolders"][..]] {
        let mut url = msgraph::url(parent)?;
        url.query_pairs_mut()
            .append_pair("$filter", &format!("displayName eq {}", odata(name)))
            .append_pair("$select", "id");
        let found = msgraph::get(url).await?;
        if let Some(id) = found.pointer("/value/0/id").and_then(Value::as_str) {
            return Ok(id.to_string());
        }
    }
    let url = msgraph::url(&["mailFolders", "inbox", "childFolders"])?;
    let made = msgraph::call(|h, t| h.post(url).bearer_auth(t).json(&json!({ "displayName": name }))).await?;
    made.get("id").and_then(Value::as_str).map(String::from).ok_or_else(invalid)
}

/// {"entryId": the message's id afterwards}, as apply_script prints.
pub async fn apply(id: &str, action: &Action) -> Result<Value, String> {
    let after = match action {
        Action::MarkRead => {
            patch(id, json!({ "isRead": true })).await?;
            id.to_string()
        }
        Action::Important => {
            patch(id, json!({ "flag": { "flagStatus": "flagged" } })).await?;
            id.to_string()
        }
        Action::Junk => move_to(id, "junkemail").await?,
        Action::Move(name) => move_to(id, &folder(name).await?).await?,
    };
    Ok(json!({ "entryId": after }))
}

pub async fn undo(id: &str, action: &Action) -> Result<(), String> {
    match action {
        Action::Junk | Action::Move(_) => move_to(id, "inbox").await.map(|_| ()),
        Action::Important => unflag(id).await,
        Action::MarkRead => patch(id, json!({ "isRead": false })).await,
    }
}

pub async fn unflag(id: &str) -> Result<(), String> {
    patch(id, json!({ "flag": { "flagStatus": "notFlagged" } })).await
}

/// {"accounts": [{"name", "address"}]}: the signed-in user.
pub async fn accounts() -> Result<Value, String> {
    let mut url = msgraph::url(&[])?;
    url.query_pairs_mut().append_pair("$select", "displayName,mail,userPrincipalName");
    let me = msgraph::get(url).await?;
    let address = me.get("mail").and_then(Value::as_str).filter(|m| !m.is_empty())
        .or_else(|| me.get("userPrincipalName").and_then(Value::as_str)).unwrap_or("");
    Ok(json!({ "accounts": [{ "name": me.get("displayName").and_then(Value::as_str).unwrap_or(""), "address": address }] }))
}

/// A file name that stays inside the folder it is saved in.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base.chars().map(|c| if c.is_control() || "<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let clean = clean.trim().trim_matches('.').to_string();
    if clean.is_empty() { "attachment".into() } else { clean }
}

/// Saves a message's file attachments (only `only` when given) in `folder`; {"saved": [names]}.
pub async fn save_attachments(id: &str, folder: &str, only: Option<&str>) -> Result<Value, String> {
    let list = msgraph::get(msgraph::url(&["messages", id, "attachments"])?).await?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let mut saved = Vec::new();
    for a in list.get("value").and_then(Value::as_array).into_iter().flatten() {
        if a.get("@odata.type").and_then(Value::as_str) != Some("#microsoft.graph.fileAttachment") {
            continue;
        }
        let name = a.get("name").and_then(Value::as_str).unwrap_or("");
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let Some(bytes) = a.get("contentBytes").and_then(Value::as_str).and_then(msgraph::auth::base64_decode) else { continue };
        let file = safe_name(name);
        if std::fs::write(std::path::Path::new(folder).join(&file), bytes).is_ok() {
            saved.push(file);
        }
    }
    Ok(json!({ "saved": saved }))
}

/// Saves a mail in Drafts, attachments included: the user sends it from Outlook.
pub async fn draft(to: &[String], cc: &[String], subject: &str, body: &str, attachments: &[String]) -> Result<(), String> {
    let recipients = |list: &[String]| list.iter().map(|a| json!({ "emailAddress": { "address": a } })).collect::<Vec<_>>();
    let mut files = Vec::new();
    for path in attachments {
        let p = std::path::Path::new(path);
        let size = std::fs::metadata(p).map_err(|e| e.to_string())?.len();
        if size > MAX_ATTACHMENT {
            return Err(crate::i18n::t("The server's answer is too large."));
        }
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        files.push(json!({
            "@odata.type": "#microsoft.graph.fileAttachment",
            "name": p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            "contentBytes": msgraph::auth::base64_encode(&bytes),
        }));
    }
    let message = json!({
        "subject": subject,
        "body": { "contentType": "Text", "content": body },
        "toRecipients": recipients(to),
        "ccRecipients": recipients(cc),
        "attachments": files,
    });
    let url = msgraph::url(&["messages"])?;
    msgraph::call(|h, t| h.post(url).bearer_auth(t).json(&message)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_read() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2026-11-20T09:30:05Z"), Some(1_795_167_005_000));
        assert_eq!(parse_utc("2000-02-29T12:00:00Z"), Some(951_825_600_000));
        assert_eq!(parse_utc("nope"), None);
    }

    #[test]
    fn graph_messages_look_like_outlook_ones() {
        let m = message(&json!({
            "id": "AAMk=", "subject": "Devis", "isRead": false, "receivedDateTime": "2026-11-20T09:30:05Z",
            "from": { "emailAddress": { "name": "Anne", "address": "anne@contoso.ch" } },
            "flag": { "flagStatus": "flagged" },
        }), "greg@contoso.ch").unwrap();
        assert_eq!(m.store_id, STORE);
        assert_eq!(m.id(), "graph/AAMk=");
        assert!(m.unread && m.flagged);
        assert_eq!(m.address, "anne@contoso.ch");
        assert_eq!(m.local, "2026-11-20 09:30 UTC");
        assert!(message(&json!({ "subject": "no id" }), "").is_none());
    }

    #[test]
    fn searches_become_kql() {
        let q = Query::parse(r#"devis from:anne subject:"May quote""#);
        assert_eq!(kql(&q).as_deref(), Some(r#""devis from:anne subject:May subject:quote""#));
        assert_eq!(kql(&Query::parse("")), None);
        // Quotes and colons can't break out of the search.
        let k = kql(&Query::parse(r#"a"b c:d"#)).unwrap();
        assert_eq!(k.matches('"').count(), 2, "{k}");
    }

    #[test]
    fn attachment_names_stay_in_their_folder() {
        assert_eq!(safe_name("../../evil.exe"), "evil.exe");
        assert_eq!(safe_name("C:\\x\\y.pdf"), "y.pdf");
        assert_eq!(safe_name("a:b?.txt"), "a_b_.txt");
        assert_eq!(safe_name(".."), "attachment");
        assert_eq!(odata("O'Brien"), "'O''Brien'");
    }
}
