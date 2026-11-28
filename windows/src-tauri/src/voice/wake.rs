// « OK Ronron »: hearing the wake phrase in what Parakeet wrote, and telling
// speech from silence on the microphone. Nothing here touches the OS.

/// Lower case, accents gone, everything but letters and digits a space.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ÿ' | 'ý' => 'y',
            'ñ' => 'n',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(base);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How Parakeet may write "OK".
fn is_ok(word: &str) -> bool {
    matches!(word, "ok" | "okay" | "oke" | "okey" | "hey")
}

/// How Parakeet may write "ronron", in one word. The French /ʁɔ̃ʁɔ̃/ comes out
/// as "ronron" or "ronrons"; an English ear writes "ron ron" or "wrong wrong"
/// (see the pairs below). Not "ronald" nor "rondo": real words people say.
fn is_ronron(word: &str) -> bool {
    matches!(
        word,
        "ronron" | "ronrons" | "ronronne" | "ronronner" | "rongrong" | "ronrong" | "rondron" | "romron" | "wrongwrong" | "honhon"
    )
}

/// …or in two.
fn is_ronron_pair(a: &str, b: &str) -> bool {
    matches!(
        (a, b),
        ("ron", "ron") | ("rond", "rond") | ("ron", "rond") | ("rond", "ron") | ("rom", "rom") | ("wrong", "wrong") | ("hon", "hon") | ("rong", "rong")
    )
}

/// What Parakeet keeps of « Ronron » when it barely hears it: "Okay, on",
/// "OK rond". Only taken when nothing else was said in the same breath — "ok,
/// on y va" is a sentence, not Ronron's name.
fn is_ronron_hint(word: &str) -> bool {
    matches!(
        word,
        "on" | "ron" | "rond" | "ronds" | "rons" | "ronde" | "rom" | "rome" | "rhum" | "hon" | "rong" | "rang" | "ran" | "wrong"
    )
}

/// The wake phrase, also as Parakeet writes it for this user: `learned` are
/// the phrases Voice → Learn kept (what was written when they said it).
pub fn after_wake_with(text: &str, learned: &[String]) -> Option<String> {
    if let Some(rest) = after_wake(text) {
        return Some(rest);
    }
    let norm = normalize(text);
    let tokens: Vec<&str> = norm.split(' ').filter(|w| !w.is_empty()).collect();
    // « OK » and one or two half-heard words, said alone: the question comes next.
    if (2..=3).contains(&tokens.len()) && is_ok(tokens[0]) && tokens[1..].iter().all(|w| is_ronron_hint(w)) {
        return Some(String::new());
    }
    learned_match(text, learned)
}

/// A learned phrase anywhere in `text`, on word boundaries: what follows it.
fn learned_match(text: &str, learned: &[String]) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    // Each normalised token, with the word it comes from ("ron-ron" gives two).
    let mut tokens: Vec<(String, usize)> = Vec::new();
    for (i, w) in words.iter().enumerate() {
        for t in normalize(w).split(' ').filter(|t| !t.is_empty()) {
            tokens.push((t.to_string(), i));
        }
    }
    for phrase in learned {
        let want: Vec<String> = normalize(phrase).split(' ').filter(|t| !t.is_empty()).map(str::to_string).collect();
        if !usable_phrase(&want) || want.len() > tokens.len() {
            continue;
        }
        for start in 0..=tokens.len() - want.len() {
            if tokens[start..start + want.len()].iter().map(|(t, _)| t).eq(want.iter()) {
                let last = tokens[start + want.len() - 1].1;
                return Some(words[last + 1..].join(" ").trim_start_matches([',', '.', '!', ' ']).trim().to_string());
            }
        }
    }
    None
}

/// A learned phrase is kept when it can wake Ronron without waking him all
/// the time: 1 to 6 words ("c'est" counts two), at least 4 letters.
pub fn usable_phrase(tokens: &[String]) -> bool {
    (1..=6).contains(&tokens.len()) && tokens.iter().map(|t| t.len()).sum::<usize>() >= 4
}

/// When `text` holds the wake phrase, what was said after it (maybe nothing).
/// The words keep their case and punctuation, as Parakeet wrote them.
pub fn after_wake(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let norm: Vec<String> = words.iter().map(|w| normalize(w)).collect();
    // "o.k." and "o k" become two tokens: glue them.
    let mut i = 0;
    while i < norm.len() {
        let mut ok_end = None;
        if is_ok(&norm[i]) {
            ok_end = Some(i + 1);
        } else if norm[i] == "o k" || (norm[i] == "o" && norm.get(i + 1).map(String::as_str) == Some("k")) {
            ok_end = Some(if norm[i] == "o" { i + 2 } else { i + 1 });
        }
        if let Some(j) = ok_end {
            // A word made of two ("ok ronron" written as one token is rare but happens).
            if let Some(w) = norm.get(j) {
                let parts: Vec<&str> = w.split(' ').collect();
                if (parts.len() == 1 && is_ronron(parts[0])) || (parts.len() == 2 && is_ronron_pair(parts[0], parts[1])) {
                    return Some(words[j + 1..].join(" ").trim_start_matches([',', '.', '!', ' ']).trim().to_string());
                }
                if let Some(next) = norm.get(j + 1) {
                    if is_ronron_pair(w, next) {
                        return Some(words[j + 2..].join(" ").trim_start_matches([',', '.', '!', ' ']).trim().to_string());
                    }
                }
            }
        }
        // "okronron" in one piece.
        if let Some(rest) = norm[i].strip_prefix("okay").or_else(|| norm[i].strip_prefix("ok")) {
            if is_ronron(rest.trim()) {
                return Some(words[i + 1..].join(" ").trim().to_string());
            }
        }
        i += 1;
    }
    None
}

/// Speech or silence, 30 ms at a time, against a noise floor that follows the
/// room. A stretch of speech is handed out once followed by `hangover` frames
/// of silence, or when it gets too long.
pub struct Vad {
    floor: f32,
    in_speech: bool,
    speech_frames: usize,
    silent_frames: usize,
    frame: Vec<f32>,
    /// The current stretch, with a little audio from before it began.
    current: Vec<f32>,
    preroll: std::collections::VecDeque<f32>,
    /// Silence that ends a stretch, in frames.
    pub hangover: usize,
    /// The last frame's loudness (RMS), for Voice → Test.
    pub level: f32,
}

/// Below this loudness (RMS) nothing is speech, however quiet the room.
pub const MIN_SPEECH_RMS: f32 = 0.012;

pub const FRAME: usize = 480; // 30 ms at 16 kHz
const PREROLL: usize = 16_000 * 3 / 10;
const MAX_SEGMENT: usize = 16_000 * 12;

impl Default for Vad {
    fn default() -> Self {
        Self {
            floor: 0.004,
            in_speech: false,
            speech_frames: 0,
            silent_frames: 0,
            frame: Vec::with_capacity(FRAME),
            current: Vec::new(),
            preroll: std::collections::VecDeque::with_capacity(PREROLL),
            hangover: 25,
            level: 0.0,
        }
    }
}

impl Vad {
    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    /// The loudness a frame needs to count as speech now: three times the room's noise, at least MIN_SPEECH_RMS.
    pub fn threshold(&self) -> f32 {
        (self.floor * 3.0).max(MIN_SPEECH_RMS)
    }

    /// Feeds 16 kHz samples; the stretches of speech that ended.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut done = Vec::new();
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() < FRAME {
                continue;
            }
            let rms = (self.frame.iter().map(|x| x * x).sum::<f32>() / FRAME as f32).sqrt();
            self.level = rms;
            let speech = rms > self.threshold();
            if !speech && !self.in_speech {
                self.floor = self.floor * 0.97 + rms * 0.03;
            }
            if self.in_speech {
                self.current.extend_from_slice(&self.frame);
                if speech {
                    self.silent_frames = 0;
                } else {
                    self.silent_frames += 1;
                }
                if self.silent_frames >= self.hangover || self.current.len() >= MAX_SEGMENT {
                    done.push(std::mem::take(&mut self.current));
                    self.in_speech = false;
                    self.speech_frames = 0;
                    self.silent_frames = 0;
                }
            } else if speech {
                self.speech_frames += 1;
                if self.speech_frames >= 3 {
                    self.in_speech = true;
                    self.current = self.preroll.drain(..).collect();
                    self.current.extend_from_slice(&self.frame);
                }
            } else {
                self.speech_frames = 0;
            }
            if !self.in_speech {
                for &x in &self.frame {
                    if self.preroll.len() == PREROLL {
                        self.preroll.pop_front();
                    }
                    self.preroll.push_back(x);
                }
            }
            self.frame.clear();
        }
        done
    }

    /// Forgets what is being heard (Ronron speaks, or dictation starts).
    pub fn reset(&mut self) {
        self.in_speech = false;
        self.speech_frames = 0;
        self.silent_frames = 0;
        self.current.clear();
        self.preroll.clear();
        self.frame.clear();
    }
}

/// "fr-FR" for a French answer, else None (the system's language).
pub fn speech_lang(text: &str) -> Option<&'static str> {
    let n = normalize(text);
    let words: Vec<&str> = n.split(' ').collect();
    let french = ["le", "la", "les", "est", "tu", "vous", "je", "un", "une", "des", "pour", "pas", "c", "et", "sur", "avec", "ton", "ta"];
    let count = words.iter().filter(|w| french.contains(w)).count();
    (count * 6 >= words.len().max(1) && count >= 1).then_some("fr-FR")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_phrase_however_parakeet_writes_it() {
        assert_eq!(after_wake("OK Ronron, play some music.").as_deref(), Some("play some music."));
        assert_eq!(after_wake("Okay, ronron! Quelle heure est-il ?").as_deref(), Some("Quelle heure est-il ?"));
        assert_eq!(after_wake("O.K. Ron Ron what's the weather").as_deref(), Some("what's the weather"));
        assert_eq!(after_wake("ok rond rond ajoute Stripe").as_deref(), Some("ajoute Stripe"));
        assert_eq!(after_wake("OK Ronron.").as_deref(), Some(""));
        assert_eq!(after_wake("Hey ronron, ça va ?").as_deref(), Some("ça va ?"));
        assert_eq!(after_wake("Ok ron-ron, quelle heure ?").as_deref(), Some("quelle heure ?"));
        assert_eq!(after_wake("okronron ouvre Spotify").as_deref(), Some("ouvre Spotify"));
        // An English ear's « OK Ronron ».
        assert_eq!(after_wake("Okay, wrong wrong.").as_deref(), Some(""));
        // Real words stay out, and another assistant's wake phrase never wakes Ronron.
        assert_eq!(after_wake("Okay, wrong."), None);
        assert_eq!(after_wake("Okay, Ronald."), None);
        assert_eq!(after_wake("Okay, Google."), None);
        assert_eq!(after_wake("on a dit ronron à Paul"), None);
        assert_eq!(after_wake("ok let's go"), None);
        assert_eq!(normalize("Ça VA, Élodie ?"), "ca va elodie");
    }

    #[test]
    fn a_half_heard_ronron_said_alone_wakes_him_but_not_inside_a_sentence() {
        let none: Vec<String> = Vec::new();
        assert_eq!(after_wake_with("Okay, on", &none).as_deref(), Some(""));
        assert_eq!(after_wake_with("OK rond.", &none).as_deref(), Some(""));
        assert_eq!(after_wake_with("Ok, rond rond.", &none).as_deref(), Some(""));
        assert_eq!(after_wake_with("ok on y va", &none), None);
        assert_eq!(after_wake_with("Okay, on se voit demain", &none), None);
        assert_eq!(after_wake_with("Mm-hmm.", &none), None);
        assert_eq!(after_wake_with("OK Ronron, quelle heure ?", &none).as_deref(), Some("quelle heure ?"));
    }

    #[test]
    fn a_learned_phrase_wakes_him_and_what_follows_is_the_question() {
        let learned = vec!["C'est le haut qui ont".to_string(), "au caron".to_string(), "ok".to_string()];
        assert_eq!(after_wake_with("C'est le haut qui ont, quelle heure est-il ?", &learned).as_deref(), Some("quelle heure est-il ?"));
        assert_eq!(after_wake_with("Au caron.", &learned).as_deref(), Some(""));
        // Too short to learn: "ok" alone would wake him all the time.
        assert_eq!(after_wake_with("ok merci", &learned), None);
        assert_eq!(after_wake_with("le caron du voisin", &learned), None);
        assert!(!usable_phrase(&["ok".to_string()]));
        assert!(usable_phrase(&["au".to_string(), "caron".to_string()]));
    }

    #[test]
    fn speech_is_cut_after_a_pause() {
        let mut vad = Vad::default();
        let silence = vec![0.0f32; 16_000];
        let tone: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.1).sin() * 0.3).collect();
        assert!(vad.feed(&silence).is_empty());
        assert!(vad.feed(&tone).is_empty());
        assert!(vad.in_speech());
        let out = vad.feed(&silence);
        assert_eq!(out.len(), 1);
        // The tone, a pre-roll before it and the hangover after it.
        assert!(out[0].len() > 16_000 && out[0].len() <= 16_000 + 25 * FRAME + PREROLL + 2 * FRAME);
        assert!(!vad.in_speech());
    }

    #[test]
    fn french_answers_are_spoken_in_french() {
        assert_eq!(speech_lang("C'est fait, j'ai ajouté Stripe pour toi."), Some("fr-FR"));
        assert_eq!(speech_lang("Done, Stripe is in the island."), None);
    }
}
