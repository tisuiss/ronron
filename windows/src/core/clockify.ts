// Clockify in the island — pure helpers over what src-tauri/src/clockify.rs
// sends (`running`, `entries`, `projects`): the day's and the week's totals,
// the entries to start again, and the suggestions the island offers. A
// suggestion is only ever a button: nothing here starts or stops a timer.

export interface ClockifyEntry {
  id: string;
  description: string;
  projectId: string | null;
  projectName: string;
  projectColor: string;
  taskId?: string | null;
  taskName?: string;
  /** ms since 1970. */
  start: number;
  /** null while it runs. */
  end: number | null;
}

export interface ClockifyProject {
  id: string;
  name: string;
  color: string;
  client: string;
}

export interface ClockifyData {
  running: ClockifyEntry | null;
  entries: ClockifyEntry[];
  projects: ClockifyProject[];
}

export const CLOCKIFY_ID = "integration_clockify";
export const CLOCKIFY_COLOR = "#03A9F4";

export function readClockify(data: unknown): ClockifyData | null {
  if (!data || typeof data !== "object") return null;
  const d = data as Record<string, unknown>;
  if (!Array.isArray(d.entries)) return null;
  return {
    running: d.running && typeof d.running === "object" ? (d.running as ClockifyEntry) : null,
    entries: d.entries as ClockifyEntry[],
    projects: Array.isArray(d.projects) ? (d.projects as ClockifyProject[]) : [],
  };
}

/** "1:05" for an hour and five minutes; "0:07". */
export function hm(ms: number): string {
  const mins = Math.max(0, Math.floor(ms / 60_000));
  return `${Math.floor(mins / 60)}:${String(mins % 60).padStart(2, "0")}`;
}

/** "1:05:09", for the running timer. */
export function hms(ms: number): string {
  const secs = Math.max(0, Math.floor(ms / 1000));
  return `${hm(secs * 1000)}:${String(secs % 60).padStart(2, "0")}`;
}

/** Local midnight today, and the Monday of this week. */
function dayStart(now: number): number {
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}
function weekStart(now: number): number {
  const d = new Date(dayStart(now));
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return d.getTime();
}

/** Time tracked today and this week (Monday on), the running timer included. */
export function totals(data: ClockifyData, now = Date.now()): { today: number; week: number } {
  const day = dayStart(now);
  const week = weekStart(now);
  let today = 0;
  let total = 0;
  for (const e of data.entries) {
    const end = e.end ?? now;
    total += Math.max(0, end - Math.max(e.start, week));
    today += Math.max(0, end - Math.max(e.start, day));
  }
  return { today, week: total };
}

/** The last different things worked on (description and project), newest first. */
export function recent(data: ClockifyData, n = 3): ClockifyEntry[] {
  const seen = new Set<string>();
  const out: ClockifyEntry[] = [];
  for (const e of data.entries) {
    if (e.end == null) continue;
    const key = `${e.description.trim().toLowerCase()}|${e.projectId ?? ""}`;
    if (key === "|" || seen.has(key)) continue;
    seen.add(key);
    out.push(e);
    if (out.length >= n) break;
  }
  return out;
}

/** The last part of a folder: "C:\\code\\ronron" → "ronron". */
export function folderName(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
}

export type Suggestion =
  /** An agent at work in a folder, no timer running. */
  | { kind: "agent"; key: string; agent: string; folder: string; projectId: string | null; description: string }
  /** A meeting starting, no timer running. */
  | { kind: "meeting"; key: string; title: string; description: string }
  /** A timer running for very long, or late in the evening. */
  | { kind: "long"; key: string; entry: ClockifyEntry };

export interface SuggestionInputs {
  now: number;
  data: ClockifyData | null;
  /** The agents' pills: name, folder, whether they are at work. */
  agents: { name: string; folder: string | null; busy: boolean }[];
  /** Calendar events (Microsoft 365, Google, Cal.com): id, title, start, end (ISO or ms). */
  events: { id: string; title: string; start: unknown; end: unknown }[];
  /** Settings → the project picked for each folder. */
  folders: Record<string, string>;
  /** The suggestions the user waved away. */
  dismissed: ReadonlySet<string>;
}

const LONG_MS = 9 * 3600_000;
const LATE_HOUR = 21;

/** What the island offers now, if anything. */
export function suggestion(input: SuggestionInputs): Suggestion | null {
  const { now, data } = input;
  if (!data) return null;
  const day = new Date(now).toDateString();
  const fresh = (key: string) => !input.dismissed.has(key);
  if (data.running) {
    const r = data.running;
    const key = `long:${r.id}`;
    const late = new Date(now).getHours() >= LATE_HOUR && now - r.start > 30 * 60_000;
    if ((now - r.start >= LONG_MS || late) && fresh(key)) return { kind: "long", key, entry: r };
    return null;
  }
  for (const e of input.events) {
    const start = new Date(typeof e.start === "number" ? e.start : String(e.start)).getTime();
    const end = new Date(typeof e.end === "number" ? e.end : String(e.end)).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end)) continue;
    // From two minutes before it starts to a quarter of an hour in.
    if (start - 2 * 60_000 <= now && now < Math.min(end, start + 15 * 60_000)) {
      const key = `meeting:${e.id}`;
      if (fresh(key)) return { kind: "meeting", key, title: e.title, description: e.title };
    }
  }
  for (const a of input.agents) {
    if (!a.busy || !a.folder) continue;
    const key = `agent:${a.folder}:${day}`;
    if (!fresh(key)) continue;
    const mapped = input.folders[a.folder];
    const projectId = mapped && data.projects.some((p) => p.id === mapped) ? mapped : null;
    return { kind: "agent", key, agent: a.name, folder: a.folder, projectId, description: folderName(a.folder) };
  }
  return null;
}

export type ExportRange = "thisWeek" | "lastWeek" | "thisMonth" | "lastMonth";

/** The local days an export covers, as [from, to) in ms: weeks from Monday, months from the 1st. */
export function exportRange(range: ExportRange, now = Date.now()): { from: number; to: number } {
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  if (range === "thisWeek" || range === "lastWeek") {
    const monday = new Date(today);
    monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7) - (range === "lastWeek" ? 7 : 0));
    const next = new Date(monday);
    next.setDate(next.getDate() + 7);
    return { from: monday.getTime(), to: next.getTime() };
  }
  const first = new Date(today.getFullYear(), today.getMonth() - (range === "lastMonth" ? 1 : 0), 1);
  const next = new Date(first.getFullYear(), first.getMonth() + 1, 1);
  return { from: first.getTime(), to: next.getTime() };
}
