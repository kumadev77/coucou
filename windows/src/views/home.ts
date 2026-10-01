// Home: what Mochi is for on Windows. Chat, music, videos and a quick web
// search, plus the screens behind the music / video / search buttons.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, type MediaItem } from "../core/bridge";
import { State } from "../core/state";
import type { ViewActions, ViewHost } from "./views";

function homeButton(icon: string, label: string, onClick: () => void): HTMLElement {
  return h(
    "button",
    { class: "home-btn", onclick: onClick },
    h("span", { class: "home-icon" }, svg(icon, 18)),
    h("span", { class: "home-label", text: label }),
  );
}

export function buildHome(actions: ViewActions): ViewHost {
  const greet = h("div", { class: "title", text: "¿Qué hacemos?" });
  const go = (fn: () => void) => () => {
    actions.blip();
    fn();
  };
  const grid = h(
    "div",
    { class: "home-grid" },
    homeButton(ICONS.bubble, "Charlar", go(() => actions.setView("prompt"))),
    homeButton(ICONS.music, "Poner música", go(() => actions.openMedia("music"))),
    homeButton(ICONS.video, "Reproducir videos", go(() => actions.openMedia("video"))),
    homeButton(ICONS.search, "Búsqueda web", go(() => actions.setView("websearch"))),
  );
  const body = h("div", { class: "stack home" }, greet, grid);
  return {
    el: h("div", { class: "view" }, h("div", { class: "card" }, body)),
    sync() {},
  };
}

// ── Music / videos ────────────────────────────────────────────────────────────

export function buildMedia(actions: ViewActions): ViewHost {
  const input = h("input", { type: "text", class: "chat-input", spellcheck: "false" }) as HTMLInputElement;
  const back = h(
    "button",
    { class: "icon-btn", title: "Back", onclick: () => actions.setView(State.defaultView()) },
    svg(ICONS.chevronLeft, 12, { stroke: 2 }),
  );
  const bar = h("div", { class: "chat-bar" }, back, input);
  const list = h("div", { class: "media-list" });
  const status = h("div", { class: "sub media-status" });
  const el = h(
    "div",
    { class: "view" },
    h("div", { class: "card chat-card" }, h("div", { class: "chat-body media-body" }, bar, status, list)),
  );

  let kind: "music" | "video" = "music";
  let seq = 0;
  let timer = 0;

  async function search() {
    const mine = ++seq;
    const items = (await Bridge.mediaSearch(kind, input.value.trim())) ?? [];
    if (mine !== seq) return; // a newer search is on its way
    render(items);
  }

  function render(items: MediaItem[]) {
    clear(list);
    if (items.length === 0) {
      status.textContent = input.value.trim()
        ? "Nada con ese nombre en tus carpetas."
        : "No hay archivos. Añade carpetas en Settings… → Media.";
      return;
    }
    status.textContent = "";
    for (const item of items) {
      const row = h(
        "button",
        { class: "media-row", title: item.path, onclick: () => void play(item) },
        svg(kind === "music" ? ICONS.music : ICONS.video, 12),
        h("span", { class: "media-name", text: item.name }),
        h("span", { class: "media-cat", text: item.category }),
      );
      list.append(row);
    }
  }

  async function play(item: MediaItem) {
    try {
      await Bridge.mediaOpen(item.path, kind);
      actions.collapse();
    } catch (err) {
      status.textContent = String(err).replace(/^Error:\s*/, "");
    }
  }

  input.addEventListener("input", () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void search(), 180);
  });
  input.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") {
      e.preventDefault();
      const first = list.querySelector<HTMLButtonElement>(".media-row");
      first?.click();
    }
    e.stopPropagation();
  });

  return {
    el,
    sync() {
      if (State.mediaKind !== kind) {
        kind = State.mediaKind;
        input.value = "";
      }
      input.placeholder = kind === "music" ? "¿Qué canción pongo?" : "¿Qué video pongo?";
    },
    focus() {
      input.focus();
      void search();
    },
  };
}

// ── Quick web search ──────────────────────────────────────────────────────────

export function buildWebSearch(actions: ViewActions): ViewHost {
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: "Buscar en la web…",
    spellcheck: "false",
  }) as HTMLInputElement;
  const back = h(
    "button",
    { class: "icon-btn", title: "Back", onclick: () => actions.setView(State.defaultView()) },
    svg(ICONS.chevronLeft, 12, { stroke: 2 }),
  );
  const yt = h("button", { class: "link-btn ws-yt", text: "YouTube" });
  const send = h("button", { class: "send-btn", title: "Search" }, svg(ICONS.search, 11));
  const bar = h("div", { class: "chat-bar" }, back, input, yt, send);
  const hint = h("div", { class: "sub", text: "Enter busca en la web · YouTube busca videos" });

  async function go(youtube: boolean) {
    const q = input.value.trim();
    if (!q) return;
    try {
      await (youtube ? Bridge.youtubeSearch(q) : Bridge.webSearch(q));
      input.value = "";
      actions.collapse();
    } catch (err) {
      hint.textContent = String(err).replace(/^Error:\s*/, "");
    }
  }
  send.addEventListener("click", () => void go(false));
  yt.addEventListener("click", () => void go(true));
  input.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") {
      e.preventDefault();
      void go(false);
    }
    e.stopPropagation();
  });

  return {
    el: h(
      "div",
      { class: "view" },
      h("div", { class: "card chat-card" }, h("div", { class: "chat-body ws-body" }, bar, hint)),
    ),
    sync() {},
    focus() {
      input.focus();
    },
  };
}
