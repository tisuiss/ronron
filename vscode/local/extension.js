// Ronron for VS Code — the side that runs on your computer, next to Ronron.
//
//  1. Hands each Claude Code hook event from the remote workspace (see the remote
//     extension) to Ronron, by running Ronron's own relay (ronron-hook) exactly as
//     Claude Code would on this computer, and sends its answer back.
//  2. Lets Ronron's chat reach the remote workspace: Ronron finds this window in
//     its folder (<Ronron>/vscode/<pid>.json) and asks through a local pipe that
//     only it knows the token of.
"use strict";

const vscode = require("vscode");
const fs = require("fs");
const net = require("net");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const { spawn } = require("child_process");

/** Ronron's own folder: where its relay is installed (src-tauri/src/settings.rs local_dir). */
function ronronDir() {
  if (process.platform === "win32") return path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local"), "Ronron");
  const data = process.env.XDG_DATA_HOME && path.isAbsolute(process.env.XDG_DATA_HOME) ? process.env.XDG_DATA_HOME : path.join(os.homedir(), ".local", "share");
  return path.join(data, "ronron");
}

function relayPath() {
  return path.join(ronronDir(), "bin", process.platform === "win32" ? "ronron-hook.exe" : "ronron-hook");
}

/** Only what the relay reads from a hook's environment comes from the remote. */
const ENV_KEYS = ["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_PROJECT_DIR", "TERM_PROGRAM"];
/** A permission card waits for a click (the relay gives up first, at 110 s). */
const HOOK_MS = 118_000;

const running = new Map();

/** One event: Ronron's relay, run here with the event's arguments and input. */
function hook({ id, args, stdin, env }) {
  return new Promise((resolve) => {
    const exe = relayPath();
    if (!fs.existsSync(exe)) return resolve({ stdout: "", code: 0 });
    const extra = {};
    for (const k of ENV_KEYS) if (env && typeof env[k] === "string") extra[k] = env[k];
    const safeArgs = (Array.isArray(args) ? args : []).filter((a) => typeof a === "string" && /^[A-Za-z-]{1,40}$/.test(a)).slice(0, 3);
    const child = spawn(exe, safeArgs, { env: { ...process.env, ...extra }, stdio: ["pipe", "pipe", "ignore"], windowsHide: true });
    running.set(id, child);
    let out = "";
    const timer = setTimeout(() => child.kill(), HOOK_MS);
    child.stdout.on("data", (c) => (out += c));
    const finish = (code) => {
      clearTimeout(timer);
      running.delete(id);
      resolve({ stdout: out, code: Number.isInteger(code) ? code : 0 });
    };
    child.on("error", () => finish(0));
    child.on("close", finish);
    child.stdin.on("error", () => {});
    child.stdin.end(typeof stdin === "string" ? stdin : "");
  });
}

function cancel({ id }) {
  const child = running.get(id);
  if (child) child.kill();
}

// ── Ronron's chat ─────────────────────────────────────────────────────────────

const token = crypto.randomBytes(24).toString("hex");
let server = null;
let pipe = "";
let registry = "";

async function folders() {
  try {
    const list = await vscode.commands.executeCommand("ronron.remote.folders");
    if (Array.isArray(list)) return list;
  } catch {
    // The remote extension is not there (yet).
  }
  return [];
}

async function answer(msg) {
  if (!msg || msg.token !== token) return { ok: false, error: "refused" };
  if (msg.type === "folders") {
    return { ok: true, remote: vscode.env.remoteName || "", window: vscode.workspace.name || "", folders: await folders() };
  }
  if (msg.type === "chat") {
    try {
      const r = await vscode.commands.executeCommand("ronron.remote.chat", { folder: msg.folder, prompt: msg.prompt, resume: msg.resume });
      if (r && typeof r.text === "string") return { ok: true, text: r.text, sessionId: r.sessionId || null };
      return { ok: false, error: (r && r.error) || "No answer.", sessionId: (r && r.sessionId) || null };
    } catch (e) {
      return { ok: false, error: `Install "Ronron for VS Code (remote)" in this workspace's remote (${e.message}).` };
    }
  }
  return { ok: false, error: "unknown request" };
}

function onConnection(conn) {
  let buf = "";
  conn.setEncoding("utf8");
  conn.on("data", async (c) => {
    buf += c;
    const nl = buf.indexOf("\n");
    if (nl < 0) return;
    const line = buf.slice(0, nl);
    buf = "";
    let msg = null;
    try {
      msg = JSON.parse(line);
    } catch {
      return conn.destroy();
    }
    const reply = await answer(msg);
    if (!conn.destroyed) conn.end(JSON.stringify(reply) + "\n");
  });
  conn.on("error", () => {});
}

function startServer() {
  const dir = path.join(ronronDir(), "vscode");
  fs.mkdirSync(dir, { recursive: true });
  const id = `${process.pid}-${crypto.randomBytes(4).toString("hex")}`;
  if (process.platform === "win32") {
    pipe = `\\\\.\\pipe\\ronron-vscode-${id}`;
  } else {
    const run = process.env.XDG_RUNTIME_DIR && path.isAbsolute(process.env.XDG_RUNTIME_DIR) ? process.env.XDG_RUNTIME_DIR : dir;
    pipe = path.join(run, `ronron-vscode-${id}.sock`);
  }
  registry = path.join(dir, `${id}.json`);
  server = net.createServer(onConnection);
  server.listen(pipe, () => {
    if (process.platform !== "win32") {
      try {
        fs.chmodSync(pipe, 0o600);
      } catch {
        // Its folder is the user's own.
      }
    }
    fs.writeFileSync(registry, JSON.stringify({ pid: process.pid, pipe, token, remote: vscode.env.remoteName || "", window: vscode.workspace.name || "" }), { mode: 0o600 });
  });
  server.on("error", () => {});
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("ronron.local.hook", (req) => hook(req || {})),
    vscode.commands.registerCommand("ronron.local.cancel", (req) => cancel(req || {})),
  );
  // The chat only makes sense for a remote window: on the computer, Ronron asks Claude Code itself.
  if (vscode.env.remoteName) startServer();
}

function deactivate() {
  for (const child of running.values()) child.kill();
  try {
    server && server.close();
    if (process.platform !== "win32") fs.unlinkSync(pipe);
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
