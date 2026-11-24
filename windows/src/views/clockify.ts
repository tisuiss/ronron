// The Clockify pill's card: the running timer (its time going by) with Stop,
// or what to start and on which project; the day's and the week's hours; the
// last things worked on, each started again in one click; and the island's
// suggestion when there is one (island/clockify.ts). The details: more to
// start again, and the folders an agent works in with their project.

import { h, svg, dot } from "./dom";
import { ICONS } from "./icons";
import { State } from "../core/state";
import { Bridge } from "../core/bridge";
import { CLOCKIFY_COLOR, folderName, hm, hms, readClockify, recent, totals, type ClockifyData, type ClockifyEntry, type Suggestion } from "../core/clockify";
import { clockifySuggestion, dismissClockify } from "../island/clockify";
import { t } from "../i18n/i18n";

let lastError: string | null = null;

async function act(run: () => Promise<void>, button?: HTMLButtonElement) {
  if (button) button.disabled = true;
  try {
    await run();
    lastError = null;
  } catch (err) {
    lastError = String(err).replace(/^Error:\s*/, "");
    State.notify();
  } finally {
    if (button) button.disabled = false;
  }
}

const start = (description: string, projectId: string | null, button?: HTMLButtonElement, taskId: string | null = null) =>
  act(() => Bridge.clockifyStart(description, projectId, projectId ? taskId : null), button);

/** The task list of the project picked: empty and off until a project is picked, then its tasks. */
function taskSelect(project: HTMLSelectElement): HTMLSelectElement {
  const select = h("select", { class: "ck-select ck-task" }) as HTMLSelectElement;
  const fill = async () => {
    const projectId = project.value;
    select.replaceChildren(h("option", { value: "", text: projectId ? t("No task") : t("Pick a project first") }));
    select.disabled = true;
    if (!projectId) return;
    const tasks = await Bridge.clockifyTasks(projectId).catch(() => []);
    if (project.value !== projectId) return; // another project was picked meanwhile
    for (const task of tasks) select.append(h("option", { value: task.id, text: task.name }));
    select.disabled = tasks.length === 0;
  };
  project.addEventListener("change", () => void fill());
  void fill();
  return select;
}
const stop = (button?: HTMLButtonElement) => act(() => Bridge.clockifyStop(), button);

function saveFolder(folder: string, projectId: string | null) {
  const folders = { ...(State.settings.clockifyFolders ?? {}) };
  if (projectId) folders[folder] = projectId;
  else delete folders[folder];
  State.settings = { ...State.settings, clockifyFolders: folders };
  void Bridge.saveSettings(State.settings);
}

function projectSelect(data: ClockifyData, value: string | null): HTMLSelectElement {
  const select = h("select", { class: "ck-select" }) as HTMLSelectElement;
  select.append(h("option", { value: "", text: t("No project") }));
  for (const p of data.projects) select.append(h("option", { value: p.id, text: p.client ? `${p.name} · ${p.client}` : p.name }));
  select.value = value ?? "";
  return select;
}

function playButton(onclick: (b: HTMLButtonElement) => void, title: string): HTMLButtonElement {
  const b = h("button", { class: "ck-play", title }, svg(ICONS.play, 8)) as HTMLButtonElement;
  b.addEventListener("click", (e) => { e.stopPropagation(); onclick(b); });
  return b;
}

/** The running time, ticking each second while the card is on screen. */
function ticking(since: number): HTMLElement {
  const el = h("span", { class: "ck-elapsed", text: hms(Date.now() - since) });
  const timer = window.setInterval(() => {
    if (!el.isConnected) return window.clearInterval(timer);
    el.textContent = hms(Date.now() - since);
  }, 1000);
  return el;
}

function entryLabel(e: ClockifyEntry): string {
  return e.description || e.projectName || t("(no description)");
}

/** The suggestion and its buttons. */
function suggestionRow(s: Suggestion, data: ClockifyData): HTMLElement {
  const row = h("div", { class: "ck-suggest" });
  const later = h("button", { class: "link-btn", text: t("Not now"), onclick: () => dismissClockify(s.key) });
  if (s.kind === "long") {
    const hours = Math.floor((Date.now() - s.entry.start) / 3600_000);
    row.append(h("span", { text: t("The timer has been running for {n} h. Still on it?", { n: hours }) }));
    const b = h("button", { class: "link-btn", style: "color:#F4505E", text: t("Stop") }) as HTMLButtonElement;
    b.addEventListener("click", () => void stop(b).then(() => dismissClockify(s.key)));
    row.append(h("div", { class: "ck-suggest-actions" }, b, later));
    return row;
  }
  if (s.kind === "meeting") {
    row.append(h("span", { text: t("« {title} » is starting. Start a timer?", { title: s.title }) }));
    const select = projectSelect(data, null);
    const b = h("button", { class: "link-btn", style: `color:${CLOCKIFY_COLOR}`, text: t("Start") }) as HTMLButtonElement;
    b.addEventListener("click", () => void start(s.description, select.value || null, b).then(() => dismissClockify(s.key)));
    row.append(h("div", { class: "ck-suggest-actions" }, select, b, later));
    return row;
  }
  const project = s.projectId ? data.projects.find((p) => p.id === s.projectId) : null;
  row.append(h("span", {
    text: project
      ? t("{agent} is working in {folder}. Start the timer on {project}?", { agent: s.agent, folder: folderName(s.folder), project: project.name })
      : t("{agent} is working in {folder}. Start a timer?", { agent: s.agent, folder: folderName(s.folder) }),
  }));
  const select = projectSelect(data, s.projectId);
  const b = h("button", { class: "link-btn", style: `color:${CLOCKIFY_COLOR}`, text: t("Start") }) as HTMLButtonElement;
  b.addEventListener("click", () => {
    const projectId = select.value || null;
    // The project picked is kept for this folder: next time it is offered at once.
    if (projectId !== s.projectId) saveFolder(s.folder, projectId);
    void start(s.description, projectId, b).then(() => dismissClockify(s.key));
  });
  row.append(h("div", { class: "ck-suggest-actions" }, select, b, later));
  return row;
}

function header(data: ClockifyData): HTMLElement {
  const sum = totals(data);
  return h("div", { class: "int-head" }, dot(CLOCKIFY_COLOR, 7), h("b", { text: "Clockify" }),
    h("span", { text: t("{today} today · {week} this week", { today: hm(sum.today), week: hm(sum.week) }) }));
}

/** The running timer with Stop, or the line to start one. */
function nowRow(data: ClockifyData): HTMLElement {
  const r = data.running;
  if (r) {
    const b = h("button", { class: "ck-stop", title: t("Stop") }, h("i")) as HTMLButtonElement;
    b.addEventListener("click", (e) => { e.stopPropagation(); void stop(b); });
    return h("div", { class: "ck-now" },
      dot(r.projectColor || CLOCKIFY_COLOR, 6),
      h("div", { class: "ck-now-text" },
        h("span", { class: "int-name", style: "color:#e8e9eb", text: entryLabel(r) }),
        r.projectName && r.description ? h("span", { class: "int-sub", text: r.taskName ? `${r.projectName} · ${r.taskName}` : r.projectName }) : h("span")),
      ticking(r.start), b);
  }
  // As in Clockify: what, then the project, then its task.
  const input = h("input", { class: "todo-input", placeholder: t("What are you working on?"), spellcheck: "false" }) as HTMLInputElement;
  const project = projectSelect(data, null);
  const task = taskSelect(project);
  const go = playButton((b) => void start(input.value, project.value || null, b, task.value || null), t("Start"));
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") go.click();
  });
  return h("div", { class: "ck-start" }, input, h("div", { class: "ck-pick" }, project, task, go));
}

function recentRows(data: ClockifyData, n: number): HTMLElement {
  const rows = h("div", { class: "int-rows tight" });
  for (const e of recent(data, n)) {
    rows.append(h("div", { class: "int-row" },
      dot(e.projectColor || "#6B7079", 5),
      h("span", { class: "int-name", text: entryLabel(e) }),
      e.projectName && e.description ? h("span", { class: "int-sub", text: e.taskName ? `${e.projectName} · ${e.taskName}` : e.projectName }) : h("span"),
      h("span", { style: "margin-left:auto" }),
      playButton((b) => void start(e.description, e.projectId, b, e.taskId ?? null), t("Start again"))));
  }
  return rows;
}

export function clockifyCard(openDetail: () => void): HTMLElement {
  const data = readClockify(State.integrations.integration_clockify?.data);
  const card = h("div", { class: "int-card ck-card" });
  if (!data) return card;
  card.append(header(data));
  const s = clockifySuggestion();
  if (s) card.append(suggestionRow(s, data));
  card.append(nowRow(data));
  if (lastError) card.append(h("div", { class: "int-status", style: "color:#F4505E", text: lastError }));
  if (!s) card.append(recentRows(data, data.running ? 3 : 2));
  card.append(h("div", { class: "int-actions" }, h("span"), h("button", { class: "link-btn", text: t("Details"), onclick: openDetail })));
  return card;
}

export function clockifyDetail(close: () => void): HTMLElement {
  const data = readClockify(State.integrations.integration_clockify?.data);
  const root = h("div", { class: "int-card detail ck-card" });
  root.append(h("div", { class: "int-detail-head" },
    h("button", { class: "int-back", onclick: close }, svg(ICONS.chevronLeft, 10, { stroke: 2.4 })),
    dot(CLOCKIFY_COLOR, 6), h("b", { text: "Clockify" }),
    h("button", { class: "link-btn", style: `color:${CLOCKIFY_COLOR};margin-left:auto`, text: t("Open {name}", { name: "Clockify" }),
      onclick: () => void Bridge.openUrl("https://app.clockify.me/tracker") })));
  if (!data) return root;
  const body = h("div", { class: "int-detail-body", style: "overflow-y:auto;max-height:190px" });
  body.append(nowRow(data), recentRows(data, 8));
  // The folders the agents work in, and the project each one goes with.
  const folders = new Set<string>([
    ...Object.keys(State.settings.clockifyFolders ?? {}),
    ...State.tasks.map((t) => t.sessionCwd).filter((f): f is string => !!f),
  ]);
  if (folders.size > 0) {
    body.append(h("div", { class: "todo-list-name", text: t("Agents' folders") }));
    for (const folder of folders) {
      const select = projectSelect(data, State.settings.clockifyFolders?.[folder] ?? null);
      select.addEventListener("change", () => saveFolder(folder, select.value || null));
      body.append(h("div", { class: "int-row", title: folder }, h("span", { class: "int-name", text: folderName(folder) }), h("span", { style: "margin-left:auto" }), select));
    }
  }
  root.append(body);
  return root;
}
