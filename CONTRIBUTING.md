# Contributing to Ronron

Thanks for wanting to help! 🐈

## Getting started

```bash
cd windows
npm install
npm run tauri dev
```

Before a pull request:

```bash
npx tsc --noEmit
npm test
cargo build --release -p ronron-hook
cargo test --workspace --locked
node scripts/gen-strings.mjs --check
python3 ../scripts/test-whats-new.py windows
```

## Good first contributions

- A new service integration: a poller in `src-tauri/src/`, a pill in the catalog and a card in `src/views/`.
- A new agent: any agent already gets its own pill by sending `ronron_agent` in its hook payload.
- A new emote or sound for Ronron (`src/cat/`, `scripts/gen-sounds.py`).
- Bug fixes: please describe how to reproduce.

## Rules of the house

- Secrets go in the Windows Credential Manager / Secret Service, never on disk or in git.
- No telemetry, no network calls except to services the user configured.
- Never block Claude Code: if the app doesn't answer, the hook exits right away.
- Never write `~/.claude/settings.json` without a backup and the user's confirmation.
- Keep it light: 0 % CPU when the island is hidden.

## Pull requests

- One topic per PR, with a short GIF or screenshot for anything visual.
- Tests must pass with no new warnings.
