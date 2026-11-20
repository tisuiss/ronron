# Ronron for VS Code

Two small extensions that bring the Claude Code sessions of a **remote** VS Code
workspace — Remote Tunnels, Remote-SSH, a container — to Ronron's island on your
computer, and let Ronron's chat ask Claude Code in that workspace.

| Extension | Install it… | What it does |
|---|---|---|
| **Ronron for VS Code (local)** (`ronron-vscode-local-*.vsix`) | on your computer (VS Code → Extensions → ⋯ → *Install from VSIX…*) | Hands each event to Ronron by running Ronron's own relay (`ronron-hook`), and opens a local pipe for Ronron's chat |
| **Ronron for VS Code (remote)** (`ronron-vscode-remote-*.vsix`) | in the remote (with a remote window open: *Install from VSIX…*, it goes to the remote) | Receives Claude Code's hook events there, runs `claude -p` in a folder for the chat, installs the hooks |

Everything goes through VS Code's own connection: nothing listens on the network,
nothing is hosted. It works while a VS Code desktop window is connected (not in
vscode.dev in a browser).

## Setup

1. Ronron on your computer, with its Claude Code relay (it installs `ronron-hook` itself).
2. Install both extensions as above, then reload the window.
3. The remote extension offers **Install the hooks…**: it shows the change to the
   remote's `~/.claude/settings.json` side by side and writes it only on *Install*,
   after a dated backup. Also in the command palette: *Ronron: Install the Claude
   Code hooks here*.

Then every Claude Code session in that remote — the Claude Code extension or the
CLI in VS Code's terminal — shows in the island, per project; a permission can be
approved from the island. In Ronron's chat, pick **VS Code** in the model picker and
a workspace folder: Claude Code answers there, with its files, and the conversation
carries on in the same session.

## How it works

```
remote: Claude Code ─hook→ ~/.ronron/ronron-hook → hook-client.js ─unix socket→ remote extension
                                                                                   │ VS Code command
computer:                       Ronron ←pipe← ronron-hook (Ronron's relay) ← local extension
Ronron chat ─pipe→ local extension ─VS Code command→ remote extension → claude -p (in the folder)
```

- Never blocks Claude Code: no VS Code window or no Ronron, the hook exits at once
  with nothing; only a permission waits (up to 115 s) for the click.
- `~/.ronron` in the remote is closed to other users; each window's socket has a
  random token. The chat pipe on the computer has its own token, in Ronron's folder.
- `ronron.claudePath` (remote setting) when the CLI is not found by itself.
