// Ronron for VS Code (vscode/ in the repo): the remote side's hook relay and settings
// merge, and the chat's VS Code provider in the island.

import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import net from "node:net";
import { visibleProviders } from "../src/core/providers.ts";
import { chipLabel } from "../src/core/subscriptions.ts";

const require = createRequire(import.meta.url);
const lib = require("../../vscode/remote/lib.js");
const CLIENT = new URL("../../vscode/remote/hook-client.js", import.meta.url).pathname;

test("the remote hooks are the computer's: same events, same timeouts", () => {
  const rust = readFileSync(new URL("../src-tauri/src/hooks.rs", import.meta.url), "utf8");
  const listed = [...rust.matchAll(/\("(\w+)", (\d+)\)/g)].map((m) => [m[1], Number(m[2])]);
  assert.deepEqual(lib.HOOK_EVENTS, listed);
});

test("installing keeps every other setting and hook, and replaces only Ronron's", () => {
  const theirs = { model: "opus", hooks: { PreToolUse: [{ hooks: [{ type: "command", command: "other-tool" }] }, { hooks: [{ type: "command", command: "/old/ronron-hook PreToolUse" }] }] } };
  const next = lib.mergeHooks(theirs, (e) => `'/home/g/.ronron/ronron-hook' ${e}`);
  assert.equal(next.model, "opus");
  assert.equal(next.hooks.PreToolUse.length, 2);
  assert.equal(next.hooks.PreToolUse[0].hooks[0].command, "other-tool");
  assert.equal(next.hooks.PermissionRequest[0].hooks[0].timeout, 120);
  assert.ok(lib.hooksInstalled(next));
  assert.ok(!lib.hooksInstalled(theirs));
  assert.equal(theirs.hooks.PreToolUse.length, 2, "the original is not changed");
  assert.throws(() => lib.mergeHooks({ hooks: [] }, String));
  assert.throws(() => lib.mergeHooks({ hooks: { Stop: {} } }, String));
});

test("an event goes to the window whose folder holds the session", () => {
  const a = { socket: "a", folders: ["/home/g/ronron"], started: 1 };
  const b = { socket: "b", folders: ["/home/g/ronron/windows"], started: 2 };
  const c = { socket: "c", folders: ["/srv/site"], started: 3 };
  assert.equal(lib.pickBridge([a, b, c], "/home/g/ronron/windows/src").socket, "b");
  assert.equal(lib.pickBridge([a, b, c], "/home/g/ronron").socket, "a");
  assert.equal(lib.pickBridge([a, c], "/home/g/ronronx").socket, "c", "else the latest window");
  assert.equal(lib.pickBridge([], "/x"), null);
  assert.deepEqual(lib.pickEnv({ CLAUDE_CODE_ENTRYPOINT: "claude-vscode", HOME: "/root", SECRET: "x" }), { CLAUDE_CODE_ENTRYPOINT: "claude-vscode" });
});

test("Claude Code's answer and session are read", () => {
  assert.deepEqual(lib.parseClaude('{"result":" Bonjour ","session_id":"abc-123","is_error":false}'), { text: "Bonjour", sessionId: "abc-123" });
  assert.equal(lib.parseClaude('{"result":"quota","is_error":true}').error, "quota");
  assert.ok(lib.parseClaude("not json").error);
  assert.match(lib.wrapperScript("/home/g/.ronron"), /exit 0/);
});

function runClient(home, event, input) {
  return new Promise((resolve) => {
    const started = Date.now();
    const child = spawn(process.execPath, [CLIENT, event], { env: { ...process.env, HOME: home, USERPROFILE: home }, stdio: ["pipe", "pipe", "pipe"] });
    let out = "";
    child.stdout.on("data", (c) => (out += c));
    child.on("close", (code) => resolve({ out, code, ms: Date.now() - started }));
    child.stdin.end(input);
  });
}

test("with no VS Code window, the hook says nothing and lets Claude Code go at once", async () => {
  const home = mkdtempSync(join(tmpdir(), "ronron-hook-"));
  const r = await runClient(home, "PreToolUse", '{"cwd":"/x"}');
  assert.deepEqual([r.out, r.code], ["", 0]);
  assert.ok(r.ms < 1500, `${r.ms} ms`);
  rmSync(home, { recursive: true, force: true });
});

test("an event reaches the window and its answer comes back to Claude Code", { skip: process.platform === "win32" }, async () => {
  const home = mkdtempSync(join(tmpdir(), "ronron-hook-"));
  mkdirSync(join(home, ".ronron", "bridges"), { recursive: true });
  const socket = join(home, ".ronron", "bridge-1.sock");
  let got = null;
  const server = net.createServer((c) => {
    c.setEncoding("utf8");
    let buf = "";
    c.on("data", (d) => {
      buf += d;
      if (!buf.includes("\n")) return;
      got = JSON.parse(buf);
      c.end(JSON.stringify({ stdout: '{"decision":"allow"}', code: 0 }) + "\n");
    });
  });
  await new Promise((r) => server.listen(socket, r));
  writeFileSync(join(home, ".ronron", "bridges", "1.json"), JSON.stringify({ pid: process.pid, socket, token: "tok", folders: ["/srv/app"], started: 1 }));
  const r = await runClient(home, "PermissionRequest", '{"cwd":"/srv/app","tool_name":"Bash"}');
  server.close();
  assert.equal(r.out, '{"decision":"allow"}');
  assert.equal(got.token, "tok");
  assert.deepEqual(got.args, ["PermissionRequest"]);
  assert.match(got.stdin, /tool_name/);
  rmSync(home, { recursive: true, force: true });
});

test("the chat offers VS Code only while a remote window is open, named by its folder", () => {
  const settings = { chatProvider: "anthropic", chatSubscriptions: {}, ollamaUrl: "", lmstudioUrl: "", customUrl: "" };
  assert.ok(!visibleProviders(settings).some((p) => p.id === "vscode"));
  assert.ok(visibleProviders(settings, new Set(["vscode"])).some((p) => p.id === "vscode"));
  assert.equal(chipLabel({ ...settings, chatProvider: "vscode" }, "1234-ab|/home/g/ronron"), "ronron");
});
