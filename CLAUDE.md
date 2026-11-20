# Ronron — guide for AI coding agents

Ronron is a Windows and Linux app (Tauri 2) forked from Coucou. Ronron, a small animated ginger cat living in an island at the top of the screen, shows AI coding agent sessions (Claude Code, Codex, Gemini CLI and more) and a few integrations, and lets the user approve, answer, chat, dictate and drop files from the island.

## Where things are
- `windows/` — the whole app: Rust in `src-tauri/`, TypeScript in `src/`, the `ronron-hook` relay in `hook/`. `windows/README.md` is the user guide.
- `windows/src/cat/` — the character: `engine.ts` (states, tweens, eyes, particles), `parts.ts` (the cat's ears, tail, stripes, nose and whiskers), `outfits.ts`, `wardrobe.ts`, `greeting.ts`, `minibots.ts`.
- `windows/src/i18n/strings.json` — generated from `windows/i18n-source/Localizable.xcstrings` by `node scripts/gen-strings.mjs`; never edit it by hand. Windows-only strings live in `windows/src/i18n/extra.json`, translated in all 14 languages.
- `windows/sounds/` — the 29 WAVs, made by `windows/scripts/gen-sounds.py`. `windows/src-tauri/icons/` — made by `windows/scripts/gen-icons.py` from `dev/icon.html`.
- `windows/dev/` — browser previews (`cat-preview.html`, `upload-preview.html`…) served by `npx vite --port 1420`.
- `linux/` — Arch and Nix packaging. `.github/workflows/` — Windows and Linux CI and releases.

## Build and test
```
cd windows && npm install && npm run tauri dev
npx tsc --noEmit && npm test
cargo build --release -p ronron-hook && cargo test --workspace --locked
```

## Rules
- Secrets live in the Windows Credential Manager / Secret Service, never on disk or in git.
- No telemetry. Network calls only to services the user configured.
- Never block Claude Code: if the app doesn't answer, the hook exits immediately.
- Never overwrite `~/.claude/settings.json`: dated backup, merge, show the diff, write only after the user confirms.
- Never send an email or approve a permission without an explicit click.
- Performance: 0 % CPU when the island is hidden.
- Keep the app identifier `ch.cloudguy.ronron` (settings, keys and hooks depend on it).
- Pill IDs are stable contract values (credential store, settings, hook routing): never rename an existing pill ID.
- The character is drawn in code (Canvas 2D), no images. Never bring back Coucou's Mochi character, icon, sounds or media: they are not under the MIT License (see NOTICE.md).
- New views follow the existing app style.
- Every release adds its CHANGELOG.md section and its What's new entry in `windows/src/whats-new/whats-new.json` (1 to 4 items, in the 14 languages); `python3 scripts/test-whats-new.py windows` checks it.
