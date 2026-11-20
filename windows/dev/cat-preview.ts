// Dev only: Ronron in every state, at a few sizes (npx vite, then /dev/cat-preview.html).
import { BotEngine, hexToRGB } from "../src/cat/engine";
import type { BotStateName } from "../src/core/layout";
const out = document.getElementById("out")!;
function cell(row: HTMLElement, label: string, size: number, setup: (e: BotEngine) => void, steps = 90) {
  const wrap = document.createElement("div");
  const c = document.createElement("canvas");
  const dpr = 2;
  c.width = size * dpr; c.height = size * dpr; c.style.width = c.style.height = `${size}px`;
  const e = new BotEngine();
  setup(e);
  for (let i = 0; i < steps; i++) e.update(1 / 60);
  const x = c.getContext("2d")!;
  x.scale(dpr, dpr);
  e.draw(x, size, size);
  wrap.append(c, Object.assign(document.createElement("div"), { textContent: label }));
  row.append(wrap);
}
const states: BotStateName[] = ["idle", "working", "thinking", "approval", "question", "error", "finished", "ratelimit", "sleeping", "dizzy"];
let row = document.createElement("div"); row.className = "row"; out.append(row);
for (const s of states) cell(row, s, 120, (e) => e.setState(s, true));
row = document.createElement("div"); row.className = "row"; out.append(row);
cell(row, "big idle", 260, (e) => e.setState("idle", true));
cell(row, "yaw .5", 160, (e) => { e.setState("idle", true); e.yaw = 0.5; }, 1);
cell(row, "yaw -.7", 160, (e) => { e.setState("idle", true); e.yaw = -0.7; }, 1);
cell(row, "morph .5", 160, (e) => { e.setState("idle", true); e.morph = 0.5; }, 1);
cell(row, "box", 160, (e) => { e.setState("idle", true); e.morph = 1; }, 1);
cell(row, "love", 160, (e) => { e.setState("idle", true); e.triggerEmote("love", 5); }, 30);
cell(row, "small 40", 40, (e) => e.setState("idle", true));
cell(row, "small 28", 28, (e) => e.setState("working", true));
row = document.createElement("div"); row.className = "row"; out.append(row);
for (const col of ["#F5F6F8", "#635BFF", "#FF6D5A", "#24292F", "#22C55E", "#1DB954"]) {
  cell(row, `mini ${col}`, 60, (e) => { e.isMini = true; e.bodyColor = hexToRGB(col); e.setState("idle", true); });
}
document.title = "ready";

// The launch greeting at a few moments.
import { Greeting, GREETING_W, GREETING_H } from "../src/cat/greeting";
row = document.createElement("div"); row.className = "row"; out.append(row);
for (const t of [1.4, 2.0, 2.6, 3.6]) {
  const c = document.createElement("canvas");
  c.width = GREETING_W; c.height = GREETING_H; c.style.width = `${GREETING_W / 2}px`;
  const g = new Greeting();
  (g as unknown as { startMs: number }).startMs = performance.now() - t * 1000;
  g.draw(c.getContext("2d")!);
  row.append(c);
}
