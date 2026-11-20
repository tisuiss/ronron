// Dev only: the three Microsoft 365 spaces and the header tabs with sample data
// (npx vite --port 1420, then /dev/spaces-preview.html). The circle marks where Ronron sits.
import "../src/style.css";
import { State } from "../src/core/state";
import { setLanguage } from "../src/i18n/i18n";
import { VIEW_LAYOUTS, type IslandViewName } from "../src/core/layout";
import { buildHeader, buildViews, type ViewActions } from "../src/views/views";
import { buildAgendaSpace, buildMailSpace, buildTodoSpace } from "../src/views/spaces";

setLanguage("fr");
const now = Date.now();
const at = (h: number, m: number, day = 0) => { const d = new Date(); d.setDate(d.getDate() + day); d.setHours(h, m, 0, 0); return d.toISOString(); };
const day = (n: number) => { const d = new Date(); d.setDate(d.getDate() + n); return d.toISOString().slice(0, 10); };
State.settings.headerPills = ["integration_calendar", "integration_mail", "integration_todo"];
State.integrations["integration_calendar"] = { data: { rangeStart: now - 86_400_000, rangeEnd: now + 30 * 86_400_000, bookings: [
  { id: "1", title: "Daily équipe", start: at(9, 0), end: at(9, 15) },
  { id: "2", title: "Point client Metral", start: new Date(now + 25 * 60_000).toISOString(), end: new Date(now + 55 * 60_000).toISOString(), attendeeName: "Anne Martin", joinUrl: "https://teams.microsoft.com/x", webUrl: "https://outlook.office.com/calendar/" },
  { id: "3", title: "Revue NetBox", start: at(16, 0), end: at(17, 0), webUrl: "https://outlook.office.com/calendar/" },
  { id: "4", title: "Réunion APE", start: at(18, 15, 1), end: at(19, 0, 1) },
] }, error: null, loaded: true, configured: true } as never;
const mail = (i: number, name: string, subject: string, mins: number, unread: boolean, flagged = false) =>
  ({ entryId: `e${i}`, storeId: "graph", account: "greg", senderName: name, address: `${name.split(" ")[0].toLowerCase()}@example.ch`, subject, date: now - mins * 60_000, unread, flagged, local: "" });
State.integrations["integration_mail"] = { data: { running: true, windows: true, unread: 3, shown: 12, maxShown: 60, messages: [
  mail(1, "Anne Martin", "Devis Metral — validation", 12, true, true),
  mail(2, "Veeam", "Rapport quotidien VBO : succès", 50, true),
  mail(3, "Infomaniak", "Votre facture de novembre", 140, true),
  mail(4, "Paul Durand", "Re: onboarding client", 300, false),
  mail(5, "GitHub", "[tisuiss/ronron] Release 0.6.1", 900, false),
] }, error: null, loaded: true, configured: true } as never;
State.integrations["integration_todo"] = { data: {
  lists: [{ id: "L1", name: "Tâches", default: true }, { id: "L2", name: "Fiduciaire", default: false }, { id: "L3", name: "Maison", default: false }],
  tasks: [
    { id: "1", listId: "L2", list: "Fiduciaire", title: "Relancer le devis Metral", importance: "high", due: day(-1) },
    { id: "2", listId: "L1", list: "Tâches", title: "Renouveler le certificat SSL", importance: "normal", due: day(0) },
    { id: "3", listId: "L2", list: "Fiduciaire", title: "Préparer l'onboarding client", importance: "normal", due: day(2) },
    { id: "4", listId: "L3", list: "Maison", title: "Commander le chlore piscine", importance: "normal", due: day(4) },
    { id: "5", listId: "L1", list: "Tâches", title: "Mettre à jour NetBox", importance: "normal", due: day(0) },
  ],
  webUrl: "https://to-do.office.com/tasks/",
}, error: null, loaded: true, configured: true } as never;
State.mode = "expanded";

const noop = () => {};
const actions = new Proxy({}, { get: () => noop }) as ViewActions;
const spaces: [IslandViewName, typeof buildAgendaSpace][] = [["agenda", buildAgendaSpace], ["inbox", buildMailSpace], ["tasks", buildTodoSpace]];
for (const [name, build] of spaces) {
  State.view = name;
  const island = document.createElement("div");
  island.className = "island";
  const content = document.createElement("div");
  content.id = "content";
  const header = buildHeader(actions);
  const views = document.createElement("div");
  views.id = "views";
  const view = build(actions);
  view.el.classList.add("on");
  views.append(view.el);
  content.append(header.el, views);
  const cat = document.createElement("div");
  cat.className = "cat";
  cat.style.left = `${VIEW_LAYOUTS[name].botX}px`;
  cat.style.top = `${VIEW_LAYOUTS[name].botY}px`;
  island.append(content, cat);
  document.body.append(island);
  header.sync();
  view.sync();
  if (name === "inbox") (view.el.querySelector(".sp-mail-row") as HTMLElement).click();
}
// The overview with a pill open: the card takes the island, the pills in a column.
State.settings.mainPill = "integration_claude";
State.settings.activeIntegrations = ["integration_github", "integration_spotify", "integration_vercel", "integration_n8n"];
State.loadIntegrationTasks();
State.setFocus("integration_github");
State.pillOpen = true;
State.view = "overview";
{
  const island = document.createElement("div");
  island.className = "island";
  const content = document.createElement("div");
  content.id = "content";
  const header = buildHeader(actions);
  const views = document.createElement("div");
  views.id = "views";
  const view = buildViews(actions, noop).get("overview")!;
  view.el.classList.add("on");
  views.append(view.el);
  content.append(header.el, views);
  const cat = document.createElement("div");
  cat.className = "cat";
  cat.style.left = `${VIEW_LAYOUTS.overview.botX}px`;
  cat.style.top = "171px";
  island.append(content, cat);
  document.body.append(island);
  header.sync();
  view.sync();
}
document.title = "ready";
