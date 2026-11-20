// Ronron for VS Code — the side that runs where your workspace is (a tunnel, SSH,
// a container). It does three things, and only while a VS Code window is open:
//
//  1. Listens for Claude Code's hook events (hook-client.js, run by the hooks in
//     ~/.claude/settings.json) and hands each one to the Ronron extension on your
//     computer, through VS Code's own connection; for a permission, it brings the
//     click on the island back.
//  2. Runs `claude -p` in a workspace folder when Ronron's chat asks for it.
//  3. Installs those hooks — after showing the change and only on your click,
//     with a dated backup of settings.json first.
"use strict";

const vscode = require("vscode");
const fs = require("fs");
const net = require("net");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { spawn } = require("child_process");
const { mergeHooks, hooksInstalled, parseClaude, wrapperScript } = require("./lib.js");

const DIR = path.join(os.homedir(), ".ronron");
const BRIDGES = path.join(DIR, "bridges");
const CHAT_MS = 5 * 60 * 1000;

let server = null;
let socketPath = "";
let registry = "";
const token = crypto.randomBytes(24).toString("hex");
let nextId = 1;

function claudeSettingsPath() {
  const dir = process.env.CLAUDE_CONFIG_DIR || path.join(os.homedir(), ".claude");
  return path.join(dir, "settings.json");
}

/** ~/.ronron, closed to others: the hook client, its wrapper and where Node is. */
function prepareDir(context) {
  fs.mkdirSync(BRIDGES, { recursive: true, mode: 0o700 });
  fs.chmodSync(DIR, 0o700);
  fs.copyFileSync(path.join(context.extensionPath, "hook-client.js"), path.join(DIR, "hook-client.js"));
  fs.copyFileSync(path.join(context.extensionPath, "lib.js"), path.join(DIR, "lib.js"));
  fs.writeFileSync(path.join(DIR, "node"), process.execPath, { mode: 0o600 });
  fs.writeFileSync(path.join(DIR, "ronron-hook"), wrapperScript(DIR), { mode: 0o700 });
  fs.chmodSync(path.join(DIR, "ronron-hook"), 0o700);
}

function writeRegistry() {
  const folders = (vscode.workspace.workspaceFolders || []).map((f) => f.uri.fsPath);
  const data = { pid: process.pid, socket: socketPath, token, folders, started: Date.now() };
  fs.writeFileSync(registry, JSON.stringify(data), { mode: 0o600 });
}

/** One hook event from hook-client.js: to the computer, and its answer back. */
function onConnection(conn) {
  let buf = "";
  let id = null;
  let answered = false;
  conn.setEncoding("utf8");
  conn.on("data", async (chunk) => {
    buf += chunk;
    const nl = buf.indexOf("\n");
    if (nl < 0 || id !== null) return;
    let msg;
    try {
      msg = JSON.parse(buf.slice(0, nl));
    } catch {
      return conn.destroy();
    }
    if (msg.token !== token) return conn.destroy();
    id = `${process.pid}-${nextId++}`;
    let reply = { stdout: "", code: 0 };
    try {
      const r = await vscode.commands.executeCommand("ronron.local.hook", { id, args: msg.args, stdin: msg.stdin, env: msg.env });
      if (r && typeof r.stdout === "string") reply = { stdout: r.stdout, code: Number.isInteger(r.code) ? r.code : 0 };
    } catch {
      // No Ronron extension on the computer: Claude Code carries on as without Ronron.
    }
    answered = true;
    if (!conn.destroyed) conn.end(JSON.stringify(reply) + "\n");
  });
  // Claude Code gave up waiting (answered in its own window): the island's card goes too.
  conn.on("close", () => {
    if (id && !answered) void vscode.commands.executeCommand("ronron.local.cancel", { id }).then(undefined, () => {});
  });
  conn.on("error", () => {});
}

function startServer() {
  socketPath = path.join(DIR, `bridge-${process.pid}.sock`);
  try {
    fs.unlinkSync(socketPath);
  } catch {
    // None left over.
  }
  server = net.createServer(onConnection);
  server.listen(socketPath, () => {
    try {
      fs.chmodSync(socketPath, 0o600);
    } catch {
      // The folder is closed to others anyway.
    }
    writeRegistry();
  });
}

// ── Chat: `claude -p` in a workspace folder ──────────────────────────────────

function findClaude() {
  const configured = vscode.workspace.getConfiguration("ronron").get("claudePath");
  const candidates = [];
  if (configured) candidates.push(configured);
  for (const dir of (process.env.PATH || "").split(path.delimiter)) if (dir) candidates.push(path.join(dir, "claude"));
  const home = os.homedir();
  candidates.push(path.join(home, ".local", "bin", "claude"), path.join(home, ".claude", "local", "claude"),
    path.join(home, ".npm-global", "bin", "claude"), "/usr/local/bin/claude");
  // The Claude Code extension carries its own CLI.
  const ext = vscode.extensions.getExtension("anthropic.claude-code");
  if (ext) {
    candidates.push(path.join(ext.extensionPath, "resources", "native-binary", "claude"));
    candidates.push(path.join(ext.extensionPath, "resources", "claude-code", "cli.js"));
  }
  return candidates.find((p) => {
    try {
      return fs.statSync(p).isFile();
    } catch {
      return false;
    }
  }) || null;
}

/** Runs `claude -p` in `folder` (a workspace folder only), resuming `resume` when given. */
function chat({ folder, prompt, resume }) {
  return new Promise((resolve) => {
    const folders = (vscode.workspace.workspaceFolders || []).map((f) => f.uri.fsPath);
    if (!folders.includes(folder)) return resolve({ error: "This folder is not open in VS Code any more." });
    if (typeof prompt !== "string" || !prompt.trim()) return resolve({ error: "Nothing to ask." });
    const exe = findClaude();
    if (!exe) return resolve({ error: "Claude Code is not installed where this workspace is (or set ronron.claudePath)." });
    const args = ["-p", "--output-format", "json"];
    if (typeof resume === "string" && /^[0-9a-f-]{8,64}$/i.test(resume)) args.push("--resume", resume);
    const cmd = exe.endsWith(".js") ? process.execPath : exe;
    const argv = exe.endsWith(".js") ? [exe, ...args] : args;
    const env = { ...process.env };
    delete env.CLAUDECODE;
    const child = spawn(cmd, argv, { cwd: folder, env, stdio: ["pipe", "pipe", "pipe"] });
    let out = "";
    let err = "";
    const timer = setTimeout(() => child.kill(), CHAT_MS);
    child.stdout.on("data", (c) => (out += c));
    child.stderr.on("data", (c) => (err += c));
    child.on("error", (e) => {
      clearTimeout(timer);
      resolve({ error: `Could not start Claude Code: ${e.message}` });
    });
    child.on("close", () => {
      clearTimeout(timer);
      const r = parseClaude(out);
      if (r.error && err.trim()) r.error += ` (${err.trim().split("\n").pop()})`;
      resolve(r);
    });
    child.stdin.end(prompt);
  });
}

// ── Installing the hooks ──────────────────────────────────────────────────────

function readSettings(file) {
  try {
    return { text: fs.readFileSync(file, "utf8"), exists: true };
  } catch (e) {
    if (e.code === "ENOENT") return { text: "", exists: false };
    throw e;
  }
}

/** Shows what changes in settings.json, and writes it only after "Install" (dated backup first). */
async function installHooks(context, quiet) {
  const file = claudeSettingsPath();
  let current;
  let parsed;
  try {
    current = readSettings(file);
    parsed = current.exists && current.text.trim() ? JSON.parse(current.text) : {};
  } catch (e) {
    void vscode.window.showErrorMessage(`Ronron: ${file} could not be read (${e.message}). Nothing was changed.`);
    return;
  }
  if (hooksInstalled(parsed)) {
    if (!quiet) void vscode.window.showInformationMessage("Ronron: the Claude Code hooks are already installed here.");
    return;
  }
  let next;
  try {
    next = mergeHooks(parsed, (event) => `'${path.join(DIR, "ronron-hook").replace(/'/g, `'\\''`)}' ${event}`);
  } catch (e) {
    void vscode.window.showErrorMessage(`Ronron: ${file} has an unexpected "hooks" (${e.message}). Nothing was changed.`);
    return;
  }
  const after = JSON.stringify(next, null, 2) + "\n";
  // The change, side by side, before anything is written.
  const preview = vscode.Uri.file(path.join(context.globalStorageUri.fsPath, "settings.after.json"));
  const before = vscode.Uri.file(path.join(context.globalStorageUri.fsPath, "settings.before.json"));
  fs.mkdirSync(context.globalStorageUri.fsPath, { recursive: true });
  fs.writeFileSync(before.fsPath, current.text);
  fs.writeFileSync(preview.fsPath, after);
  await vscode.commands.executeCommand("vscode.diff", before, preview, "Ronron: ~/.claude/settings.json (before ↔ after)");
  const choice = await vscode.window.showInformationMessage(
    "Ronron: add the Claude Code hooks to ~/.claude/settings.json here, so Ronron on your computer sees these sessions? A dated backup is made first.",
    { modal: true }, "Install");
  if (choice !== "Install") return;
  // Changed meanwhile: nothing is written over it.
  if (readSettings(file).text !== current.text) {
    void vscode.window.showWarningMessage("Ronron: settings.json changed meanwhile. Nothing was written; try again.");
    return;
  }
  fs.mkdirSync(path.dirname(file), { recursive: true });
  if (current.exists) {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    fs.copyFileSync(file, `${file}.ronron-${stamp}.bak`);
  }
  fs.writeFileSync(file, after);
  void vscode.window.showInformationMessage("Ronron: hooks installed. New Claude Code sessions here show in Ronron's island.");
}

// ── Life cycle ────────────────────────────────────────────────────────────────

function activate(context) {
  // On the computer itself, Ronron's own hooks already do this.
  if (!vscode.env.remoteName) return;
  try {
    prepareDir(context);
  } catch (e) {
    void vscode.window.showErrorMessage(`Ronron: could not prepare ~/.ronron (${e.message}).`);
    return;
  }
  registry = path.join(BRIDGES, `${process.pid}.json`);
  startServer();
  context.subscriptions.push(
    vscode.workspace.onDidChangeWorkspaceFolders(() => writeRegistry()),
    vscode.commands.registerCommand("ronron.remote.chat", (req) => chat(req || {})),
    vscode.commands.registerCommand("ronron.remote.folders", () => (vscode.workspace.workspaceFolders || []).map((f) => ({ name: f.name, path: f.uri.fsPath }))),
    vscode.commands.registerCommand("ronron.installHooks", () => installHooks(context, false)),
  );
  // Offered once per machine when the hooks are missing.
  try {
    const parsed = JSON.parse(readSettings(claudeSettingsPath()).text || "{}");
    if (!hooksInstalled(parsed) && !context.globalState.get("ronron.askedHooks")) {
      void context.globalState.update("ronron.askedHooks", true);
      void vscode.window.showInformationMessage("Ronron: show this machine's Claude Code sessions in Ronron's island?", "Install the hooks…")
        .then((c) => c && installHooks(context, false));
    }
  } catch {
    // An unreadable settings.json: the command is still there.
  }
}

function deactivate() {
  try {
    server && server.close();
    fs.unlinkSync(socketPath);
  } catch {
    // Gone already.
  }
  try {
    fs.unlinkSync(registry);
  } catch {
    // Gone already.
  }
}

module.exports = { activate, deactivate };
