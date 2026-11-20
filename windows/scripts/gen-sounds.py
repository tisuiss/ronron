#!/usr/bin/env python3
"""Synthesises Ronron's 29 sounds into windows/sounds/ (mono, 44.1 kHz, 16-bit WAV).

Everything is made from sine waves and filtered noise, so the sounds are ours
to ship. Tweak a recipe below and run it again:

    python3 scripts/gen-sounds.py

Users can still replace any sound with a file of their own (Settings → General →
Open sounds folder), as before.
"""
import wave
from pathlib import Path

import numpy as np

SR = 44_100
OUT = Path(__file__).resolve().parent.parent / "sounds"
rng = np.random.default_rng(7)


# ── Building blocks ─────────────────────────────────────────────────────────────

def t_axis(dur):
    return np.arange(int(SR * dur)) / SR


def env(n, attack=0.005, release=0.08, dur=None):
    """Attack / exponential release envelope over n samples."""
    t = np.arange(n) / SR
    dur = dur or n / SR
    a = np.clip(t / max(attack, 1e-4), 0, 1)
    r = np.exp(-np.maximum(0, t - attack) / max(release, 1e-4))
    tail = np.clip((dur - t) / 0.01, 0, 1)  # no click at the end
    return a * r * tail


def glide(f0, f1, dur, curve=1.0):
    """Instantaneous frequency going from f0 to f1."""
    x = np.linspace(0, 1, int(SR * dur)) ** curve
    return f0 + (f1 - f0) * x


def osc(freq, harmonics=(1.0,), phase=0.0):
    """Additive oscillator following a frequency curve (array) or constant."""
    freq = np.asarray(freq, dtype=float)
    ph = 2 * np.pi * np.cumsum(freq) / SR + phase
    out = np.zeros_like(ph)
    for k, a in enumerate(harmonics, start=1):
        out += a * np.sin(k * ph)
    return out


def bell(freq, dur, release=0.25, bright=0.35):
    n = int(SR * dur)
    f = np.full(n, freq)
    s = osc(f, (1.0, bright, bright * 0.4, bright * 0.15))
    return s * env(n, 0.003, release)


def lowpass(x, cutoff):
    """One-pole low-pass, cutoff in Hz (array or constant)."""
    cutoff = np.broadcast_to(np.asarray(cutoff, dtype=float), x.shape)
    a = 1 - np.exp(-2 * np.pi * cutoff / SR)
    y = np.zeros_like(x)
    acc = 0.0
    for i in range(len(x)):
        acc += a[i] * (x[i] - acc)
        y[i] = acc
    return y


def noise(dur):
    return rng.uniform(-1, 1, int(SR * dur))


def meow(dur, f_start, f_peak, f_end, vibrato=6.0, bright=1.0):
    """A stylised meow: a pitch arc with harmonics shaped like an opening mouth."""
    n = int(SR * dur)
    x = np.linspace(0, 1, n)
    # Up to the peak at 35 %, then down.
    f = np.where(x < 0.35, f_start + (f_peak - f_start) * np.sin(x / 0.35 * np.pi / 2),
                 f_peak + (f_end - f_peak) * np.sin((x - 0.35) / 0.65 * np.pi / 2))
    f = f * (1 + 0.012 * np.sin(2 * np.pi * vibrato * x * dur))
    ph = 2 * np.pi * np.cumsum(f) / SR
    mouth = np.sin(np.pi * np.clip(x * 1.15, 0, 1)) ** 0.7  # "mi-aa-ou"
    out = np.zeros(n)
    for k in range(1, 9):
        # Formant around 900–1600 Hz moves with the mouth.
        formant = 800 + 900 * mouth
        w = np.exp(-((k * f - formant) / (500 * bright)) ** 2) + 0.35 / k
        out += w * np.sin(k * ph)
    shape = np.sin(np.pi * x) ** 0.6
    return out * shape


def purr(dur, rate=24.0, level=1.0):
    """A purr: low noise pulsing ~25 times a second."""
    n = int(SR * dur)
    t = np.arange(n) / SR
    pulses = (0.5 + 0.5 * np.sin(2 * np.pi * rate * t)) ** 3
    breath = 0.65 + 0.35 * np.sin(2 * np.pi * 0.9 * t)  # in and out
    body = lowpass(noise(dur), 260) * 3.0 + 0.4 * np.sin(2 * np.pi * rate * 2 * t)
    fade = np.minimum(1, t / 0.08) * np.minimum(1, (dur - t) / 0.15)
    return body * pulses * breath * fade * level


def click(dur=0.012, cutoff=6000):
    n = int(SR * dur)
    return lowpass(noise(dur), cutoff) * env(n, 0.0005, dur / 4)


def seq(*parts):
    """Places (start_seconds, signal) parts on one timeline."""
    end = max(start + len(sig) / SR for start, sig in parts)
    out = np.zeros(int(SR * end) + 1)
    for start, sig in parts:
        i = int(SR * start)
        out[i:i + len(sig)] += sig
    return out


def note(name):
    names = {"C": -9, "D": -7, "E": -5, "F": -4, "G": -2, "A": 0, "B": 2}
    letter, octave = name[0], int(name[-1])
    semis = names[letter] + (1 if "#" in name else 0) + 12 * (octave - 4)
    return 440.0 * 2 ** (semis / 12)


# ── The sounds ─────────────────────────────────────────────────────────────────

def chirp(f0, f1, dur, release=0.05):
    n = int(SR * dur)
    return osc(glide(f0, f1, dur), (1, 0.2)) * env(n, 0.003, release)


SOUNDS = {
    "peek": lambda: chirp(620, 980, 0.09),
    "open": lambda: seq((0, bell(note("C6"), 0.14, 0.06)), (0.06, bell(note("E6"), 0.16, 0.08))),
    "close": lambda: seq((0, bell(note("E6"), 0.12, 0.05)), (0.05, bell(note("A5"), 0.16, 0.07))),
    "hover": lambda: bell(note("A6"), 0.04, 0.012, 0.1),
    "blip": lambda: bell(1250, 0.06, 0.02, 0.15),
    "slap": lambda: lowpass(noise(0.07), 3500) * env(int(SR * 0.07), 0.001, 0.015)
            + osc(glide(180, 90, 0.07)) * env(int(SR * 0.07), 0.001, 0.03) * 0.8,
    "annoyed": lambda: meow(0.28, 260, 300, 200, bright=0.7) * 0.9,
    "dizzy": lambda: osc(600 + 220 * np.sin(2 * np.pi * 7 * t_axis(0.55)) * np.linspace(1, 0.3, int(SR * 0.55)), (1, 0.2))
             * env(int(SR * 0.55), 0.01, 0.3),
    "greet": lambda: meow(0.34, 520, 760, 560),
    "work": lambda: purr(0.6, level=0.9),
    "finish": lambda: seq(*[(i * 0.07, bell(note(n), 0.3, 0.16)) for i, n in enumerate(["C6", "E6", "G6", "C7"])]),
    "error": lambda: seq((0, bell(note("E4"), 0.18, 0.08, 0.6)), (0.11, bell(note("C4"), 0.24, 0.12, 0.6))),
    "approval": lambda: seq((0, meow(0.16, 700, 880, 760)), (0.2, meow(0.18, 720, 920, 800))),
    "question": lambda: meow(0.36, 480, 560, 820),
    "approve": lambda: seq((0, bell(note("G6"), 0.25, 0.14)), (0.0, bell(note("D7"), 0.25, 0.1, 0.1) * 0.5)),
    "gulp": lambda: osc(glide(520, 180, 0.14, 0.6), (1, 0.3)) * env(int(SR * 0.14), 0.004, 0.06),
    "tick": lambda: click(0.015, 7000),
    "send": lambda: lowpass(noise(0.26), glide(500, 6000, 0.26)) * np.sin(np.linspace(0, np.pi, int(SR * 0.26))) * 1.4,
    "love": lambda: seq((0, purr(0.75, level=0.7)), (0.05, bell(note("E6"), 0.4, 0.2) * 0.6), (0.2, bell(note("A6"), 0.45, 0.25) * 0.5)),
    "pop": lambda: osc(glide(900, 300, 0.05, 0.5)) * env(int(SR * 0.05), 0.001, 0.015),
    "proud": lambda: seq((0, bell(note("G5"), 0.16, 0.08)), (0.1, bell(note("C6"), 0.16, 0.08)), (0.2, bell(note("E6"), 0.4, 0.22)), (0.2, bell(note("G6"), 0.4, 0.22) * 0.6)),
    "wink": lambda: seq((0, chirp(900, 1400, 0.07)), (0.07, chirp(1400, 1100, 0.08))),
    "yawn": lambda: meow(0.85, 640, 700, 300, vibrato=4, bright=0.8),
    "attach": lambda: seq((0, click(0.012, 5000)), (0.03, bell(note("B5"), 0.16, 0.08))),
    "think": lambda: seq(*[(i * 0.12, bell(note(n), 0.35, 0.2) * 0.7) for i, n in enumerate(["A5", "C6", "E6"])]),
    "search": lambda: seq((0, chirp(500, 1300, 0.12)), (0.15, chirp(500, 1300, 0.12))),
    "rate": lambda: seq((0, bell(note("D5"), 0.22, 0.1, 0.5)), (0.14, bell(note("A4"), 0.3, 0.15, 0.5))),
    "sleep": lambda: purr(1.3, rate=20, level=0.8),
}


def greeting():
    """The launch score: a hello meow, a little tune, and a purr to settle in."""
    melody = [("C6", 1.25), ("E6", 1.45), ("G6", 1.65), ("E6", 1.85), ("A6", 2.15), ("G6", 2.6)]
    parts = [(0.25, meow(0.4, 560, 820, 600) * 0.9)]
    parts += [(t0, bell(note(n), 0.45, 0.25) * 0.55) for n, t0 in melody]
    parts += [(2.6, bell(note("C6"), 0.9, 0.5) * 0.35), (2.6, bell(note("E6"), 0.9, 0.5) * 0.3)]
    parts += [(3.0, purr(1.9, level=0.55))]
    return seq(*parts)


SOUNDS["greeting"] = greeting


def write(name, sig):
    sig = np.asarray(sig, dtype=float)
    peak = np.max(np.abs(sig)) or 1.0
    sig = sig / peak * 0.7  # about -3 dBFS
    data = (sig * 32767).astype("<i2").tobytes()
    with wave.open(str(OUT / f"{name}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, make in SOUNDS.items():
        write(name, make())
    print(f"{len(SOUNDS)} sounds written to {OUT}")


if __name__ == "__main__":
    main()
