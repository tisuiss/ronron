// Dev only: the To Do card and its detail with sample tasks (npx vite, then /dev/todo-preview.html).
import "../src/style.css";
import { State } from "../src/core/state";
import { setLanguage } from "../src/i18n/i18n";
import { todoCard, todoDetail } from "../src/views/todo";
const day = (n: number) => { const d = new Date(); d.setDate(d.getDate() + n); return d.toISOString().slice(0, 10); };
setLanguage("fr");
State.integrations["integration_todo"] = {
  data: {
    lists: [{ id: "L1", name: "Tâches", default: true }, { id: "L2", name: "Fiduciaire", default: false }, { id: "L3", name: "Maison", default: false }],
    tasks: [
      { id: "1", listId: "L2", list: "Fiduciaire", title: "Relancer devis Metral", importance: "high", due: day(-1) },
      { id: "2", listId: "L1", list: "Tâches", title: "Renouveler certificat SSL", importance: "normal", due: day(0) },
      { id: "3", listId: "L2", list: "Fiduciaire", title: "Préparer onboarding client", importance: "normal", due: day(2) },
      { id: "4", listId: "L3", list: "Maison", title: "Commander le chlore piscine", importance: "normal", due: null },
      { id: "5", listId: "L1", list: "Tâches", title: "Mettre à jour NetBox", importance: "high", due: null },
    ],
    webUrl: "https://to-do.office.com/tasks/",
  },
  error: null, loaded: true, configured: true,
} as never;
for (const [el, tall] of [[todoCard(() => {}), false], [todoDetail(() => {}), true]] as const) {
  const f = document.createElement("div"); f.className = tall ? "frame tall" : "frame"; f.append(el); document.body.append(f);
}
document.title = "ready";
