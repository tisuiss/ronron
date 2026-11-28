// « OK Ronron » on Windows and Linux: say it, then ask anything. Port of the
// Mac's Voice/ (VoiceEngine.swift and its friends).
//
// While it is on, the microphone is open and a cheap energy detector cuts what
// you say into stretches; only a stretch of speech goes to Parakeet, on this
// computer, and only a stretch that starts with « OK Ronron » goes further:
// what you said after it goes to the model picked on the voice page — Claude
// with your own key, Gemini, Ollama… (brain.rs, llm.rs) — never the audio. Ronron shows it all in the pill at the bottom of the screen —
// listening, thinking, then the answer, said out loud by the system's voice
// (the pill's speechSynthesis) or ElevenLabs with your key.
//
// It stops listening while you dictate, while a call is recorded and while it
// speaks itself.

pub mod brain;
pub mod llm;
pub mod wake;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::flow::pill;
use crate::speech::{self, audio};

/// What the pill shows of a conversation.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceView {
    /// "listening", "thinking", "speaking".
    pub phase: String,
    /// What you said (after « OK Ronron »).
    pub heard: String,
    /// Ronron's answer.
    pub answer: String,
}

struct Running {
    stop: Arc<AtomicBool>,
    /// Started by the island's microphone button for one question, with « OK Ronron » off.
    one_shot: bool,
}

/// The island's microphone button, while Ronron listens anyway: the next words are the question.
static ASK: AtomicBool = AtomicBool::new(false);
/// Voice → Test is on: the listening loop leaves the microphone to it.
static TESTING: AtomicBool = AtomicBool::new(false);

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// Rust waits on these for the island's answers and the end of speech.
fn pending() -> &'static Mutex<HashMap<u64, mpsc::Sender<String>>> {
    static P: OnceLock<Mutex<HashMap<u64, mpsc::Sender<String>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The model that answers, as the voice page and the chat settings say, or
/// what is missing for it.
pub fn backend(app: &AppHandle) -> Result<llm::Backend, String> {
    let stored = crate::flow::get(app).map(|f| f.store.lock().unwrap().prefs.voice_provider.clone()).unwrap_or_default();
    let settings = app
        .try_state::<crate::Shared>()
        .map(|s| s.settings.lock().unwrap().clone())
        .unwrap_or_default();
    llm::backend(&settings, &stored)
}

/// Whether « OK Ronron » can run: the voice model and a model to answer.
pub fn requirements_met(app: &AppHandle) -> bool {
    speech::is_ready() && backend(app).is_ok()
}

/// Starts or stops listening to follow the preference.
pub fn sync(app: &AppHandle) {
    let wanted = crate::flow::get(app).is_some_and(|f| f.store.lock().unwrap().prefs.voice);
    let mut running = RUNNING.lock().unwrap();
    // A one-question run (the island's button) finishes on its own, then calls sync again.
    if running.as_ref().is_some_and(|r| r.one_shot) {
        return;
    }
    match (wanted && requirements_met(app), running.is_some()) {
        (true, false) => {
            let stop = Arc::new(AtomicBool::new(false));
            let app2 = app.clone();
            let stop2 = stop.clone();
            std::thread::Builder::new().name("ronron-voice".into()).spawn(move || run(app2, stop2, false)).ok();
            *running = Some(Running { stop, one_shot: false });
            speech::preload();
            crate::log::line("voice: « OK Ronron » listening");
        }
        (false, true) => {
            if let Some(r) = running.take() {
                r.stop.store(true, Ordering::Relaxed);
            }
            let dictation = crate::flow::get(app).is_some_and(|f| f.store.lock().unwrap().prefs.enabled);
            if !dictation && !crate::notes::is_recording(app) {
                speech::unload();
            }
            crate::log::line("voice: « OK Ronron » off");
        }
        _ => {}
    }
}

/// « OK Ronron » is listening (the voice model stays loaded).
pub fn is_listening() -> bool {
    RUNNING.lock().unwrap().is_some()
}

/// The island's microphone button: Ronron listens to a question now, no
/// « OK Ronron » needed — through the listening already running, or for this
/// one question when « OK Ronron » is off. What is missing shows in the pill.
#[tauri::command]
pub fn voice_ask(app: AppHandle) -> Result<(), String> {
    let problem = if !speech::is_ready() {
        Some(crate::i18n::t("Download the voice model so Ronron can hear you."))
    } else {
        backend(&app).err()
    };
    if let Some(err) = problem {
        show(&app, Some(VoiceView { phase: "speaking".into(), heard: String::new(), answer: err.clone() }));
        let app2 = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(6));
            show(&app2, None);
        });
        return Err(err);
    }
    let mut running = RUNNING.lock().unwrap();
    if running.is_some() {
        ASK.store(true, Ordering::Relaxed);
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let app2 = app.clone();
    let stop2 = stop.clone();
    std::thread::Builder::new().name("ronron-voice".into()).spawn(move || run(app2, stop2, true)).ok();
    *running = Some(Running { stop, one_shot: true });
    speech::preload();
    Ok(())
}

/// Starts again on another microphone.
pub fn restart(app: &AppHandle) {
    if let Some(r) = RUNNING.lock().unwrap().take() {
        r.stop.store(true, Ordering::Relaxed);
    }
    sync(app);
}

fn show(app: &AppHandle, view: Option<VoiceView>) {
    pill::update(app, |s| s.voice = view);
}

enum Mode {
    /// Waiting for « OK Ronron ».
    Wake,
    /// Heard « OK Ronron » alone: what comes next is the question.
    Command { text: String, until: Instant },
    /// Ronron asked something back: an answer comes without « OK Ronron ».
    FollowUp { until: Instant },
}

fn run(app: AppHandle, stop: Arc<AtomicBool>, one_shot: bool) {
    let mic = crate::flow::get(&app).map(|f| f.store.lock().unwrap().prefs.mic.clone()).unwrap_or_default();
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let capture = audio::Capture::start(audio::Source::Mic(if mic.is_empty() { None } else { Some(mic) }), move |s| {
        let _ = tx.send(s.to_vec());
    });
    let mut capture = match capture {
        Ok(c) => c,
        Err(err) => {
            crate::log::line(format!("voice: microphone: {err}"));
            *RUNNING.lock().unwrap() = None;
            return;
        }
    };
    let mut vad = wake::Vad::default();
    let mut mode = Mode::Wake;
    if one_shot {
        ASK.store(true, Ordering::Relaxed);
    }
    let mut brain = brain::Brain::default();
    let mut hide_at: Option<Instant> = None;
    // Silence for the question that follows « OK Ronron » (flow/hush.rs).
    let mut hush: Option<crate::flow::hush::Guard> = None;
    let quiet = || crate::flow::get(&app).is_some_and(|f| f.store.lock().unwrap().prefs.mute_others);
    while !stop.load(Ordering::Relaxed) {
        // The island's microphone button: what comes next is the question.
        if ASK.swap(false, Ordering::Relaxed) {
            vad.reset();
            vad.hangover = 40;
            hide_at = None;
            mode = Mode::Command { text: String::new(), until: Instant::now() + Duration::from_secs(8) };
            show(&app, Some(VoiceView { phase: "listening".into(), heard: String::new(), answer: String::new() }));
        }
        // One question asked and answered (or nothing came): this run is over.
        if one_shot && matches!(mode, Mode::Wake) && hide_at.is_none() {
            break;
        }
        // Listening for a question or an answer: the other apps are quiet.
        let asking = matches!(mode, Mode::Command { .. } | Mode::FollowUp { .. });
        if asking && hush.is_none() {
            hush = crate::flow::hush::hush(quiet());
        } else if !asking {
            hush = None;
        }
        let chunk = match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let now = Instant::now();
        // The pill's last answer goes away after a while.
        if hide_at.is_some_and(|t| now > t) {
            hide_at = None;
            show(&app, None);
        }
        match &mode {
            Mode::Command { until, .. } | Mode::FollowUp { until } if now > *until => {
                mode = Mode::Wake;
                vad.hangover = 25;
                show(&app, None);
            }
            _ => {}
        }
        // Dictation and calls have the microphone: Ronron doesn't listen meanwhile.
        if pill::dictating(&app) || crate::notes::is_recording(&app) || TESTING.load(Ordering::Relaxed) {
            vad.reset();
            continue;
        }
        let Some(chunk) = chunk else { continue };
        for segment in vad.feed(&chunk) {
            if segment.len() < speech::SAMPLE_RATE as usize * 35 / 100 {
                continue;
            }
            let Some(heard) = speech::transcribe(&segment).map(|t| t.text.trim().to_string()) else { continue };
            if heard.is_empty() {
                continue;
            }
            let question = match &mut mode {
                Mode::Wake => match wake::after_wake(&heard) {
                    Some(rest) => {
                        crate::log::line("voice: « OK Ronron » heard");
                        hide_at = None;
                        if crate::flow::text::word_count(&rest) >= 2 {
                            Some(rest)
                        } else {
                            show(&app, Some(VoiceView { phase: "listening".into(), heard: rest.clone(), answer: String::new() }));
                            mode = Mode::Command { text: rest, until: now + Duration::from_secs(7) };
                            // A question may come with pauses in it.
                            vad.hangover = 40;
                            None
                        }
                    }
                    None => None,
                },
                Mode::Command { text, .. } => {
                    let full = format!("{text} {heard}").trim().to_string();
                    Some(full)
                }
                Mode::FollowUp { .. } => Some(heard),
            };
            let Some(question) = question else { continue };
            mode = Mode::Wake;
            vad.hangover = 25;
            show(&app, Some(VoiceView { phase: "thinking".into(), heard: question.clone(), answer: String::new() }));
            let reply = brain.respond(&app, &question);
            let answer = reply.as_ref().map(|r| r.text.clone()).unwrap_or_default();
            if answer.is_empty() {
                let done = reply.is_some_and(|r| r.acted);
                show(&app, Some(VoiceView {
                    phase: "speaking".into(),
                    heard: question,
                    answer: if done {
                        "✓".into()
                    } else if let Some(err) = brain.error.take() {
                        let short: String = err.chars().take(180).collect();
                        crate::i18n::tf("Ronron couldn't answer: {error}", &[("error", &short)])
                    } else {
                        crate::i18n::t("Ronron couldn't answer: check the model picked in Voice and its key in AI agents → Chat.")
                    },
                }));
                hide_at = Some(Instant::now() + Duration::from_secs(5));
                continue;
            }
            show(&app, Some(VoiceView { phase: "speaking".into(), heard: question.clone(), answer: answer.clone() }));
            speak(&app, &answer);
            vad.reset();
            // Drop what the microphone heard of Ronron's own voice.
            while rx.try_recv().is_ok() {}
            if answer.trim_end().ends_with('?') {
                mode = Mode::FollowUp { until: Instant::now() + Duration::from_secs(8) };
                vad.hangover = 40;
                show(&app, Some(VoiceView { phase: "listening".into(), heard: String::new(), answer }));
            } else {
                hide_at = Some(Instant::now() + Duration::from_secs(6));
            }
        }
    }
    capture.stop();
    show(&app, None);
    if one_shot {
        {
            let mut running = RUNNING.lock().unwrap();
            if running.as_ref().is_some_and(|r| Arc::ptr_eq(&r.stop, &stop)) {
                *running = None;
            }
        }
        // « OK Ronron » may have been turned on meanwhile; else the model can go.
        sync(&app);
        let keep = crate::flow::get(&app).is_some_and(|f| {
            let p = &f.store.lock().unwrap().prefs;
            p.enabled || p.voice
        });
        if !keep && !crate::notes::is_recording(&app) {
            speech::unload();
        }
    }
}

// ── Voice → Test ──────────────────────────────────────────────────────────────

/// What the test shows: the microphone's loudness against the speech threshold,
/// then what Parakeet understood and whether it counts as « OK Ronron ».
/// Nothing of it is kept: it goes to the window, never to disk.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TestView {
    pub level: f32,
    pub threshold: f32,
    pub speaking: bool,
    pub heard: Option<String>,
    pub wake: Option<bool>,
    pub done: bool,
    pub error: Option<String>,
}

static TEST_STOP: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
const TEST_FOR: Duration = Duration::from_secs(30);

/// Voice → Test: 30 seconds of the microphone (`voice-test` events), then it stops.
#[tauri::command]
pub fn voice_test_start(app: AppHandle) -> Result<(), String> {
    if !speech::is_ready() {
        return Err(crate::i18n::t("Download the voice model so Ronron can hear you."));
    }
    let stop = Arc::new(AtomicBool::new(false));
    if let Some(old) = TEST_STOP.lock().unwrap().replace(stop.clone()) {
        old.store(true, Ordering::Relaxed);
    }
    speech::preload();
    std::thread::Builder::new().name("ronron-voice-test".into()).spawn(move || test_run(app, stop)).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn voice_test_stop() {
    if let Some(stop) = TEST_STOP.lock().unwrap().take() {
        stop.store(true, Ordering::Relaxed);
    }
}

fn test_run(app: AppHandle, stop: Arc<AtomicBool>) {
    let emit = |v: TestView| {
        let _ = app.emit("voice-test", v);
    };
    let mic = crate::flow::get(&app).map(|f| f.store.lock().unwrap().prefs.mic.clone()).unwrap_or_default();
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let capture = audio::Capture::start(audio::Source::Mic(if mic.is_empty() { None } else { Some(mic) }), move |s| {
        let _ = tx.send(s.to_vec());
    });
    let mut capture = match capture {
        Ok(c) => c,
        Err(err) => {
            emit(TestView { done: true, error: Some(err.to_string()), ..Default::default() });
            return;
        }
    };
    TESTING.store(true, Ordering::Relaxed);
    let mut vad = wake::Vad::default();
    let started = Instant::now();
    let mut last = Instant::now();
    while !stop.load(Ordering::Relaxed) && started.elapsed() < TEST_FOR {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) else { continue };
        for segment in vad.feed(&chunk) {
            if segment.len() < speech::SAMPLE_RATE as usize * 35 / 100 {
                continue;
            }
            let heard = speech::transcribe(&segment).map(|t| t.text.trim().to_string()).unwrap_or_default();
            let wake = !heard.is_empty() && wake::after_wake(&heard).is_some();
            emit(TestView { level: vad.level, threshold: vad.threshold(), heard: Some(heard), wake: Some(wake), ..Default::default() });
        }
        if last.elapsed() >= Duration::from_millis(100) {
            last = Instant::now();
            emit(TestView { level: vad.level, threshold: vad.threshold(), speaking: vad.in_speech(), ..Default::default() });
        }
    }
    capture.stop();
    TESTING.store(false, Ordering::Relaxed);
    {
        let mut current = TEST_STOP.lock().unwrap();
        if current.as_ref().is_some_and(|s| Arc::ptr_eq(s, &stop)) {
            *current = None;
        }
    }
    emit(TestView { done: true, ..Default::default() });
}

// ── Speaking ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Say {
    id: u64,
    text: String,
    lang: Option<&'static str>,
    /// ElevenLabs' MP3, base64.
    audio: Option<String>,
    gender: String,
}

/// Says `text` and returns once it is said (or after a sensible time).
fn speak(app: &AppHandle, text: &str) {
    let (output, gender) = crate::flow::get(app)
        .map(|f| {
            let p = &f.store.lock().unwrap().prefs;
            (p.voice_output.clone(), p.voice_gender.clone())
        })
        .unwrap_or_else(|| ("system".into(), "female".into()));
    if output == "off" {
        std::thread::sleep(Duration::from_millis(1500));
        return;
    }
    let audio = if output == "elevenlabs" { elevenlabs(text, &gender).map(|mp3| base64(&mp3)) } else { None };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    pending().lock().unwrap().insert(id, tx);
    let _ = app.emit_to(pill::LABEL, "voice-say", Say { id, text: text.into(), lang: wake::speech_lang(text), audio, gender });
    let words = crate::flow::text::word_count(text) as u64;
    let _ = rx.recv_timeout(Duration::from_millis(4000 + words * 600));
    pending().lock().unwrap().remove(&id);
}

/// The pill says it has finished speaking (`id`), or the island answers a question.
#[tauri::command]
pub fn voice_done(id: u64, text: Option<String>) {
    if let Some(tx) = pending().lock().unwrap().remove(&id) {
        let _ = tx.send(text.unwrap_or_default());
    }
}

/// Where the webview has no voice (WebKitGTK often): speech-dispatcher or
/// espeak-ng, when installed.
#[tauri::command]
pub async fn voice_say_native(text: String, lang: Option<String>) -> bool {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "linux")]
        {
            let lang = lang.unwrap_or_default();
            let short = lang.split('-').next().unwrap_or("").to_string();
            if let Some(spd) = crate::platform::find_on_path("spd-say") {
                let mut cmd = std::process::Command::new(spd);
                cmd.arg("-w");
                if !short.is_empty() {
                    cmd.args(["-l", &short]);
                }
                return cmd.arg(&text).status().is_ok_and(|s| s.success());
            }
            if let Some(espeak) = crate::platform::find_on_path("espeak-ng") {
                let mut cmd = std::process::Command::new(espeak);
                if !short.is_empty() {
                    cmd.args(["-v", &short]);
                }
                return cmd.arg(&text).status().is_ok_and(|s| s.success());
            }
            false
        }
        #[cfg(windows)]
        {
            let _ = (text, lang);
            false
        }
    })
    .await
    .unwrap_or(false)
}

/// ElevenLabs' MP3 for `text`, with the user's key; None and the system voice speaks.
fn elevenlabs(text: &str, gender: &str) -> Option<Vec<u8>> {
    let key = crate::secrets::get("elevenlabs-api-key")?;
    tauri::async_runtime::block_on(async {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().ok()?;
        let voices: Value = client.get("https://api.elevenlabs.io/v1/voices").header("xi-api-key", &key).send().await.ok()?.json().await.ok()?;
        let voice = pick_voice(voices["voices"].as_array()?, gender)?;
        let response = client
            .post(format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=mp3_44100_128"))
            .header("xi-api-key", &key)
            .header("accept", "audio/mpeg")
            .json(&json!({ "text": text, "model_id": "eleven_flash_v2_5" }))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            crate::log::line(format!("voice: ElevenLabs {}", response.status()));
            return None;
        }
        Some(response.bytes().await.ok()?.to_vec())
    })
}

/// First English voice of the gender (a young one first for a man); else any
/// voice of the gender; else the first one. Same choice as the Mac.
pub fn pick_voice(voices: &[Value], gender: &str) -> Option<String> {
    let label = |v: &Value, k: &str| v["labels"][k].as_str().unwrap_or("").to_lowercase();
    let english = |v: &Value| {
        let lang = label(v, "language");
        if !lang.is_empty() {
            return lang.starts_with("en");
        }
        let accent = label(v, "accent");
        accent.is_empty() || ["american", "british", "australian", "irish", "english", "us", "uk"].iter().any(|a| accent.contains(a))
    };
    let of_gender: Vec<&Value> = voices.iter().filter(|v| label(v, "gender") == gender).collect();
    let english_ones: Vec<&&Value> = of_gender.iter().filter(|v| english(v)).collect();
    let young = if gender == "male" { english_ones.iter().find(|v| label(v, "age").contains("young")).copied() } else { None };
    let v = young.or_else(|| english_ones.first().copied()).copied().or_else(|| of_gender.first().copied()).or_else(|| voices.first())?;
    v["voice_id"].as_str().map(String::from)
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ── The tools ─────────────────────────────────────────────────────────────────

/// Asks the island (it knows the pills and the services' data) and waits for its answer.
pub fn ask_island(app: &AppHandle, kind: &str, input: Value) -> Option<String> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    pending().lock().unwrap().insert(id, tx);
    let _ = app.emit_to(crate::island::WINDOW_LABEL, "voice-ask", json!({ "id": id, "kind": kind, "input": input }));
    let answer = rx.recv_timeout(Duration::from_secs(5)).ok();
    pending().lock().unwrap().remove(&id);
    answer
}

/// Runs one of Claude's tools: what goes back to Claude, and whether something changed.
pub fn run_tool(app: &AppHandle, name: &str, input: &Value) -> (String, bool) {
    match name {
        "pills" => {
            let answer = ask_island(app, "pills", input.clone()).unwrap_or_else(|| "the island did not answer".into());
            let acted = input["action"].as_str() != Some("list") && !answer.contains("unknown");
            (answer, acted)
        }
        "service_info" => (ask_island(app, "info", json!({})).unwrap_or_else(|| "no data".into()), false),
        "music" => music(app, input),
        "open_app" => open_app(input["name"].as_str().unwrap_or("")),
        "prepare_email" => {
            let to = input["recipient"].as_str().unwrap_or("").trim().to_string();
            if !to.contains('@') {
                return ("No email address: ask the user to spell it.".into(), false);
            }
            let enc = |s: &str| -> String {
                s.bytes()
                    .map(|b| match b {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                        _ => format!("%{b:02X}"),
                    })
                    .collect()
            };
            let url = format!(
                "mailto:{}?subject={}&body={}",
                enc(&to).replace("%40", "@"),
                enc(input["subject"].as_str().unwrap_or("")),
                enc(input["body"].as_str().unwrap_or(""))
            );
            crate::platform::open_url(&url);
            (format!("The mail app opened with the email to {to}. Nothing is sent until the user sends it."), true)
        }
        "reminder" => reminder(app, input),
        "timer" => crate::clockify::voice(app, input),
        _ => (format!("unknown tool {name}"), false),
    }
}

/// « Remind me to call Anne at 3 pm »: the reminder tool.
fn reminder(app: &AppHandle, input: &Value) -> (String, bool) {
    match input["action"].as_str().unwrap_or("") {
        "add" => {
            let when = input["when"].as_str().unwrap_or("");
            let Some(at) = crate::reminders::parse_local(when) else {
                return ("No valid time: ask the user when (then give YYYY-MM-DDTHH:MM).".into(), false);
            };
            match crate::reminders::add(app, input["text"].as_str().unwrap_or(""), at) {
                Ok(r) => (format!("Reminder set for {when}: {}", r.text), true),
                Err(e) => (e, false),
            }
        }
        "remove" => {
            let id = input["id"].as_str().unwrap_or("");
            let known = crate::reminders::list().iter().any(|r| r.id == id);
            if !known {
                return ("No reminder with that id: list them first.".into(), false);
            }
            let removed = crate::reminders::reminders_remove(app.clone(), id.to_string()).is_ok();
            (if removed { "Removed.".into() } else { "Could not remove it.".into() }, removed)
        }
        _ => {
            use chrono::TimeZone;
            let lines: Vec<String> = crate::reminders::list().iter().map(|r| {
                let when = chrono::Local.timestamp_millis_opt(r.at).single()
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
                format!("{} · {} · {}", r.id, when, r.text)
            }).collect();
            (if lines.is_empty() { "No reminders.".into() } else { lines.join("\n") }, false)
        }
    }
}

fn music(app: &AppHandle, input: &Value) -> (String, bool) {
    let action = input["action"].as_str().unwrap_or("");
    let (control, value) = match action {
        "play_pause" => ("playPause", None),
        "next" => ("next", None),
        "previous" => ("previous", None),
        "set_volume" => ("volume", Some(input["volume"].as_f64().unwrap_or(50.0).clamp(0.0, 100.0))),
        "search" => {
            let q = input["query"].as_str().unwrap_or("").trim();
            if q.is_empty() {
                return ("missing query".into(), false);
            }
            let encoded: String = q.chars().map(|c| if c.is_alphanumeric() { c.to_string() } else { "%20".into() }).collect();
            crate::platform::open_url(&format!("spotify:search:{encoded}"));
            return (format!("Spotify opened on a search for {q}: the user picks what to play."), true);
        }
        _ => return ("unknown action".into(), false),
    };
    let app2 = app.clone();
    let done = tauri::async_runtime::block_on(crate::spotify::spotify_control(app2, control.into(), value));
    if done {
        ("done".into(), true)
    } else {
        ("Spotify is not running or not playing: tell the user to open it.".into(), false)
    }
}

/// Opens an app whose name matches: a Start menu shortcut on Windows, a
/// .desktop entry on Linux. Nothing else is ever run.
fn open_app(name: &str) -> (String, bool) {
    let wanted = wake::normalize(name);
    if wanted.is_empty() {
        return ("no name".into(), false);
    }
    match find_app(&wanted) {
        Some((label, launch)) => {
            launch();
            (format!("{label} opened"), true)
        }
        None => (format!("No app named {name} on this computer."), false),
    }
}

#[cfg(windows)]
fn find_app(wanted: &str) -> Option<(String, Box<dyn FnOnce()>)> {
    let mut roots = Vec::new();
    if let Some(p) = std::env::var_os("APPDATA") {
        roots.push(std::path::PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    if let Some(p) = std::env::var_os("ProgramData") {
        roots.push(std::path::PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    let mut best: Option<(usize, std::path::PathBuf)> = None;
    let mut stack = roots;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
                let stem = wake::normalize(&path.file_stem().unwrap_or_default().to_string_lossy());
                let score = if stem == wanted { 0 } else if stem.starts_with(wanted) { 1 } else if stem.contains(wanted) { 2 } else { continue };
                if best.as_ref().is_none_or(|(s, p)| score < *s || (score == *s && path.as_os_str().len() < p.as_os_str().len())) {
                    best = Some((score, path));
                }
            }
        }
    }
    let (_, path) = best?;
    let label = path.file_stem()?.to_string_lossy().to_string();
    Some((label, Box::new(move || crate::platform::reveal_folder(&path.to_string_lossy()))))
}

#[cfg(target_os = "linux")]
fn find_app(wanted: &str) -> Option<(String, Box<dyn FnOnce()>)> {
    let mut dirs = vec![std::path::PathBuf::from("/usr/share/applications"), std::path::PathBuf::from("/var/lib/flatpak/exports/share/applications")];
    dirs.push(crate::platform::home_dir().join(".local/share/applications"));
    dirs.push(crate::platform::home_dir().join(".local/share/flatpak/exports/share/applications"));
    let mut best: Option<(usize, String, String)> = None;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            if text.lines().any(|l| l.trim() == "NoDisplay=true") {
                continue;
            }
            let Some(app_name) = text.lines().find_map(|l| l.strip_prefix("Name=")).map(str::trim) else { continue };
            let n = wake::normalize(app_name);
            let score = if n == wanted { 0 } else if n.starts_with(wanted) { 1 } else if n.contains(wanted) { 2 } else { continue };
            let id = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                best = Some((score, app_name.to_string(), id));
            }
        }
    }
    let (_, label, id) = best?;
    Some((label, Box::new(move || {
        let mut cmd = std::process::Command::new("gtk-launch");
        cmd.arg(&id);
        let _ = crate::platform::no_console(&mut cmd).spawn();
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn the_voice_is_picked_like_on_the_mac() {
        let voices: Vec<Value> = serde_json::from_str(r#"[
            {"voice_id":"a","labels":{"gender":"male","accent":"american","age":"middle aged"}},
            {"voice_id":"b","labels":{"gender":"male","accent":"british","age":"young"}},
            {"voice_id":"c","labels":{"gender":"female","accent":"american"}},
            {"voice_id":"d","labels":{"gender":"female","language":"fr"}}
        ]"#).unwrap();
        assert_eq!(pick_voice(&voices, "male").as_deref(), Some("b"));
        assert_eq!(pick_voice(&voices, "female").as_deref(), Some("c"));
        assert_eq!(pick_voice(&[], "female"), None);
    }
}

/// With the models downloaded: `RONRON_TEST_WAKE_WAV=… cargo test -- --ignored wake_in`.
#[cfg(test)]
mod with_models {
    #[test]
    #[ignore]
    fn hears_the_wake_phrase_in_a_recording() {
        let path = std::env::var("RONRON_TEST_WAKE_WAV").expect("RONRON_TEST_WAKE_WAV");
        let bytes = std::fs::read(&path).unwrap();
        let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        let pcm: Vec<f32> = bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        let mut r = crate::speech::audio::Resampler::new(rate);
        let mut samples = Vec::new();
        r.process(&pcm, &mut samples);
        let mut vad = super::wake::Vad::default();
        let mut heard = Vec::new();
        for chunk in samples.chunks(1600) {
            for segment in vad.feed(chunk) {
                let text = crate::speech::transcribe(&segment).expect("models").text;
                println!("segment {:.2} s: {text}", segment.len() as f32 / 16_000.0);
                heard.push(super::wake::after_wake(&text));
            }
        }
        println!("{heard:?}");
        assert!(heard.iter().any(|h| h.as_deref().is_some_and(|q| !q.is_empty())));
    }
}
