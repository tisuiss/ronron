//! iCal calendars (an .ics address: Google's secret address, Outlook's published
//! calendar, iCloud, a school…): fetched like a page, read here, recurring events
//! expanded (RRULE, EXDATE, moved occurrences), in the shape of the Microsoft 365
//! calendar's events (calendar365.rs).
//!
//! The address is a secret (it opens the calendar to anyone who has it): it lives
//! in the credential store, in `ical-url-0` … `ical-url-7`.

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{json, Value};

/// How many iCal calendars Ronron keeps.
pub const SLOTS: usize = 8;
/// An .ics bigger than this is not read.
const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Occurrences of one recurring event inside the window, at most.
const MAX_OCCURRENCES: u16 = 400;

pub fn key(slot: usize) -> String {
    format!("ical-url-{slot}")
}

/// "webcal://…" → "https://…"; only https addresses are fetched.
pub fn normalize_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let url = if let Some(rest) = raw.strip_prefix("webcal://").or_else(|| raw.strip_prefix("webcals://")) {
        format!("https://{rest}")
    } else {
        raw.to_string()
    };
    let parsed = reqwest::Url::parse(&url).ok()?;
    (parsed.scheme() == "https" && parsed.host_str().is_some()).then_some(url)
}

pub async fn fetch(url: &str) -> Result<String, String> {
    let url = normalize_url(url).ok_or_else(|| crate::i18n::t("The iCal address must start with https:// or webcal://."))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(&url).header("Accept", "text/calendar").send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(crate::i18n::tf("The calendar answered {status}.", &[("status", &response.status().as_u16().to_string())]));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_BYTES {
            return Err(crate::i18n::t("The calendar is too big."));
        }
    }
    let text = String::from_utf8_lossy(&body).into_owned();
    if !text.contains("BEGIN:VCALENDAR") {
        return Err(crate::i18n::t("This address is not an iCal calendar."));
    }
    Ok(text)
}

// ── Reading ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, name: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Lines folded per RFC 5545 joined again.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if (line.starts_with(' ') || line.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&line[1..]);
        } else if !line.is_empty() {
            lines.push(line.to_string());
        }
    }
    lines
}

fn parse_line(line: &str) -> Option<Prop> {
    // The name and parameters end at the first ':' outside quotes.
    let mut quoted = false;
    let mut split = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let split = split?;
    let (head, value) = (&line[..split], &line[split + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts.filter_map(|p| {
        let (k, v) = p.split_once('=')?;
        Some((k.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string()))
    }).collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A time as the calendar wrote it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum When {
    Utc(NaiveDateTime),
    Zoned(NaiveDateTime, chrono_tz::Tz),
    Floating(NaiveDateTime),
    Day(NaiveDate),
}

/// Windows zone names, as Outlook writes them in its .ics, → IANA. The common ones.
fn windows_zone(name: &str) -> Option<&'static str> {
    Some(match name {
        "Romance Standard Time" => "Europe/Paris",
        "W. Europe Standard Time" => "Europe/Zurich",
        "Central Europe Standard Time" => "Europe/Budapest",
        "Central European Standard Time" => "Europe/Warsaw",
        "GMT Standard Time" => "Europe/London",
        "Greenwich Standard Time" => "Atlantic/Reykjavik",
        "E. Europe Standard Time" => "Europe/Chisinau",
        "FLE Standard Time" => "Europe/Kiev",
        "GTB Standard Time" => "Europe/Bucharest",
        "Russian Standard Time" => "Europe/Moscow",
        "Turkey Standard Time" => "Europe/Istanbul",
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "Pacific Standard Time" => "America/Los_Angeles",
        "India Standard Time" => "Asia/Kolkata",
        "China Standard Time" => "Asia/Shanghai",
        "Tokyo Standard Time" => "Asia/Tokyo",
        "Korea Standard Time" => "Asia/Seoul",
        "AUS Eastern Standard Time" => "Australia/Sydney",
        "UTC" | "Coordinated Universal Time" => "UTC",
        _ => return None,
    })
}

fn zone(tzid: &str) -> Option<chrono_tz::Tz> {
    let tzid = tzid.trim().trim_start_matches('/');
    tzid.parse::<chrono_tz::Tz>().ok().or_else(|| windows_zone(tzid)?.parse().ok())
}

fn parse_when(prop: &Prop) -> Option<When> {
    let v = prop.value.trim();
    if prop.param("VALUE").is_some_and(|x| x.eq_ignore_ascii_case("DATE")) || (v.len() == 8 && !v.contains('T')) {
        return NaiveDate::parse_from_str(&v[..8.min(v.len())], "%Y%m%d").ok().map(When::Day);
    }
    let utc = v.ends_with('Z');
    let naive = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    if utc {
        return Some(When::Utc(naive));
    }
    match prop.param("TZID").and_then(zone) {
        Some(tz) => Some(When::Zoned(naive, tz)),
        None => Some(When::Floating(naive)),
    }
}

fn to_utc(when: When) -> Option<DateTime<Utc>> {
    match when {
        When::Utc(n) => Some(Utc.from_utc_datetime(&n)),
        When::Zoned(n, tz) => tz.from_local_datetime(&n).earliest().map(|d| d.with_timezone(&Utc)),
        When::Floating(n) => Local.from_local_datetime(&n).earliest().map(|d| d.with_timezone(&Utc)),
        When::Day(d) => Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest().map(|x| x.with_timezone(&Utc)),
    }
}

/// The same time for the recurrence rules, in its own zone.
fn to_rrule(when: When) -> Option<DateTime<rrule::Tz>> {
    match when {
        When::Utc(n) => Some(rrule::Tz::UTC.from_utc_datetime(&n)),
        When::Zoned(n, tz) => rrule::Tz::Tz(tz).from_local_datetime(&n).earliest(),
        When::Floating(n) => rrule::Tz::LOCAL.from_local_datetime(&n).earliest(),
        When::Day(d) => rrule::Tz::LOCAL.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest(),
    }
}

/// "PT1H30M", "P1D", "-PT15M" → a duration.
fn parse_duration(text: &str) -> Option<ChronoDuration> {
    let t = text.trim();
    let (sign, t) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    let t = t.strip_prefix('P')?;
    let mut secs: i64 = 0;
    let mut number = String::new();
    let mut in_time = false;
    for c in t.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => number.push(c),
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                secs += n * match (unit, in_time) {
                    ('W', _) => 7 * 86_400,
                    ('D', _) => 86_400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    Some(ChronoDuration::seconds(sign * secs))
}

#[derive(Debug, Default, Clone)]
struct Event {
    uid: String,
    summary: String,
    start: Option<When>,
    end: Option<When>,
    duration: Option<ChronoDuration>,
    rrule: Option<String>,
    rdates: Vec<When>,
    exdates: Vec<When>,
    recurrence_id: Option<When>,
    cancelled: bool,
    location: String,
    description: String,
    url: String,
    organizer: String,
    teams: String,
}

fn events(text: &str) -> Vec<Event> {
    let mut out = Vec::new();
    let mut current: Option<Event> = None;
    // Inside a VALARM (or anything else nested) the event's own lines pause.
    let mut nested = 0usize;
    for line in unfold(text) {
        let Some(prop) = parse_line(&line) else { continue };
        match (prop.name.as_str(), prop.value.trim().to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") => {
                current = Some(Event::default());
                nested = 0;
                continue;
            }
            ("END", "VEVENT") => {
                if let Some(e) = current.take() {
                    out.push(e);
                }
                continue;
            }
            ("BEGIN", _) if current.is_some() => {
                nested += 1;
                continue;
            }
            ("END", _) if current.is_some() => {
                nested = nested.saturating_sub(1);
                continue;
            }
            _ => {}
        }
        let Some(e) = current.as_mut() else { continue };
        if nested > 0 {
            continue;
        }
        match prop.name.as_str() {
            "UID" => e.uid = prop.value.trim().to_string(),
            "SUMMARY" => e.summary = unescape(&prop.value),
            "DTSTART" => e.start = parse_when(&prop),
            "DTEND" => e.end = parse_when(&prop),
            "DURATION" => e.duration = parse_duration(&prop.value),
            "RRULE" => e.rrule = Some(prop.value.trim().to_string()),
            "RDATE" | "EXDATE" => {
                for part in prop.value.split(',') {
                    let one = Prop { value: part.to_string(), ..prop.clone() };
                    if let Some(w) = parse_when(&one) {
                        if prop.name == "RDATE" { e.rdates.push(w) } else { e.exdates.push(w) }
                    }
                }
            }
            "RECURRENCE-ID" => e.recurrence_id = parse_when(&prop),
            "STATUS" => e.cancelled = prop.value.trim().eq_ignore_ascii_case("CANCELLED"),
            "LOCATION" => e.location = unescape(&prop.value),
            "DESCRIPTION" => e.description = unescape(&prop.value),
            "URL" => e.url = prop.value.trim().to_string(),
            "ORGANIZER" => e.organizer = prop.param("CN").map(str::to_string).unwrap_or_else(|| prop.value.trim_start_matches("mailto:").to_string()),
            "X-MICROSOFT-SKYPETEAMSMEETINGURL" | "X-GOOGLE-CONFERENCE" => e.teams = prop.value.trim().to_string(),
            _ => {}
        }
    }
    out
}

/// A meeting link in the event: its own field, else the first Teams, Meet or Zoom link in the text.
fn join_url(e: &Event) -> Option<String> {
    let candidates = [e.teams.as_str(), e.url.as_str()];
    for c in candidates {
        if c.starts_with("https://") && is_meeting(c) {
            return Some(c.to_string());
        }
    }
    for text in [&e.location, &e.description] {
        for word in text.split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"') {
            if word.starts_with("https://") && is_meeting(word) {
                return Some(word.trim_end_matches([')', '.', ',']).to_string());
            }
        }
    }
    None
}

fn is_meeting(url: &str) -> bool {
    ["teams.microsoft.com/", "teams.live.com/", "meet.google.com/", "zoom.us/j/", ".zoom.us/j/", "webex.com/"].iter().any(|h| url.contains(h))
}

/// Every occurrence between `from` and `until` (Unix ms), as calendar365 events, ids prefixed with `calendar`.
pub fn read(text: &str, calendar: &str, from: i64, until: i64) -> Vec<Value> {
    let all = events(text);
    let from_utc = Utc.timestamp_millis_opt(from).single().unwrap_or_else(Utc::now);
    let until_utc = Utc.timestamp_millis_opt(until).single().unwrap_or_else(Utc::now);
    // Moved or changed occurrences: replace the series' occurrence at their RECURRENCE-ID.
    let moved: Vec<(String, i64)> = all.iter().filter_map(|e| {
        Some((e.uid.clone(), to_utc(e.recurrence_id?)?.timestamp_millis()))
    }).collect();
    let mut out = Vec::new();
    for e in &all {
        let Some(start) = e.start else { continue };
        let Some(start_utc) = to_utc(start) else { continue };
        let all_day = matches!(start, When::Day(_));
        let length = match (e.end.and_then(to_utc), e.duration) {
            (Some(end), _) if end >= start_utc => end - start_utc,
            (_, Some(d)) if d >= ChronoDuration::zero() => d,
            _ if all_day => ChronoDuration::days(1),
            _ => ChronoDuration::zero(),
        };
        let mut starts: Vec<DateTime<Utc>> = Vec::new();
        match (&e.rrule, e.recurrence_id) {
            (Some(rule), None) => {
                let set = to_rrule(start).and_then(|dt| {
                    let rule: rrule::RRule<rrule::Unvalidated> = rule.parse().ok()?;
                    let mut set = rule.build(dt).ok()?;
                    for x in e.exdates.iter().filter_map(|w| to_rrule(*w)) {
                        set = set.exdate(x);
                    }
                    for x in e.rdates.iter().filter_map(|w| to_rrule(*w)) {
                        set = set.rdate(x);
                    }
                    Some(set)
                });
                match set {
                    Some(set) => {
                        let after = (from_utc - length - ChronoDuration::seconds(1)).with_timezone(&rrule::Tz::UTC);
                        let before = until_utc.with_timezone(&rrule::Tz::UTC);
                        starts.extend(set.after(after).before(before).all(MAX_OCCURRENCES).dates.into_iter().map(|d| d.with_timezone(&Utc)));
                    }
                    // A rule this reader cannot follow: at least the first occurrence.
                    None => starts.push(start_utc),
                }
            }
            _ => starts.push(start_utc),
        }
        for s in starts {
            let ms = s.timestamp_millis();
            // This occurrence was moved: the moved one stands for it.
            if e.recurrence_id.is_none() && moved.iter().any(|(uid, at)| *uid == e.uid && *at == ms) {
                continue;
            }
            if e.cancelled {
                continue;
            }
            let end = s + length;
            if end.timestamp_millis() < from || s.timestamp_millis() > until {
                continue;
            }
            out.push(json!({
                "id": format!("{calendar}:{}:{ms}", e.uid),
                "calendarId": calendar,
                "title": e.summary,
                "start": s.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                "end": end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                "allDay": all_day,
                "attendeeName": e.organizer,
                "attendeeEmail": "",
                "attendeeNotes": e.location,
                "joinUrl": join_url(e).unwrap_or_default(),
                "webUrl": if e.url.starts_with("https://") && !is_meeting(&e.url) { e.url.clone() } else { String::new() },
            }));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
BEGIN:VTIMEZONE\r\nTZID:Romance Standard Time\r\nBEGIN:STANDARD\r\nTZOFFSETTO:+0100\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:weekly\r\nSUMMARY:Daily\\, équipe\r\nDTSTART;TZID=Romance Standard Time:20261116T090000\r\nDTEND;TZID=Romance Standard Time:20261116T091500\r\n\
RRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nEXDATE;TZID=Romance Standard Time:20261118T090000\r\n\
DESCRIPTION:Rejoindre : https://teams.microsoft.com/l/meetup-join/abc\\n\r\n\
BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nDESCRIPTION:ignored\r\nEND:VALARM\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:weekly\r\nRECURRENCE-ID;TZID=Romance Standard Time:20261123T090000\r\nSUMMARY:Daily (moved)\r\n\
DTSTART;TZID=Romance Standard Time:20261123T110000\r\nDTEND;TZID=Romance Standard Time:20261123T111500\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:day\r\nSUMMARY:Férié\r\nDTSTART;VALUE=DATE:20261125\r\nDTEND;VALUE=DATE:20261126\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:gone\r\nSUMMARY:Annulé\r\nSTATUS:CANCELLED\r\nDTSTART:20261120T100000Z\r\nDURATION:PT1H\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:long\r\nSUMMARY:Une ligne\r\n  pliée\r\nDTSTART:20261120T130000Z\r\nDURATION:PT1H30M\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    #[test]
    fn addresses_are_https_only() {
        assert_eq!(normalize_url("webcal://p01-caldav.icloud.com/x.ics").as_deref(), Some("https://p01-caldav.icloud.com/x.ics"));
        assert_eq!(normalize_url(" https://calendar.google.com/basic.ics ").as_deref(), Some("https://calendar.google.com/basic.ics"));
        assert!(normalize_url("http://example.com/a.ics").is_none());
        assert!(normalize_url("file:///etc/passwd").is_none());
    }

    #[test]
    fn events_recurrences_and_exceptions_are_read() {
        let got = read(ICS, "ical-0", ms("2026-11-15T00:00:00Z"), ms("2026-11-30T00:00:00Z"));
        let titles: Vec<(String, String)> = got.iter().map(|e| (e["title"].as_str().unwrap().to_string(), e["start"].as_str().unwrap().to_string())).collect();
        // Mondays and Wednesdays at 9:00 Paris (8:00 UTC), minus the 18th, the 23rd moved to 11:00.
        assert!(titles.contains(&("Daily, équipe".into(), "2026-11-16T08:00:00Z".into())));
        assert!(!titles.iter().any(|(_, s)| s == "2026-11-18T08:00:00Z"));
        assert!(titles.contains(&("Daily, équipe".into(), "2026-11-25T08:00:00Z".into())));
        assert!(!titles.iter().any(|(_, s)| s == "2026-11-23T08:00:00Z"));
        assert!(titles.contains(&("Daily (moved)".into(), "2026-11-23T10:00:00Z".into())));
        assert!(!titles.iter().any(|(t, _)| t == "Annulé"));
        assert!(titles.iter().any(|(t, _)| t == "Une ligne pliée"));
        let daily = got.iter().find(|e| e["start"] == "2026-11-16T08:00:00Z").unwrap();
        assert_eq!(daily["joinUrl"], "https://teams.microsoft.com/l/meetup-join/abc");
        assert_eq!(daily["end"], "2026-11-16T08:15:00Z");
        assert_eq!(daily["calendarId"], "ical-0");
        let long = got.iter().find(|e| e["title"] == "Une ligne pliée").unwrap();
        assert_eq!(long["end"], "2026-11-20T14:30:00Z");
        let day = got.iter().find(|e| e["title"] == "Férié").unwrap();
        assert_eq!(day["allDay"], true);
    }

    #[test]
    fn durations_are_read() {
        assert_eq!(parse_duration("PT1H30M"), Some(ChronoDuration::minutes(90)));
        assert_eq!(parse_duration("P1D"), Some(ChronoDuration::days(1)));
        assert_eq!(parse_duration("-PT15M"), Some(ChronoDuration::minutes(-15)));
        assert_eq!(parse_duration("1H"), None);
    }
}
