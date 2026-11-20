//! The Music tab: whatever plays on this computer — the Spotify app, YouTube Music
//! in a browser or as an app, Amazon Music — read and driven through the system's
//! media controls, with no account. Windows: the system media transport controls
//! (the overlay the media keys show). Linux: MPRIS, every `org.mpris.MediaPlayer2.*`
//! on the session bus.
//!
//! Nothing runs on its own: the Music space asks for the state while it is on
//! screen (src/views/music.ts), and the controls are clicks.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// The session's id: the Windows app id (AUMID) or the MPRIS bus name.
    pub id: String,
    /// "Spotify", "YouTube Music", "Amazon Music", "Edge"…
    pub name: String,
    pub playing: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub source: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Seconds; 0 when the player does not say.
    pub position: f64,
    pub duration: f64,
    /// Changes with the cover: the page asks for it (media_art) only then.
    pub art_key: String,
    pub can_previous: bool,
    pub can_next: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaState {
    pub sources: Vec<Source>,
    pub now: Option<NowPlaying>,
}

/// A friendly name for a media session's id.
pub fn source_name(id: &str, identity: Option<&str>) -> String {
    let low = id.to_ascii_lowercase();
    // YouTube Music installed as an app from Chrome or Edge (its extension id), or its site.
    if low.contains("cinhimbnkkaeohfgghhklpknlkffjgod") || low.contains("music.youtube") || low.contains("youtubemusic") || low.contains("youtube-music") {
        return "YouTube Music".into();
    }
    if low.contains("spotify") {
        return "Spotify".into();
    }
    if low.contains("amazon") {
        return "Amazon Music".into();
    }
    if let Some(name) = identity.map(str::trim).filter(|s| !s.is_empty()) {
        return name.to_string();
    }
    for (needle, name) in [("msedge", "Edge"), ("chrome", "Chrome"), ("chromium", "Chromium"), ("firefox", "Firefox"), ("brave", "Brave"), ("opera", "Opera"), ("vivaldi", "Vivaldi"), ("vlc", "VLC")] {
        if low.contains(needle) {
            return name.into();
        }
    }
    // "org.mpris.MediaPlayer2.rhythmbox" → "rhythmbox"; "Foo.exe" → "Foo".
    let last = id.rsplit(['.', '!', '\\', '/']).find(|s| !s.is_empty() && !s.eq_ignore_ascii_case("exe")).unwrap_or(id);
    let last = last.split('_').next().unwrap_or(last);
    let mut chars = last.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// The source shown: the one asked for while it exists, else the one playing, else the system's current one.
pub fn pick<'a>(sources: &'a [Source], wanted: Option<&str>, current: Option<&str>) -> Option<&'a Source> {
    wanted.and_then(|w| sources.iter().find(|s| s.id == w))
        .or_else(|| sources.iter().find(|s| s.playing))
        .or_else(|| current.and_then(|c| sources.iter().find(|s| s.id == c)))
        .or_else(|| sources.first())
}

fn art_key(title: &str, artist: &str, album: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in [artist, album, title].join("\u{1f}").bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn data_url(bytes: &[u8]) -> Option<String> {
    let mime = crate::spotify::sniff_image(bytes)?;
    Some(format!("data:{mime};base64,{}", crate::claude::base64_for(bytes)))
}

/// The biggest cover read.
const ART_LIMIT: u64 = 3 * 1024 * 1024;

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn media_state(source: Option<String>) -> MediaState {
    tauri::async_runtime::spawn_blocking(move || platform::state(source.as_deref()))
        .await
        .unwrap_or_default()
}

/// "playPause", "next" or "previous" on `source`.
#[tauri::command]
pub async fn media_control(source: String, action: String) -> bool {
    if !matches!(action.as_str(), "playPause" | "next" | "previous") {
        return false;
    }
    tauri::async_runtime::spawn_blocking(move || platform::control(&source, &action))
        .await
        .unwrap_or(false)
}

/// The cover of what `source` plays, as a data URL.
#[tauri::command]
pub async fn media_art(source: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || platform::art(&source)).await.ok().flatten()
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Storage::Streams::DataReader;

    fn manager() -> Option<Manager> {
        Manager::RequestAsync().ok()?.get().ok()
    }

    fn sessions(manager: &Manager) -> Vec<Session> {
        let Ok(list) = manager.GetSessions() else { return Vec::new() };
        (0..list.Size().unwrap_or(0)).filter_map(|i| list.GetAt(i).ok()).collect()
    }

    fn id_of(s: &Session) -> String {
        s.SourceAppUserModelId().map(|h| h.to_string_lossy()).unwrap_or_default()
    }

    fn playing(s: &Session) -> bool {
        s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|st| st == Status::Playing)
    }

    fn find(manager: &Manager, id: &str) -> Option<Session> {
        sessions(manager).into_iter().find(|s| id_of(s) == id)
    }

    pub fn state(wanted: Option<&str>) -> MediaState {
        let Some(manager) = manager() else { return MediaState::default() };
        let all = sessions(&manager);
        let sources: Vec<Source> = all.iter().map(|s| {
            let id = id_of(s);
            Source { name: source_name(&id, None), playing: playing(s), id }
        }).collect();
        let current = manager.GetCurrentSession().ok().map(|s| id_of(&s));
        let Some(chosen) = pick(&sources, wanted, current.as_deref()).cloned() else {
            return MediaState { sources, now: None };
        };
        let now = all.iter().find(|s| id_of(s) == chosen.id).and_then(|s| now_playing(s, &chosen).ok());
        MediaState { sources, now }
    }

    fn now_playing(s: &Session, source: &Source) -> windows::core::Result<NowPlaying> {
        let props = s.TryGetMediaPropertiesAsync()?.get()?;
        let timeline = s.GetTimelineProperties()?;
        let start = timeline.StartTime()?.Duration;
        let ticks = |t: i64| (t - start).max(0) as f64 / 10_000_000.0;
        let controls = s.GetPlaybackInfo()?.Controls()?;
        let title = props.Title()?.to_string_lossy();
        let artist = props.Artist()?.to_string_lossy();
        let album = props.AlbumTitle()?.to_string_lossy();
        // Windows only moves the position when the player says so: run it on from its last update.
        let mut position = ticks(timeline.Position()?.Duration);
        let duration = ticks(timeline.EndTime()?.Duration);
        if source.playing {
            let updated = timeline.LastUpdatedTime()?.UniversalTime;
            if let Some(at) = crate::spotify::windows_time_to_unix_ms(updated) {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(at);
                if now > at {
                    position += (now - at) / 1000.0;
                }
            }
        }
        if duration > 0.0 {
            position = position.min(duration);
        }
        Ok(NowPlaying {
            source: source.id.clone(),
            art_key: art_key(&title, &artist, &album),
            title,
            artist,
            album,
            playing: source.playing,
            position,
            duration,
            can_previous: controls.IsPreviousEnabled().unwrap_or(true),
            can_next: controls.IsNextEnabled().unwrap_or(true),
        })
    }

    pub fn control(id: &str, action: &str) -> bool {
        let Some(s) = manager().and_then(|m| find(&m, id)) else { return false };
        let op = match action {
            "playPause" => s.TryTogglePlayPauseAsync(),
            "next" => s.TrySkipNextAsync(),
            _ => s.TrySkipPreviousAsync(),
        };
        op.and_then(|o| o.get()).unwrap_or(false)
    }

    pub fn art(id: &str) -> Option<String> {
        let s = manager().and_then(|m| find(&m, id))?;
        let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
        let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()?;
        if size == 0 || size > ART_LIMIT {
            return None;
        }
        let reader = DataReader::CreateDataReader(&stream).ok()?;
        let loaded = reader.LoadAsync(size as u32).ok()?.get().ok()?;
        let mut bytes = vec![0u8; loaded as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        data_url(&bytes)
    }
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use dbus::arg::RefArg;
    use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
    use dbus::blocking::Connection;
    use std::time::Duration;

    const PATH: &str = "/org/mpris/MediaPlayer2";
    const ROOT: &str = "org.mpris.MediaPlayer2";
    const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
    const PREFIX: &str = "org.mpris.MediaPlayer2.";
    /// A player never holds anything up for longer.
    const CALL: Duration = Duration::from_millis(800);

    fn players(conn: &Connection) -> Vec<String> {
        let proxy = conn.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", CALL);
        let names: Result<(Vec<String>,), _> = proxy.method_call("org.freedesktop.DBus", "ListNames", ());
        let mut names: Vec<String> = names.map(|(n,)| n).unwrap_or_default().into_iter().filter(|n| n.starts_with(PREFIX)).collect();
        names.sort();
        names
    }

    fn text(v: &dyn RefArg) -> String {
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        // xesam:artist is a list of strings; variants are unwrapped.
        v.as_iter().map(|it| it.map(text).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
    }

    fn get_str(conn: &Connection, name: &str, iface: &str, prop: &str) -> Option<String> {
        let proxy = conn.with_proxy(name, PATH, CALL);
        let v: dbus::arg::Variant<Box<dyn RefArg>> = proxy.get(iface, prop).ok()?;
        Some(text(&v.0))
    }

    fn get_bool(conn: &Connection, name: &str, prop: &str) -> Option<bool> {
        let proxy = conn.with_proxy(name, PATH, CALL);
        proxy.get::<bool>(PLAYER, prop).ok()
    }

    fn get_i64(conn: &Connection, name: &str, prop: &str) -> Option<i64> {
        let proxy = conn.with_proxy(name, PATH, CALL);
        let v: dbus::arg::Variant<Box<dyn RefArg>> = proxy.get(PLAYER, prop).ok()?;
        v.0.as_i64()
    }

    fn metadata(conn: &Connection, name: &str) -> Vec<(String, String, Option<i64>)> {
        let proxy = conn.with_proxy(name, PATH, CALL);
        let Ok(map) = proxy.get::<dbus::arg::PropMap>(PLAYER, "Metadata") else { return Vec::new() };
        map.iter().map(|(k, v)| {
            let inner = v.0.as_iter().and_then(|mut it| it.next().map(|x| x.as_i64())).flatten();
            (k.clone(), text(&v.0), v.0.as_i64().or(inner))
        }).collect()
    }

    pub fn state(wanted: Option<&str>) -> MediaState {
        let Ok(conn) = Connection::new_session() else { return MediaState::default() };
        let sources: Vec<Source> = players(&conn).into_iter().map(|id| {
            let identity = get_str(&conn, &id, ROOT, "Identity");
            let playing = get_str(&conn, &id, PLAYER, "PlaybackStatus").as_deref() == Some("Playing");
            Source { name: source_name(&id, identity.as_deref()), playing, id }
        }).collect();
        let Some(chosen) = pick(&sources, wanted, None).cloned() else { return MediaState { sources, now: None } };
        let meta = metadata(&conn, &chosen.id);
        let field = |k: &str| meta.iter().find(|(key, _, _)| key == k).map(|(_, v, _)| v.clone()).unwrap_or_default();
        let length = meta.iter().find(|(key, _, _)| key == "mpris:length").and_then(|(_, _, n)| *n).unwrap_or(0);
        let (title, artist, album) = (field("xesam:title"), field("xesam:artist"), field("xesam:album"));
        let now = NowPlaying {
            source: chosen.id.clone(),
            art_key: art_key(&title, &artist, &album),
            title,
            artist,
            album,
            playing: chosen.playing,
            position: get_i64(&conn, &chosen.id, "Position").unwrap_or(0).max(0) as f64 / 1e6,
            duration: length.max(0) as f64 / 1e6,
            can_previous: get_bool(&conn, &chosen.id, "CanGoPrevious").unwrap_or(true),
            can_next: get_bool(&conn, &chosen.id, "CanGoNext").unwrap_or(true),
        };
        MediaState { sources, now: Some(now) }
    }

    pub fn control(id: &str, action: &str) -> bool {
        if !id.starts_with(PREFIX) {
            return false;
        }
        let Ok(conn) = Connection::new_session() else { return false };
        let method = match action {
            "playPause" => "PlayPause",
            "next" => "Next",
            _ => "Previous",
        };
        conn.with_proxy(id, PATH, CALL).method_call::<(), _, _, _>(PLAYER, method, ()).is_ok()
    }

    /// Only a cover on this computer (file://): Ronron does not fetch the players' remote images.
    pub fn art(id: &str) -> Option<String> {
        if !id.starts_with(PREFIX) {
            return None;
        }
        let conn = Connection::new_session().ok()?;
        let url = metadata(&conn, id).into_iter().find(|(k, _, _)| k == "mpris:artUrl").map(|(_, v, _)| v)?;
        let path = url.strip_prefix("file://")?;
        let path = percent_decode(path);
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() > ART_LIMIT {
            return None;
        }
        data_url(&std::fs::read(path).ok()?)
    }

    fn percent_decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub fn state(_: Option<&str>) -> MediaState {
        MediaState::default()
    }
    pub fn control(_: &str, _: &str) -> bool {
        false
    }
    pub fn art(_: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_get_their_names() {
        assert_eq!(source_name("Spotify.exe", None), "Spotify");
        assert_eq!(source_name("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", None), "Spotify");
        assert_eq!(source_name("Chrome._crx_cinhimbnkkaeohfgghhklpknlkffjgod", None), "YouTube Music");
        assert_eq!(source_name("AmazonMobileLLC.AmazonMusic_kc6t79cpj4tp0!AmazonMobileLLC.AmazonMusic", None), "Amazon Music");
        assert_eq!(source_name("MSEdge", None), "Edge");
        assert_eq!(source_name("org.mpris.MediaPlayer2.chromium.instance123", Some("Chromium")), "Chromium");
        assert_eq!(source_name("org.mpris.MediaPlayer2.rhythmbox", None), "Rhythmbox");
    }

    #[test]
    fn the_source_asked_for_wins_then_the_playing_one() {
        let s = |id: &str, playing| Source { id: id.into(), name: id.into(), playing };
        let list = vec![s("a", false), s("b", true), s("c", false)];
        assert_eq!(pick(&list, Some("c"), None).unwrap().id, "c");
        assert_eq!(pick(&list, Some("gone"), None).unwrap().id, "b");
        let quiet = vec![s("a", false), s("c", false)];
        assert_eq!(pick(&quiet, None, Some("c")).unwrap().id, "c");
        assert_eq!(pick(&quiet, None, None).unwrap().id, "a");
        assert!(pick(&[], None, None).is_none());
    }

    #[test]
    fn the_cover_key_follows_the_track() {
        assert_eq!(art_key("A", "B", "C"), art_key("A", "B", "C"));
        assert_ne!(art_key("A", "B", "C"), art_key("A2", "B", "C"));
    }
}
