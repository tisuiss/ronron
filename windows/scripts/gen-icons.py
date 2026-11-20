#!/usr/bin/env python3
"""Renders Ronron's app icons from the real drawing code (src/cat/engine.ts).

Needs the dev server (`npx vite --port 1420`), Google Chrome and Pillow:

    python3 scripts/gen-icons.py

It screenshots dev/icon.html at each size with a transparent background and
writes the PNG set and icon.ico Tauri needs into src-tauri/icons/.
"""
import io
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
URL = "http://localhost:1420/dev/icon.html?size={size}"
CHROME = "google-chrome"


def render(size: int) -> Image.Image:
    with tempfile.TemporaryDirectory() as tmp:
        shot = Path(tmp) / "shot.png"
        subprocess.run(
            [CHROME, "--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars",
             "--default-background-color=00000000", f"--window-size={size},{size}",
             "--virtual-time-budget=3000", f"--screenshot={shot}", URL.format(size=size)],
            check=True, capture_output=True,
        )
        return Image.open(shot).convert("RGBA").crop((0, 0, size, size)).copy()


def main() -> None:
    big = render(1024)
    def at(size: int) -> Image.Image:
        # Small sizes are drawn natively (no whiskers below a certain size), larger ones downscaled.
        return render(size) if size <= 64 else big.resize((size, size), Image.LANCZOS)
    OUT.mkdir(parents=True, exist_ok=True)
    for name, size in {"32x32.png": 32, "128x128.png": 128, "128x128@2x.png": 256, "icon.png": 512}.items():
        at(size).save(OUT / name)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    frames = [at(s) for s in sizes]
    frames[-1].save(OUT / "icon.ico", sizes=[(s, s) for s in sizes], append_images=frames[:-1])
    print("icons written to", OUT)


if __name__ == "__main__":
    main()
