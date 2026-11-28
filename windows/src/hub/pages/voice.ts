// « OK Ronron »: say it, then ask anything. Turning it on, how Ronron answers
// out loud, and what leaves the computer. Port of the Mac's Settings → Voice.

import { h, clear } from "../../views/dom";
import { t } from "../../i18n/i18n";
import { Bridge } from "../../core/bridge";
import { FlowBridge, type VoiceOutput, type VoiceProvider, type VoiceTestView } from "../../core/flow";
import { onEvent } from "../../core/bridge";
import type { HubContext } from "../shell";
import { button, card, divider, label, row, segmented, select, toggle } from "../ui";
import { modelsControl } from "./flowsettings";

const ELEVEN = "elevenlabs-api-key";
/** Where secrets.rs keeps the keys on this OS (same words as AI agents → Chat). */
const KEY_STORE = navigator.userAgent.includes("Windows") ? "Windows Credential Manager" : "Secret Service (GNOME Keyring, KWallet)";

/** The loudness on a 0–100 scale (decibels from −60 to 0), so a quiet voice still moves the bar. */
function meterPct(rms: number): number {
  const db = 20 * Math.log10(Math.max(rms, 1e-6));
  return Math.max(0, Math.min(100, ((db + 60) / 60) * 100));
}

/** The test on screen now: the `voice-test` events go to it alone. */
let testView: ((v: VoiceTestView) => void) | null = null;
let testListening = false;

/**
 * Voice → Test: the microphone's level against the threshold speech must pass,
 * then what Parakeet understood and whether it counts as « OK Ronron ».
 * Nothing heard is kept: it only shows here.
 */
/** Words of a heard phrase as Rust counts them (voice/wake.rs `usable_phrase`): 1 to 6, at least 4 letters. */
function learnable(heard: string): boolean {
  const words = heard.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "").split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  return words.length >= 1 && words.length <= 6 && words.join("").length >= 4;
}

const LEARN_TIMES = 3;
const MAX_VARIANTS = 8;

function voiceTest(ctx: HubContext, variants: string[]): HTMLElement {
  if (!testListening) {
    testListening = true;
    void onEvent<VoiceTestView>("voice-test", (v) => testView?.(v));
  }
  const fill = h("div", { class: "vt-fill" });
  const mark = h("div", { class: "vt-mark", title: t("Speech starts here") });
  const meter = h("div", { class: "vt-meter" }, fill, mark);
  const state = h("div", { class: "hub-note" });
  const results = h("div", { class: "vt-results" });
  const box = h("div", { class: "vt-box", style: "display:none" }, meter, state, results);
  let running = false;
  /** Learning: what Parakeet wrote each time « OK Ronron » was said, kept once there are three. */
  let learning: string[] | null = null;
  const button = h("button", { class: "hub-btn primary small", text: t("Test") }) as HTMLButtonElement;
  const learnButton = h("button", { class: "hub-btn small", text: t("Learn") }) as HTMLButtonElement;
  const stopTest = () => {
    running = false;
    learning = null;
    button.textContent = t("Test");
    learnButton.textContent = t("Learn");
    void FlowBridge.voiceTestStop();
  };
  learnButton.addEventListener("click", async () => {
    if (running) return stopTest();
    clear(results);
    learning = [];
    state.textContent = t("Say « OK Ronron » alone, {n} times, with a pause after each.", { n: LEARN_TIMES });
    state.style.color = "";
    box.style.display = "";
    try {
      await FlowBridge.voiceTestStart();
      running = true;
      learnButton.textContent = t("Stop");
    } catch (err) {
      learning = null;
      state.textContent = String(err).replace(/^Error:\s*/, "");
      state.style.color = "var(--hub-red, #f4505e)";
    }
  });
  button.addEventListener("click", async () => {
    if (running) return stopTest();
    clear(results);
    learning = null;
    state.textContent = t("Say « OK Ronron », then a question. The test stops after 30 seconds.");
    state.style.color = "";
    box.style.display = "";
    try {
      await FlowBridge.voiceTestStart();
      running = true;
      button.textContent = t("Stop");
    } catch (err) {
      state.textContent = String(err).replace(/^Error:\s*/, "");
      state.style.color = "var(--hub-red, #f4505e)";
    }
  });
  testView = (v) => {
    if (!box.isConnected) {
      testView = null;
      void FlowBridge.voiceTestStop();
      return;
    }
    if (v.error) {
      state.textContent = v.error;
      state.style.color = "var(--hub-red, #f4505e)";
    }
    if (v.done) {
      running = false;
      button.textContent = t("Test");
      fill.style.width = "0%";
      return;
    }
    fill.style.width = `${meterPct(v.level)}%`;
    fill.classList.toggle("on", v.level > v.threshold);
    mark.style.left = `${meterPct(v.threshold)}%`;
    if (v.heard != null && learning) {
      const heard = v.heard.trim();
      if (!heard || !learnable(heard)) {
        results.prepend(h("div", { class: "vt-row" }, h("span", { class: "vt-no", text: "·" }),
          h("span", { text: heard ? `« ${heard} »` : t("(nothing understood)") }),
          h("span", { class: "hub-note", text: t("Too long or too short: say it alone") })));
        return;
      }
      learning.push(heard);
      results.prepend(h("div", { class: "vt-row" }, h("span", { class: "vt-ok", text: `${learning.length}/${LEARN_TIMES}` }),
        h("span", { text: `« ${heard} »` })));
      if (learning.length >= LEARN_TIMES) {
        const learned = learning;
        stopTest();
        const all = [...variants];
        for (const x of learned) if (!all.some((y) => y.toLowerCase() === x.toLowerCase())) all.push(x);
        void ctx.setPrefs((p) => (p.wakeVariants = all.slice(-MAX_VARIANTS))).then(() => ctx.navigate("voice"));
      }
      return;
    }
    if (v.heard != null) {
      const ok = v.wake === true;
      results.prepend(h("div", { class: "vt-row" },
        h("span", { class: ok ? "vt-ok" : "vt-no", text: ok ? "✓" : "✗" }),
        h("span", { text: v.heard ? `« ${v.heard} »` : t("(nothing understood)") }),
        h("span", { class: "hub-note", text: ok ? t("Counts as « OK Ronron »") : t("Not « OK Ronron »") })));
      while (results.childElementCount > 5) results.lastElementChild?.remove();
    }
  };
  // What was learned: each phrase wakes Ronron too; ✕ forgets one.
  const learned = h("div", { class: "vt-learned" });
  for (const v of variants) {
    learned.append(h("span", { class: "vt-chip" }, h("span", { text: `« ${v} »` }),
      h("button", { class: "vt-x", title: t("Remove"), text: "✕",
        onclick: () => void ctx.setPrefs((p) => (p.wakeVariants = (p.wakeVariants ?? []).filter((x) => x !== v))).then(() => ctx.navigate("voice")) })));
  }
  return h("div", {},
    row(t("Test the microphone"), t("The bar must pass the line when you speak; then what Parakeet understood shows below."), button),
    divider(),
    row(t("Learn how you say « OK Ronron »"),
      variants.length > 0
        ? t("Ronron also wakes on what Parakeet writes when you say it:")
        : t("Say it 3 times: what Parakeet writes for your voice wakes Ronron from then on."),
      learnButton),
    variants.length > 0 ? learned : null,
    box);
}

function elevenKey(): HTMLElement {
  const box = h("div", { style: "display:flex;align-items:center;gap:8px" });
  const draw = async () => {
    const present = (await Bridge.secretPresent(ELEVEN)) ?? false;
    clear(box);
    if (present) {
      box.append(
        h("span", { class: "hub-note", style: "color:var(--hub-green)", text: `✓ ${t("Key saved in the {store}.", { store: KEY_STORE })}` }),
        button(t("Remove"), () => void Bridge.secretClear(ELEVEN).then(draw), "", true),
      );
      return;
    }
    const input = h("input", { class: "hub-input", type: "password", placeholder: t("ElevenLabs key"), autocomplete: "off", spellcheck: "false", style: "width:220px" }) as HTMLInputElement;
    const save = button(t("Save key"), () => {
      const value = input.value.trim();
      if (!value) return;
      void Bridge.secretSet(ELEVEN, value).then(draw, () => input.classList.add("err"));
    }, "primary", true);
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") save.click();
    });
    box.append(input, save);
  };
  void draw();
  return box;
}

export async function voicePage(page: HTMLElement, ctx: HubContext) {
  const status = await ctx.refresh();
  const prefs = status.prefs;
  page.append(ctx.hero("voice", "OK Ronron", t("Say « OK Ronron », then ask anything: add a pill, play music, the weather, an email to prepare.")));

  const ready = status.models.parakeet.state === "ready";
  page.append(label(t("Listening")));
  const main = card(
    row(t("Listen for « OK Ronron »"), t("The microphone stays open; only what follows « OK Ronron » goes further."),
      toggle(prefs.voice, (on) => void ctx.setPrefs((p) => (p.voice = on)).then(() => ctx.navigate("voice")))),
    divider(),
    row(t("Parakeet, on your computer"), t("Hears « OK Ronron » here, without the internet."), modelsControl(ctx, () => ctx.navigate("voice"))),
  );
  // Who answers: the chat's providers, with the model picked for each in AI agents → Chat (voice/llm.rs).
  const providers: [VoiceProvider, string][] = [
    ["anthropic", "Claude"], ["google", "Gemini"], ["openai", "OpenAI"], ["openrouter", "OpenRouter"],
    ["ollama", "Ollama"], ["lmstudio", "LM Studio"], ["custom", t("OpenAI-compatible server")],
  ];
  const provider = (prefs.voiceProvider || "anthropic") as VoiceProvider;
  const model = status.voiceModel ? t("Model: {model}", { model: status.voiceModel }) : "";
  main.append(
    divider(),
    row(t("Ronron answers with"),
      [model, t("Free: Gemini with a Google AI Studio key, or Ollama on this computer. Claude and Gemini can search the web.")].filter(Boolean).join(" · "),
      select(providers, provider, (v) => void ctx.setPrefs((p) => (p.voiceProvider = v)).then(() => ctx.navigate("voice")))),
  );
  if (prefs.voice && status.voiceProblem) {
    main.append(h("div", { class: "hub-warn", text: status.voiceProblem }));
  } else if (prefs.voice && !ready) {
    main.append(h("div", { class: "hub-warn", text: t("Download the voice model so Ronron can hear you.") }));
  }
  if (ready) main.append(divider(), voiceTest(ctx, prefs.wakeVariants ?? []));
  main.append(h("div", { class: "hub-note", style: "padding:6px 2px 0", text: t("No need to say « OK Ronron »: the microphone button in the island's header listens to one question.") }));
  page.append(main);

  page.append(label(t("Answers")));
  const outputs: [VoiceOutput, string][] = [["system", t("System voice")], ["elevenlabs", "ElevenLabs"], ["off", t("Text only")]];
  const answers = card(
    row(t("Ronron answers out loud"), null, segmented(outputs, prefs.voiceOutput, (v) => void ctx.setPrefs((p) => (p.voiceOutput = v)).then(() => ctx.navigate("voice")))),
  );
  if (prefs.voiceOutput === "elevenlabs") {
    answers.append(
      divider(),
      row(t("Voice"), null, segmented([["female", t("Female")], ["male", t("Male")]], prefs.voiceGender, (g) => void ctx.setPrefs((p) => (p.voiceGender = g)))),
      divider(),
      row(t("ElevenLabs key"), null, elevenKey()),
    );
  }
  page.append(answers);

  page.append(label(t("Privacy")));
  page.append(card(
    h("div", { class: "hub-note", text: t("Your voice never leaves this computer: Parakeet turns it into text here. Only the words after « OK Ronron » go to the model you picked, with your key (they stay here with a model on this computer), and to ElevenLabs if you picked it for the answers.") }),
    divider(),
    h("div", { class: "hub-note", text: t("An email is never sent by Ronron: your mail app opens with it written, and you send it.") }),
  ));
}
