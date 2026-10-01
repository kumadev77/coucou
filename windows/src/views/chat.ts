// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift.

import { h, svg, clear } from "./dom";
import { renderMarkdown } from "./markdown";
import { ICONS } from "./icons";
import { Bridge, onEvent, type ChatContext } from "../core/bridge";
import { Sound } from "../core/sound";
import { State, type ChatMessage } from "../core/state";
import type { ViewHost } from "./views";

let nextId = 1;

function bubble(message: ChatMessage): HTMLElement {
  if (message.role === "user") {
    return h(
      "div",
      { class: "chat-row user" },
      h("div", { class: "bubble", text: message.content }),
    );
  }
  return h("div", { class: "chat-row" }, h("div", { class: "reply" }, renderMarkdown(message.content)));
}

function typingDots(): HTMLElement {
  return h(
    "div",
    { class: "chat-row" },
    h("div", { class: "typing" }, h("i"), h("i"), h("i")),
  );
}

/** The coloured chip showing what the question is about (a dropped file). */
function contextChip(label: string): HTMLElement {
  const chip = h("div", { class: "chip" }, h("i", { class: "chip-dot" }), h("span", { text: label }));
  requestAnimationFrame(() => chip.classList.add("settled"));
  return chip;
}

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  const log = h("div", { class: "chat-log" });
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: "Ask me anything…",
    spellcheck: "false",
  }) as HTMLInputElement;
  const send = h("button", { class: "send-btn", title: "Send" }, svg(ICONS.arrowUp, 11));
  // Shown while Mochi reads an answer aloud.
  const hush = h("button", { class: "hush-btn", text: "Callar", onclick: () => void Bridge.voiceStop() });
  const bar = h("div", { class: "chat-bar" }, input, hush, send);

  const el = h(
    "div",
    { class: "view" },
    h("div", { class: "card wash chat-card" }, h("div", { class: "chat-body" }, chipRow, log, bar)),
  );
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  let renderedCount = -1;

  // Local models stream their answer: the reply grows in place instead of
  // landing as one block. `streaming` is the message being written.
  let streaming: ChatMessage | null = null;
  void onEvent<string>("chat-stream", (text) => {
    if (!sending) return;
    if (!streaming) {
      streaming = { id: nextId++, role: "assistant", content: "" };
      State.chatHistory.push(streaming);
      State.stateOverride = null; // the text itself replaces the typing dots
      State.notify();
      onHeightChange();
    }
    streaming.content = text;
    const replies = log.querySelectorAll(".reply");
    const last = replies[replies.length - 1];
    if (last) last.replaceChildren(renderMarkdown(text));
    log.scrollTop = log.scrollHeight;
  });

  /** Command mode: the router turns the words into an action and confirms briefly. */
  async function command(text: string, alt = "") {
    sending = true;
    input.value = "";
    State.chatDay = new Date().toDateString();
    State.chatHistory.push({ id: nextId++, role: "user", content: text });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();
    try {
      const reply = await Bridge.assistantCommand(text, alt);
      State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      if (reply.ui === "chat" || reply.ui === "commands") {
        State.assistantMode = reply.ui;
        void Bridge.voiceMode(reply.ui === "chat");
      }
      if (State.settings.voiceSpeak) void Bridge.voiceSpeak(reply.text);
      Sound.play(reply.action ? "finish" : "pop");
      if (reply.ui === "music" || reply.ui === "video") {
        window.dispatchEvent(new CustomEvent("mochi-open-media", { detail: reply.ui }));
      }
    } catch (err) {
      State.chatHistory.push({ id: nextId++, role: "assistant", content: String(err).replace(/^Error:\s*/, "") });
    } finally {
      State.stateOverride = null;
      sending = false;
      State.notify();
      onHeightChange();
    }
  }

  async function submit(spoken = false) {
    const query = input.value.trim();
    if (!query || sending) return;
    input.value = "";
    sending = true;
    State.chatDay = new Date().toDateString();
    Sound.play("send");

    State.chatHistory.push({ id: nextId++, role: "user", content: query });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();

    const file = State.droppedFile;
    const context: ChatContext | null =
      State.chatHistory.length === 1 && file ? { kind: "file", name: file.name, path: file.path } : null;

    try {
      const reply = await Bridge.chatSend(query, context);
      if (streaming) streaming.content = reply.text;
      else State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      // Spoken question, spoken answer.
      if (spoken && State.settings.voiceSpeak) void Bridge.voiceSpeak(reply.text);
      State.stateOverride = null;
      Sound.play("finish");
    } catch (err) {
      // A half-written answer that then failed is not kept.
      if (streaming) State.chatHistory.splice(State.chatHistory.indexOf(streaming), 1);
      State.stateOverride = null;
      State.noteMessage = String(err).replace(/^Error:\s*/, "");
      State.view = "note";
      Sound.play("error");
    } finally {
      streaming = null;
      sending = false;
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  send.addEventListener("click", () => void submit());

  // Voice: what's being said shows in the field, the final words are sent.
  void onEvent<string>("voice-partial", (text) => {
    if (!sending) input.value = text;
  });
  // `alt`: the same words heard by the other language's model (Spanglish).
  void onEvent<{ text: string; alt: string }>("voice-text", ({ text, alt }) => {
    State.voiceListening = false;
    if (sending) return;
    // Layers: commands by default; conversation only after "charlemos".
    const backToCommands = /modo comando|deja de (charlar|hablar)|command(s)? mode|stop chatting/i.test(`${text} ${alt}`);
    if (State.assistantMode === "commands" || backToCommands) void command(text, alt);
    else {
      input.value = text;
      void submit(true);
    }
  });
  void onEvent<string>("voice-timeout", () => {
    State.voiceListening = false;
    State.notify();
  });
  input.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") {
      e.preventDefault();
      void submit();
    }
    e.stopPropagation(); // Escape closes the island, not the chat
  });

  return {
    el,
    sync() {
      const file = State.droppedFile;
      const wantChip = file?.name ?? "";
      if (chipRow.dataset.label !== wantChip) {
        chipRow.dataset.label = wantChip;
        clear(chipRow);
        if (wantChip) chipRow.append(contextChip(wantChip));
      }

      const thinking = State.stateOverride === "thinking";
      const count = State.chatHistory.length + (thinking ? 0.5 : 0);
      if (count !== renderedCount) {
        renderedCount = count;
        clear(log);
        for (const m of State.chatHistory) log.append(bubble(m));
        if (thinking) log.append(typingDots());
        log.scrollTop = log.scrollHeight;
      }

      input.placeholder = State.voiceListening
        ? State.assistantMode === "chat"
          ? "Te escucho (charlando)…"
          : "Te escucho (comandos)…"
        : State.chatHistory.length === 0
          ? "Ask me anything…"
          : "Continue…";
      input.disabled = sending;
      hush.style.display = State.voiceSpeaking ? "" : "none";
    },
    focus() {
      input.focus();
      input.select();
    },
  };
}
