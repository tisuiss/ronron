// The mailbox through the Gmail API (sign-in: crate::google), for the Mail tab once
// the user has signed in to Google. Each function answers with the same JSON as
// its Microsoft Graph twin (graph.rs) and the Outlook scripts (outlook.rs), so the
// rest of the mail module does not care which one ran.
//
// Ronron reads, labels, archives and trashes (gmail.modify) but never sends: a
// reply and a plan's mails are saved in Drafts for the user to send from Gmail.
// Gmail's labels stand in for folders: "Important" is the star, "Junk" is Spam,
// and a rule's folder is a label of that name (the message leaves the inbox).

use serde_json::{json, Value};

use super::outlook::Message;
use super::triage::{Action, Filter, Query};
use crate::google::{self, GMAIL};
use crate::msgraph::auth::{base64url, base64url_encode};

/// What `store_id` says for a message read through Gmail.
pub const STORE: &str = "gmail";
/// A draft's attachments, all together (Gmail's own limit is 25 MB).
const MAX_ATTACHMENTS: u64 = 20 * 1024 * 1024;

fn invalid() -> String {
    crate::i18n::t("Unexpected API response.")
}

fn url(segments: &[&str]) -> Result<reqwest::Url, String> {
    google::url(GMAIL, segments)
}

fn account() -> String {
    google::auth::account().unwrap_or_default()
}

// ── Messages ─────────────────────────────────────────────────────────────────

fn header<'a>(m: &'a Value, name: &str) -> &'a str {
    m.pointer("/payload/headers").and_then(Value::as_array).into_iter().flatten()
        .find(|h| h.get("name").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|h| h.get("value")).and_then(Value::as_str).unwrap_or("")
}

/// `"Anne Martin" <anne@x.ch>`, `Anne <anne@x.ch>` or `anne@x.ch` → (name, address).
pub fn split_from(from: &str) -> (String, String) {
    let from = from.trim();
    match (from.rfind('<'), from.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            let name = from[..open].trim().trim_matches('"').trim().to_string();
            (name, from[open + 1..close].trim().to_string())
        }
        _ => (String::new(), from.to_string()),
    }
}

fn has_label(m: &Value, label: &str) -> bool {
    m.get("labelIds").and_then(Value::as_array).is_some_and(|l| l.iter().any(|x| x.as_str() == Some(label)))
}

pub fn message(m: &Value, account: &str) -> Option<Message> {
    let date: i64 = m.get("internalDate").and_then(Value::as_str).and_then(|d| d.parse().ok()).unwrap_or(0);
    let (sender_name, address) = split_from(header(m, "From"));
    let iso = crate::whatsapp::gateway::iso_utc(date);
    Some(Message {
        entry_id: m.get("id")?.as_str()?.to_string(),
        store_id: STORE.to_string(),
        account: account.to_string(),
        sender_name,
        address,
        subject: header(m, "Subject").to_string(),
        date,
        unread: has_label(m, "UNREAD"),
        flagged: has_label(m, "STARRED"),
        local: iso.get(..16).map(|d| format!("{} UTC", d.replace('T', " "))).unwrap_or_default(),
    })
}

/// The headers of one message (no body).
async fn metadata(id: &str) -> Result<Value, String> {
    let mut u = url(&["messages", id])?;
    u.query_pairs_mut()
        .append_pair("format", "metadata")
        .append_pair("metadataHeaders", "From")
        .append_pair("metadataHeaders", "Subject");
    google::get(u).await
}

/// The messages a `messages.list` answer names, read one by one.
async fn read_all(listing: &Value) -> Result<Vec<Message>, String> {
    let account = account();
    let mut out = Vec::new();
    for item in listing.get("messages").and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = item.get("id").and_then(Value::as_str) else { continue };
        if let Some(m) = message(&metadata(id).await?, &account) {
            out.push(m);
        }
    }
    Ok(out)
}

/// {"unread": n, "messages": [...]}: the newest `count` messages of the inbox.
pub async fn list(count: usize) -> Result<Value, String> {
    let mut u = url(&["messages"])?;
    u.query_pairs_mut().append_pair("labelIds", "INBOX").append_pair("maxResults", &count.clamp(1, 100).to_string());
    let list = read_all(&google::get(u).await?).await?;
    let unread = google::get(url(&["labels", "INBOX"])?).await?.get("messagesUnread").and_then(Value::as_i64).unwrap_or(0);
    Ok(json!({ "unread": unread, "messages": list }))
}

/// A Gmail search from the card's search box and quick filter, inside the inbox.
pub fn gmail_query(q: &Query, filter: Filter) -> String {
    let clean = |s: &str| s.chars().filter(|c| !matches!(c, '"' | '\\' | ':' | '(' | ')' | '{' | '}')).collect::<String>();
    let mut parts = vec!["in:inbox".to_string()];
    parts.extend(q.words.iter().map(|w| clean(w)).filter(|w| !w.is_empty()));
    parts.extend(q.from.iter().map(|f| clean(f)).filter(|f| !f.is_empty()).map(|f| format!("from:{f}")));
    parts.extend(q.subject.iter().map(|s| clean(s)).filter(|s| !s.trim().is_empty()).map(|s| format!("subject:\"{}\"", s.trim())));
    match filter {
        Filter::All => {}
        Filter::Unread => parts.push("is:unread".into()),
        Filter::Flagged => parts.push("is:starred".into()),
        Filter::Today => parts.push("newer_than:1d".into()),
    }
    parts.join(" ")
}

/// Inbox messages matching a search and a quick filter, newest first.
pub async fn search(q: &Query, filter: Filter, limit: usize, start_of_today: i64) -> Result<Vec<Message>, String> {
    let mut u = url(&["messages"])?;
    u.query_pairs_mut().append_pair("q", &gmail_query(q, filter)).append_pair("maxResults", &limit.clamp(1, 100).to_string());
    let mut list = read_all(&google::get(u).await?).await?;
    // "newer_than:1d" is the last 24 hours: today starts at midnight.
    if filter == Filter::Today {
        list.retain(|m| m.date >= start_of_today);
    }
    list.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(list)
}

/// A part's data, decoded.
fn data(part: &Value) -> Option<Vec<u8>> {
    base64url(part.pointer("/body/data")?.as_str()?)
}

/// Walks the parts: the first text/plain (else text/html, its tags dropped) and the attachments' names.
fn walk(part: &Value, plain: &mut Option<String>, html: &mut Option<String>, files: &mut Vec<(String, String)>) {
    let mime = part.get("mimeType").and_then(Value::as_str).unwrap_or("");
    let name = part.get("filename").and_then(Value::as_str).unwrap_or("");
    if !name.is_empty() {
        let id = part.pointer("/body/attachmentId").and_then(Value::as_str).unwrap_or("");
        files.push((name.to_string(), id.to_string()));
    } else if mime == "text/plain" && plain.is_none() {
        *plain = data(part).map(|b| String::from_utf8_lossy(&b).into_owned());
    } else if mime == "text/html" && html.is_none() {
        *html = data(part).map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    for p in part.get("parts").and_then(Value::as_array).into_iter().flatten() {
        walk(p, plain, html, files);
    }
}

/// HTML reduced to its text, enough to read a message on the card.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.to_ascii_lowercase();
                let name = t.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_string();
                if name == "style" || name == "script" {
                    skip = !t.starts_with('/');
                }
                if matches!(name.as_str(), "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3") {
                    out.push('\n');
                }
            }
            _ if in_tag => tag.push(c),
            _ if skip => {}
            _ => out.push(c),
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let mut text = lines.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.trim().to_string()
}

async fn full(id: &str) -> Result<Value, String> {
    let mut u = url(&["messages", id])?;
    u.query_pairs_mut().append_pair("format", "full");
    google::get(u).await
}

/// {"text": the message's text (`limit` characters), "attachments": [names]}.
pub async fn detail(id: &str, limit: usize) -> Result<Value, String> {
    let m = full(id).await?;
    let payload = m.get("payload").ok_or_else(invalid)?;
    let (mut plain, mut html, mut files) = (None, None, Vec::new());
    walk(payload, &mut plain, &mut html, &mut files);
    let text = plain.or_else(|| html.map(|h| strip_html(&h))).unwrap_or_default();
    let text: String = text.chars().take(limit).collect();
    Ok(json!({ "text": text, "attachments": files.into_iter().map(|(n, _)| n).collect::<Vec<_>>() }))
}

/// Gmail on the web, in the signed-in account.
fn web(fragment: &str) -> String {
    let mut u = reqwest::Url::parse("https://mail.google.com/mail/").expect("static URL");
    let who = account();
    if !who.is_empty() {
        u.query_pairs_mut().append_pair("authuser", &who);
    }
    u.set_fragment(Some(fragment));
    u.into()
}

/// Gmail on the web at the message, or, with `reply_body`, a reply saved in Drafts
/// (the text on top) and Gmail opened at the drafts. Nothing is sent.
pub async fn open(id: &str, reply_body: Option<&str>) -> Result<(), String> {
    let Some(text) = reply_body else {
        crate::platform::open_url(&web(&format!("all/{id}")));
        return Ok(());
    };
    let mut u = url(&["messages", id])?;
    u.query_pairs_mut()
        .append_pair("format", "metadata")
        .append_pair("metadataHeaders", "From")
        .append_pair("metadataHeaders", "Reply-To")
        .append_pair("metadataHeaders", "Subject")
        .append_pair("metadataHeaders", "Message-ID")
        .append_pair("metadataHeaders", "References");
    let m = google::get(u).await?;
    let to = match header(&m, "Reply-To") {
        "" => header(&m, "From"),
        r => r,
    };
    let (_, address) = split_from(to);
    let subject = header(&m, "Subject");
    let subject = if subject.to_ascii_lowercase().starts_with("re:") { subject.to_string() } else { format!("Re: {subject}") };
    let message_id = header(&m, "Message-ID");
    let references = format!("{} {}", header(&m, "References"), message_id).trim().to_string();
    let mut extra = Vec::new();
    if !message_id.is_empty() {
        extra.push(("In-Reply-To", message_id.to_string()));
        extra.push(("References", references));
    }
    let raw = mime(&[address], &[], &subject, text, &[], &extra)?;
    let mut draft = json!({ "message": { "raw": base64url_encode(raw.as_bytes()) } });
    if let Some(thread) = m.get("threadId").and_then(Value::as_str) {
        draft["message"]["threadId"] = json!(thread);
    }
    google::post(url(&["drafts"])?, draft).await?;
    crate::platform::open_url(&web("drafts"));
    Ok(())
}

async fn modify(id: &str, add: &[&str], remove: &[&str]) -> Result<(), String> {
    google::post(url(&["messages", id, "modify"])?, json!({ "addLabelIds": add, "removeLabelIds": remove })).await.map(|_| ())
}

/// A user label's id by its name, created when it does not exist yet.
async fn label(name: &str) -> Result<String, String> {
    let labels = google::get(url(&["labels"])?).await?;
    let found = labels.get("labels").and_then(Value::as_array).into_iter().flatten()
        .find(|l| l.get("name").and_then(Value::as_str).is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|l| l.get("id")).and_then(Value::as_str).map(String::from);
    if let Some(id) = found {
        return Ok(id);
    }
    let made = google::post(url(&["labels"])?, json!({ "name": name, "labelListVisibility": "labelShow", "messageListVisibility": "show" })).await?;
    made.get("id").and_then(Value::as_str).map(String::from).ok_or_else(invalid)
}

/// {"entryId": the message's id afterwards} — Gmail's ids never change.
pub async fn apply(id: &str, action: &Action) -> Result<Value, String> {
    match action {
        Action::MarkRead => modify(id, &[], &["UNREAD"]).await?,
        Action::Important => modify(id, &["STARRED"], &[]).await?,
        Action::Junk => modify(id, &["SPAM"], &["INBOX"]).await?,
        Action::Move(name) => {
            let l = label(name).await?;
            modify(id, &[l.as_str()], &["INBOX"]).await?
        }
    }
    Ok(json!({ "entryId": id }))
}

pub async fn undo(id: &str, action: &Action) -> Result<(), String> {
    match action {
        Action::Junk => modify(id, &["INBOX"], &["SPAM"]).await,
        Action::Move(name) => {
            let l = label(name).await?;
            modify(id, &["INBOX"], &[l.as_str()]).await
        }
        Action::Important => unflag(id).await,
        Action::MarkRead => modify(id, &["UNREAD"], &[]).await,
    }
}

/// To the Trash (recoverable for 30 days): Ronron never deletes for good.
pub async fn delete(id: &str) -> Result<(), String> {
    google::post(url(&["messages", id, "trash"])?, json!({})).await.map(|_| ())
}

pub async fn unflag(id: &str) -> Result<(), String> {
    modify(id, &[], &["STARRED"]).await
}

/// {"accounts": [{"name", "address"}]}: the signed-in user.
pub async fn accounts() -> Result<Value, String> {
    Ok(json!({ "accounts": [{ "name": "", "address": account() }] }))
}

/// Saves a message's file attachments (only `only` when given) in `folder`; {"saved": [names]}.
pub async fn save_attachments(id: &str, folder: &str, only: Option<&str>) -> Result<Value, String> {
    let m = full(id).await?;
    let (mut plain, mut html, mut files) = (None, None, Vec::new());
    walk(m.get("payload").ok_or_else(invalid)?, &mut plain, &mut html, &mut files);
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let mut saved = Vec::new();
    for (name, attachment) in files {
        if only.is_some_and(|o| o != name) || attachment.is_empty() {
            continue;
        }
        let body = google::get(url(&["messages", id, "attachments", &attachment])?).await?;
        let Some(bytes) = body.get("data").and_then(Value::as_str).and_then(base64url) else { continue };
        let file = super::graph::safe_name(&name);
        if std::fs::write(std::path::Path::new(folder).join(&file), bytes).is_ok() {
            saved.push(file);
        }
    }
    Ok(json!({ "saved": saved }))
}

/// A header value in RFC 2047 when it is not plain ASCII.
fn encoded(text: &str) -> String {
    if text.is_ascii() {
        text.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", crate::whatsapp::gateway::base64(text.as_bytes()))
    }
}

/// Base64 in lines of 76, as MIME wants it.
fn wrapped(bytes: &[u8]) -> String {
    let b = crate::whatsapp::gateway::base64(bytes);
    b.as_bytes().chunks(76).map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join("\r\n")
}

/// A whole mail, text and attachments, as Gmail's `raw` wants it.
pub fn mime(to: &[String], cc: &[String], subject: &str, body: &str, attachments: &[(String, Vec<u8>)], extra: &[(&str, String)]) -> Result<String, String> {
    let line = |s: &str| s.replace(['\r', '\n'], " ");
    let mut head = format!("To: {}\r\n", line(&to.join(", ")));
    if !cc.is_empty() {
        head.push_str(&format!("Cc: {}\r\n", line(&cc.join(", "))));
    }
    head.push_str(&format!("Subject: {}\r\n", encoded(&line(subject))));
    for (name, value) in extra {
        head.push_str(&format!("{name}: {}\r\n", line(value)));
    }
    head.push_str("MIME-Version: 1.0\r\n");
    let text_part = format!("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n", wrapped(body.as_bytes()));
    if attachments.is_empty() {
        return Ok(format!("{head}{text_part}"));
    }
    let boundary = format!("ronron-{}", base64url_encode(&crate::msgraph::auth::random(12)?));
    let mut out = format!("{head}Content-Type: multipart/mixed; boundary=\"{boundary}\"\r\n\r\n--{boundary}\r\n{text_part}");
    for (name, bytes) in attachments {
        let name = line(name).replace('"', "'");
        out.push_str(&format!(
            "--{boundary}\r\nContent-Type: application/octet-stream; name=\"{n}\"\r\nContent-Disposition: attachment; filename=\"{n}\"\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n",
            wrapped(bytes),
            n = encoded(&name),
        ));
    }
    out.push_str(&format!("--{boundary}--\r\n"));
    Ok(out)
}

/// Saves a mail in Gmail's Drafts, attachments included: the user sends it from Gmail.
pub async fn draft(to: &[String], cc: &[String], subject: &str, body: &str, attachments: &[String]) -> Result<(), String> {
    let mut files = Vec::new();
    let mut total = 0;
    for path in attachments {
        let p = std::path::Path::new(path);
        total += std::fs::metadata(p).map_err(|e| e.to_string())?.len();
        if total > MAX_ATTACHMENTS {
            return Err(crate::i18n::t("The server's answer is too large."));
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        files.push((name, std::fs::read(p).map_err(|e| e.to_string())?));
    }
    let raw = mime(to, cc, subject, body, &files, &[])?;
    google::post(url(&["drafts"])?, json!({ "message": { "raw": base64url_encode(raw.as_bytes()) } })).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn senders_are_split() {
        assert_eq!(split_from("\"Anne Martin\" <anne@x.ch>"), ("Anne Martin".into(), "anne@x.ch".into()));
        assert_eq!(split_from("Anne <anne@x.ch>"), ("Anne".into(), "anne@x.ch".into()));
        assert_eq!(split_from("anne@x.ch"), ("".into(), "anne@x.ch".into()));
    }

    #[test]
    fn gmail_messages_look_like_outlook_ones() {
        let m = message(&json!({
            "id": "18c0f", "threadId": "18c0f", "labelIds": ["INBOX", "UNREAD", "STARRED"], "internalDate": "1795167005000",
            "payload": { "headers": [{ "name": "From", "value": "Anne <anne@x.ch>" }, { "name": "subject", "value": "Devis" }] },
        }), "greg@gmail.com").unwrap();
        assert_eq!(m.id(), "gmail/18c0f");
        assert_eq!((m.sender_name.as_str(), m.address.as_str(), m.subject.as_str()), ("Anne", "anne@x.ch", "Devis"));
        assert!(m.unread && m.flagged);
        assert_eq!(m.date, 1_795_167_005_000);
        assert_eq!(m.local, "2026-11-20 09:30 UTC");
        assert!(message(&json!({ "labelIds": [] }), "").is_none());
    }

    #[test]
    fn searches_stay_in_the_inbox_and_cannot_break_out() {
        let q = Query::parse(r#"devis from:anne subject:"May quote""#);
        assert_eq!(gmail_query(&q, Filter::Unread), r#"in:inbox devis from:anne subject:"May quote" is:unread"#);
        assert_eq!(gmail_query(&Query::parse(""), Filter::All), "in:inbox");
        let tricky = gmail_query(&Query::parse(r#"a"b} c:d"#), Filter::All);
        assert!(!tricky.contains('}') && tricky.matches('"').count() == 0, "{tricky}");
    }

    #[test]
    fn the_text_is_found_in_the_parts() {
        let html = base64url_encode(b"<html><style>p{}</style><p>Bonjour&nbsp;Anne</p><div>Merci</div></html>");
        let plain = base64url_encode("Bonjour Anne\nÀ bientôt".as_bytes());
        let tree = json!({ "mimeType": "multipart/mixed", "parts": [
            { "mimeType": "multipart/alternative", "parts": [
                { "mimeType": "text/plain", "body": { "data": plain } },
                { "mimeType": "text/html", "body": { "data": html } },
            ] },
            { "mimeType": "application/pdf", "filename": "devis.pdf", "body": { "attachmentId": "A1" } },
        ] });
        let (mut p, mut h, mut f) = (None, None, Vec::new());
        walk(&tree, &mut p, &mut h, &mut f);
        assert_eq!(p.as_deref(), Some("Bonjour Anne\nÀ bientôt"));
        assert_eq!(strip_html(&h.unwrap()), "Bonjour Anne\n\nMerci");
        assert_eq!(f, vec![("devis.pdf".to_string(), "A1".to_string())]);
    }

    #[test]
    fn drafts_are_well_formed_mime() {
        let raw = mime(&["anne@x.ch".into()], &[], "Devis été", "Bonjour", &[], &[("In-Reply-To", "<a@b>".into())]).unwrap();
        assert!(raw.starts_with("To: anne@x.ch\r\nSubject: =?UTF-8?B?"), "{raw}");
        assert!(raw.contains("In-Reply-To: <a@b>\r\n") && raw.contains("charset=UTF-8"));
        // A header can't be split by a newline in what the AI wrote.
        let evil = mime(&["a@x.ch\r\nBcc: evil@x.ch".into()], &[], "s", "b", &[], &[]).unwrap();
        assert!(!evil.contains("\r\nBcc:"), "{evil}");
        let with_file = mime(&["a@x.ch".into()], &["b@x.ch".into()], "s", "b", &[("plan.pdf".into(), vec![1, 2, 3])], &[]).unwrap();
        assert!(with_file.contains("multipart/mixed") && with_file.contains("filename=\"plan.pdf\"") && with_file.contains("Cc: b@x.ch"));
    }
}
