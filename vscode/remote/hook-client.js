#!/usr/bin/env node
// Ronron for VS Code: the hook Claude Code runs in the remote machine. It hands the
// event to this machine's VS Code window (the Ronron extension), which hands it to
// the Ronron on the computer through VS Code's own tunnel, and prints the answer.
//
// Never block Claude Code: no window, no answer in time, anything odd — it exits 0
// with nothing, and Claude Code carries on as if Ronron were not there.
"use strict";

const fs = require("fs");
const net = require("net");
const os = require("os");
const path = require("path");
const { pickBridge, pickEnv } = require("./lib.js");

const DIR = path.join(os.homedir(), ".ronron");
const CONNECT_MS = 300;
const STDIN_MS = 2000;

const args = process.argv.slice(2);
const event = args.find((a) => !a.startsWith("-")) || "";
/** A permission waits for a click on the island (Ronron gives up at 110 s). */
const budget = event === "PermissionRequest" ? 115_000 : 3_000;
const done = () => process.exit(0);
setTimeout(done, budget + STDIN_MS).unref();

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return e.code === "EPERM";
  }
}

function bridges() {
  const dir = path.join(DIR, "bridges");
  let names = [];
  try {
    names = fs.readdirSync(dir).filter((n) => n.endsWith(".json"));
  } catch {
    return [];
  }
  const out = [];
  for (const n of names) {
    try {
      const b = JSON.parse(fs.readFileSync(path.join(dir, n), "utf8"));
      if (typeof b.socket === "string" && Number.isInteger(b.pid) && alive(b.pid)) out.push(b);
    } catch {
      // A half-written file: skipped.
    }
  }
  return out;
}

let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (c) => (input += c));
process.stdin.on("end", send);
setTimeout(send, STDIN_MS).unref();

let sent = false;
function send() {
  if (sent) return;
  sent = true;
  let cwd = "";
  try {
    cwd = JSON.parse(input).cwd || "";
  } catch {
    // Not JSON: the relay on the computer decides.
  }
  const bridge = pickBridge(bridges(), cwd || process.cwd());
  if (!bridge) return done();
  const socket = net.connect(bridge.socket);
  const timer = setTimeout(() => socket.destroy(), CONNECT_MS);
  let reply = "";
  socket.setEncoding("utf8");
  socket.on("connect", () => {
    clearTimeout(timer);
    socket.write(JSON.stringify({ token: bridge.token, args, stdin: input, env: pickEnv(process.env) }) + "\n");
  });
  socket.on("data", (c) => {
    reply += c;
    const nl = reply.indexOf("\n");
    if (nl < 0) return;
    try {
      const r = JSON.parse(reply.slice(0, nl));
      if (typeof r.stdout === "string" && r.stdout) process.stdout.write(r.stdout);
      socket.end();
      process.exitCode = Number.isInteger(r.code) ? r.code : 0;
      setImmediate(() => process.exit(process.exitCode));
    } catch {
      done();
    }
  });
  socket.on("error", done);
  socket.on("close", () => setImmediate(() => process.exit(process.exitCode || 0)));
}
