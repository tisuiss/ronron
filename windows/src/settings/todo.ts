// Settings → Integrations → To Do: sign in with Microsoft (device code flow).
// The client ID and tenant fields above it are ordinary integration fields;
// this block shows who is signed in, or the code to enter at Microsoft.

import { Bridge, onEvent } from "../core/bridge";
import { h, clear } from "../views/dom";
import { t } from "../i18n/i18n";

type AuthEvent = { ok: true; account: string } | { ok: false; error: string };

/** The block on screen now: the `todo-auth` event goes to it alone. */
let current: ((e: AuthEvent) => void) | null = null;
let listening = false;

function listen() {
  if (listening) return;
  listening = true;
  void onEvent<AuthEvent>("todo-auth", (e) => current?.(e));
}

export function todoSignIn(): HTMLElement {
  listen();
  const root = h("div", { class: "todo-signin", style: "display:flex;flex-direction:column;gap:6px;padding-top:2px" });

  function signedIn(account: string | null) {
    clear(root);
    root.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px;color:#22c55e", text: account ? t("Signed in as {account}", { account }) : t("Connected") }),
      h("button", { text: t("Sign out"), onclick: async () => { await Bridge.todoSignOut(); signedOut(null); } })));
  }

  function signedOut(error: string | null) {
    clear(root);
    const button = h("button", { class: "primary", text: t("Sign in with Microsoft") }) as HTMLButtonElement;
    button.addEventListener("click", async () => {
      button.disabled = true;
      try {
        const code = await Bridge.todoSignIn();
        waiting(code.userCode, code.verificationUri);
      } catch (e) {
        signedOut(e instanceof Error ? e.message : String(e));
      }
    });
    root.append(h("div", { class: "row" },
      h("span", { style: "flex:1 1 auto;font-size:12px;color:#f4505e", text: t("Not signed in") }), button));
    if (error) root.append(h("div", { class: "hint", style: "color:#f5a524", text: error }));
  }

  function waiting(code: string, url: string) {
    clear(root);
    const copy = h("button", { text: t("Copy") }) as HTMLButtonElement;
    copy.addEventListener("click", () => {
      void navigator.clipboard?.writeText(code).then(() => (copy.textContent = t("Copied")));
    });
    root.append(
      h("div", { class: "hint", text: t("Go to {url} and enter the code {code}.", { url, code }) }),
      h("div", { class: "row" },
        h("code", { style: "font:600 15px ui-monospace,monospace;letter-spacing:2px;flex:1 1 auto", text: code }),
        copy,
        h("button", { class: "primary", text: t("Open {name}", { name: "Microsoft" }), onclick: () => void Bridge.openUrl(url) }),
        h("button", { text: t("Cancel"), onclick: () => { void Bridge.todoSignInCancel(); signedOut(null); } })),
      h("div", { class: "hint", text: t("Waiting for you to sign in…") }),
    );
    void navigator.clipboard?.writeText(code).then(() => (copy.textContent = t("Copied")));
  }

  current = (e) => {
    if (!root.isConnected) return;
    if (e.ok) signedIn(e.account || null);
    else signedOut(e.error);
  };

  void Bridge.todoAccount().then((account) => (account !== null ? signedIn(account) : signedOut(null)));
  return root;
}
