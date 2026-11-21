// Ronron for VS Code (remote side): the pure parts, shared with hook-client.js and
// tested in windows/tests/vscode-bridge.test.mjs.
"use strict";

/** The events Ronron reacts to, with the hook timeout written to settings.json
 *  (src-tauri/src/hooks.rs HOOK_EVENTS: keep both lists the same). */
const HOOK_EVENTS = [
  ["SessionStart", 10],
  ["SessionEnd", 10],
  ["UserPromptSubmit", 10],
  ["PreToolUse", 10],
  ["PostToolUse", 10],
  ["PostToolUseFailure", 10],
  ["PermissionRequest", 120],
  ["Notification", 10],
  ["Stop", 10],
  ["StopFailure", 10],
  ["SubagentStart", 10],
  ["SubagentStop", 10],
];

/** What marks a Ronron entry in settings.json (the same as on the computer). */
const MARKER = "ronron-hook";

/** Only these reach the computer's relay with the event. */
const ENV_KEYS = ["CLAUDE_CODE_ENTRYPOINT", "CLAUDE_PROJECT_DIR", "TERM_PROGRAM"];

function isOurs(entry) {
  return Array.isArray(entry && entry.hooks) &&
    entry.hooks.some((h) => typeof (h && h.command) === "string" && h.command.includes(MARKER));
}

/** settings.json with Ronron's hooks (command(event) for each), everything else kept.
 *  A "hooks" that is not what Claude Code documents is refused, never replaced. */
function mergeHooks(settings, command) {
  const root = settings && typeof settings === "object" && !Array.isArray(settings) ? { ...settings } : {};
  const hooks = root.hooks === undefined ? {} : root.hooks;
  if (!hooks || typeof hooks !== "object" || Array.isArray(hooks)) throw new Error('"hooks" is not an object');
  const next = { ...hooks };
  for (const [event, timeout] of HOOK_EVENTS) {
    const list = next[event] === undefined ? [] : next[event];
    if (!Array.isArray(list)) throw new Error(`"hooks"."${event}" is not a list`);
    next[event] = [...list.filter((e) => !isOurs(e)), { hooks: [{ type: "command", command: command(event), timeout }] }];
  }
  root.hooks = next;
  return root;
}

/** Ronron's hooks are all there. */
function hooksInstalled(settings) {
  const hooks = settings && settings.hooks;
  if (!hooks || typeof hooks !== "object") return false;
  return HOOK_EVENTS.every(([event]) => Array.isArray(hooks[event]) && hooks[event].some(isOurs));
}

/** The window whose folder holds `cwd` (the deepest; the most recent of windows on the
 *  same folder — an older one may be left over from a window whose tunnel is gone),
 *  else the most recent one. */
function pickBridge(bridges, cwd) {
  const norm = (p) => String(p || "").replace(/\/+$/, "");
  const inside = (folder) => {
    const f = norm(folder);
    return f !== "" && (norm(cwd) === f || norm(cwd).startsWith(f + "/"));
  };
  let best = null;
  let bestLen = -1;
  for (const b of bridges) {
    for (const folder of b.folders || []) {
      const len = norm(folder).length;
      const newer = best !== null && len === bestLen && (b.started || 0) > (best.started || 0);
      if (inside(folder) && (len > bestLen || newer)) {
        best = b;
        bestLen = len;
      }
    }
  }
  if (best) return best;
  return [...bridges].sort((a, b) => (b.started || 0) - (a.started || 0))[0] || null;
}

/** The hook's environment, cut down to what the computer's relay reads. */
function pickEnv(env) {
  const out = {};
  for (const k of ENV_KEYS) if (typeof env[k] === "string") out[k] = env[k];
  return out;
}

/** The answer's text and session from `claude -p --output-format json`. */
function parseClaude(stdout) {
  let json;
  try {
    json = JSON.parse(String(stdout).trim());
  } catch {
    return { error: "Claude Code gave no answer." };
  }
  const text = typeof json.result === "string" ? json.result.trim() : "";
  const sessionId = typeof json.session_id === "string" ? json.session_id : null;
  if (json.is_error) return { error: text || "Claude Code could not answer.", sessionId };
  if (!text) return { error: "Claude Code gave no answer.", sessionId };
  return { text, sessionId };
}

/** The shell wrapper Claude Code runs as its hook: the VS Code server's Node, else any. */
function wrapperScript(dir) {
  const q = (s) => `'${String(s).replace(/'/g, `'\\''`)}'`;
  return [
    "#!/bin/sh",
    "# Ronron for VS Code: hands Claude Code's hook event to the Ronron on your computer.",
    "# Never holds Claude Code up: no VS Code window, no Ronron, it exits at once.",
    `D=${q(dir)}`,
    'N=$(cat "$D/node" 2>/dev/null)',
    '[ -x "$N" ] || N=$(command -v node 2>/dev/null)',
    '[ -x "$N" ] || exit 0',
    'exec "$N" "$D/hook-client.js" "$@"',
    "",
  ].join("\n");
}

module.exports = { HOOK_EVENTS, MARKER, ENV_KEYS, mergeHooks, hooksInstalled, pickBridge, pickEnv, parseClaude, wrapperScript };
