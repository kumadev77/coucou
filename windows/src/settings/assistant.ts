// Settings sections for Mochi as a home assistant: media folders, PotPlayer
// and games. Each section edits `settings` in place and calls `save()`.

import { Bridge, onEvent, type VoiceProgress } from "../core/bridge";
import type { Game, MediaCategory, Settings } from "../core/state";
import { h, clear } from "../views/dom";

type Save = () => void;

function textInput(value: string, placeholder: string, onChange: (v: string) => void): HTMLInputElement {
  const el = h("input", {
    type: "text",
    placeholder,
    style: "flex:1 1 auto;min-width:0",
    autocomplete: "off",
    spellcheck: "false",
  }) as HTMLInputElement;
  el.value = value;
  el.addEventListener("change", () => onChange(el.value.trim()));
  return el;
}

// ── Media ─────────────────────────────────────────────────────────────────────

export function mediaSection(settings: Settings, save: Save): HTMLElement {
  const list = h("div", { class: "stack-list" });
  const potHint = h("span", { class: "hint" });

  function draw() {
    clear(list);
    settings.mediaCategories.forEach((cat, i) => list.append(categoryCard(cat, i)));
  }

  function categoryCard(initial: MediaCategory, index: number): HTMLElement {
    // Always the live object: settings arrays are replaced when a save echoes back.
    const cat = () => settings.mediaCategories[index] ?? initial;
    const kind = h("select", {}) as HTMLSelectElement;
    kind.append(h("option", { value: "music", text: "Música" }), h("option", { value: "video", text: "Video" }));
    kind.value = initial.kind;
    kind.addEventListener("change", () => {
      cat().kind = kind.value as MediaCategory["kind"];
      save();
    });
    const name = textInput(initial.name, "Nombre (Películas, Series, Anime…)", (v) => {
      cat().name = v || "Media";
      save();
    });
    const remove = h("button", { class: "danger", text: "Quitar" });
    remove.addEventListener("click", () => {
      settings.mediaCategories.splice(index, 1);
      save();
      draw();
    });

    const folders = h("div", { class: "stack-list" });
    const drawFolders = () => {
      clear(folders);
      cat().folders.forEach((folder, fi) => {
        const input = textInput(folder, "C:\\Users\\…\\Videos", (v) => {
          cat().folders[fi] = v;
          save();
        });
        const x = h("button", { text: "×", title: "Quitar carpeta" });
        x.addEventListener("click", () => {
          cat().folders.splice(fi, 1);
          save();
          drawFolders();
        });
        folders.append(h("div", { class: "row" }, h("label", { text: fi === 0 ? "Carpetas" : "" }), input, x));
      });
      const add = h("button", { text: "+ Carpeta" });
      add.addEventListener("click", () => {
        cat().folders.push("");
        save();
        drawFolders();
        (folders.querySelectorAll("input")[cat().folders.length - 1] as HTMLInputElement | undefined)?.focus();
      });
      folders.append(h("div", { class: "row" }, h("label", { text: cat().folders.length ? "" : "Carpetas" }), add));
    };
    drawFolders();

    return h(
      "div",
      { class: "sub-card" },
      h("div", { class: "row" }, h("label", { text: "Categoría" }), name, kind, remove),
      folders,
    );
  }

  const add = h("button", { class: "primary", text: "+ Categoría" });
  add.addEventListener("click", () => {
    settings.mediaCategories.push({ name: "Nueva", kind: "video", folders: [""] });
    save();
    draw();
  });

  const pot = textInput(settings.potplayerPath, "Detectar automáticamente", (v) => {
    settings.potplayerPath = v;
    save();
  });
  void Bridge.potplayerDetect().then((found) => {
    potHint.textContent = found
      ? `Detectado: ${found}. Déjalo vacío para usar ese.`
      : "No encontré PotPlayer. Sin él se usa el reproductor por defecto de Windows.";
  });

  draw();
  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Media" })),
    h("span", {
      class: "hint",
      text: "Carpetas donde Mochi busca cuando dices \"pon…\" o usas Poner música / Reproducir videos. Pega la ruta de cada carpeta.",
    }),
    list,
    h("div", { class: "row" }, add),
    h("div", { class: "row" }, h("label", { text: "PotPlayer" }), pot),
    potHint,
  );
}

// ── Games ─────────────────────────────────────────────────────────────────────

export function gamesSection(settings: Settings, save: Save): HTMLElement {
  const list = h("div", { class: "stack-list" });

  function draw() {
    clear(list);
    if (settings.games.length === 0) {
      list.append(h("span", { class: "hint", text: "Todavía no hay juegos." }));
    }
    settings.games.forEach((game, i) => list.append(gameCard(game, i)));
  }

  function gameCard(initial: Game, index: number): HTMLElement {
    // Always the live object: settings arrays are replaced when a save echoes back.
    const game = () => settings.games[index] ?? initial;
    const name = textInput(initial.name, "Nombre", (v) => {
      game().name = v;
      save();
    });
    const keywords = textInput(initial.keywords.join(", "), "lol, league (separadas por comas)", (v) => {
      game().keywords = v.split(",").map((k) => k.trim()).filter(Boolean);
      save();
    });
    const path = textInput(initial.path, "C:\\…\\juego.exe, acceso directo o steam://rungameid/…", (v) => {
      game().path = v;
      save();
    });
    const browse = h("button", { text: "Elegir…" });
    browse.addEventListener("click", async () => {
      const picked = await Bridge.pickFile();
      if (picked) {
        path.value = picked;
        game().path = picked;
        if (!game().name) {
          game().name = picked.split(/[\\/]/).pop()!.replace(/\.(exe|lnk|url)$/i, "");
          name.value = game().name;
        }
        save();
      }
    });
    const test = h("button", { text: "Probar" });
    const feedback = h("span", { class: "hint" });
    test.addEventListener("click", async () => {
      feedback.textContent = "";
      try {
        await Bridge.gameLaunch(game().name);
      } catch (err) {
        feedback.textContent = String(err).replace(/^Error:\s*/, "");
      }
    });
    const remove = h("button", { class: "danger", text: "Quitar" });
    remove.addEventListener("click", () => {
      settings.games.splice(index, 1);
      save();
      draw();
    });

    return h(
      "div",
      { class: "sub-card" },
      h("div", { class: "row" }, h("label", { text: "Juego" }), name, remove),
      h("div", { class: "row" }, h("label", { text: "Palabras clave" }), keywords),
      h("div", { class: "row" }, h("label", { text: "Ubicación" }), path, browse, test),
      feedback,
    );
  }

  const add = h("button", { class: "primary", text: "+ Juego" });
  add.addEventListener("click", () => {
    settings.games.push({ name: "", keywords: [], path: "" });
    save();
    draw();
  });

  draw();
  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Juegos" })),
    h("span", {
      class: "hint",
      text: "Di o escribe \"abre <juego>\" o \"juega a <palabra clave>\". Para Steam, usa el acceso directo del escritorio o steam://rungameid/<id>.",
    }),
    list,
    h("div", { class: "row" }, add),
  );
}

// ── Mochi customization ───────────────────────────────────────────────────────

/** The sounds, grouped so the list reads as something other than 28 file names. */
const SOUND_GROUPS: [string, string[]][] = [
  ["Abrir y cerrar", ["peek", "open", "close", "hover", "blip"]],
  ["Estados", ["work", "think", "search", "finish", "error", "rate", "sleep"]],
  ["Avisos", ["approval", "question", "tick"]],
  ["Chat y archivos", ["send", "attach", "gulp", "approve", "pop"]],
  ["Emociones", ["greet", "love", "proud", "wink", "yawn", "annoyed", "dizzy", "slap"]],
];

type ColourKey = "mochiBody" | "mochiEyes" | "mochiAccent";

export function mochiSection(settings: Settings, save: Save): HTMLElement {
  const name = textInput(settings.mochiName, "Mochi", (v) => {
    settings.mochiName = v || "Mochi";
    save();
  });

  const colour = (key: ColourKey, fallback: string) => {
    const input = h("input", { type: "color" }) as HTMLInputElement;
    input.value = settings[key] || fallback;
    input.addEventListener("input", () => {
      settings[key] = input.value;
      save();
    });
    const reset = h("button", { text: "Original" });
    reset.addEventListener("click", () => {
      settings[key] = "";
      input.value = fallback;
      save();
    });
    return [input, reset];
  };

  const personality = h("textarea", {
    rows: 3,
    placeholder: "Ej.: divertido y un poco sarcástico, me llama \"jefe\", responde corto.",
    style: "flex:1 1 auto;min-width:0;resize:vertical",
  }) as HTMLTextAreaElement;
  personality.value = settings.mochiPersonality;
  personality.addEventListener("change", () => {
    settings.mochiPersonality = personality.value.trim();
    save();
  });

  const size = h("input", { type: "range", min: "0.8", max: "1.3", step: "0.05" }) as HTMLInputElement;
  size.value = String(settings.mochiScale || 1);
  const sizeLabel = h("span", { class: "hint" });
  const showSize = () => (sizeLabel.textContent = `${Math.round(Number(size.value) * 100)} %`);
  showSize();
  size.addEventListener("input", () => {
    settings.mochiScale = Number(size.value);
    showSize();
    save();
  });

  const reactions = h("input", { type: "checkbox" }) as HTMLInputElement;
  reactions.checked = settings.mochiReactions;
  reactions.addEventListener("change", () => {
    settings.mochiReactions = reactions.checked;
    save();
  });

  const sounds = h("div", { class: "stack-list" });
  for (const [group, names] of SOUND_GROUPS) {
    const chips = h("div", { class: "row chips" });
    for (const n of names) {
      const box = h("input", { type: "checkbox" }) as HTMLInputElement;
      box.checked = !settings.mutedSounds.includes(n);
      box.addEventListener("change", () => {
        const muted = new Set(settings.mutedSounds);
        if (box.checked) muted.delete(n);
        else muted.add(n);
        settings.mutedSounds = [...muted];
        save();
      });
      chips.append(h("label", { class: "chip-check" }, box, h("span", { text: n })));
    }
    sounds.append(h("div", { class: "row" }, h("label", { text: group }), chips));
  }

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Personalizar a Mochi" })),
    h("div", { class: "row" }, h("label", { text: "Nombre" }), name),
    h("span", {
      class: "hint",
      text: "Así se presenta en el chat. Cuando llegue la voz, también será la palabra para despertarlo.",
    }),
    h("div", { class: "row" }, h("label", { text: "Color del cuerpo" }), ...colour("mochiBody", "#c4c5ca")),
    h("div", { class: "row" }, h("label", { text: "Color de los ojos" }), ...colour("mochiEyes", "#1a1412")),
    h("div", { class: "row" }, h("label", { text: "Color de acento" }), ...colour("mochiAccent", "#6366f1")),
    h("div", { class: "row" }, h("label", { text: "Personalidad" }), personality),
    h("div", { class: "row" }, h("label", { text: "Tamaño" }), size, sizeLabel),
    h(
      "div",
      { class: "row" },
      h("label", { text: "Reacciones" }),
      h("label", { class: "chip-check" }, reactions, h("span", { text: "Gafas al buscar, baile con música, mando al jugar" })),
    ),
    h("div", { class: "hint", text: "Sonidos (desmarca los que no quieras oír):" }),
    sounds,
  );
}

// ── Voice ─────────────────────────────────────────────────────────────────────

export function voiceSection(settings: Settings, save: Save): HTMLElement {
  const statusLine = h("span", { class: "hint" });
  const progress = h("div", { class: "hint" });
  const buttons = h("div", { class: "row" });
  const feedback = h("div", {});
  const whisperLine = h("span", { class: "hint" });

  const enabled = h("input", { type: "checkbox" }) as HTMLInputElement;
  enabled.checked = settings.voiceEnabled;
  enabled.addEventListener("change", () => {
    settings.voiceEnabled = enabled.checked;
    save();
    void refresh();
  });

  const langSel = h("select", {}) as HTMLSelectElement;
  langSel.addEventListener("change", () => {
    settings.voiceLang = langSel.value as Settings["voiceLang"];
    save();
  });

  const speak = h("input", { type: "checkbox" }) as HTMLInputElement;
  speak.checked = settings.voiceSpeak;
  speak.addEventListener("change", () => {
    settings.voiceSpeak = speak.checked;
    save();
  });

  const speed = h("input", { type: "range", min: "0.6", max: "1.6", step: "0.1" }) as HTMLInputElement;
  speed.value = String(settings.voiceSpeed || 1);
  speed.addEventListener("change", () => {
    settings.voiceSpeed = Number(speed.value);
    save();
  });

  const test = h("button", { text: "Probar voz" });
  test.addEventListener("click", () => {
    void Bridge.voiceSpeak(
      settings.voiceLang === "es" ? `Hola, soy ${settings.mochiName}. ¿Qué hacemos?` : `Hi, I'm ${settings.mochiName}. What shall we do?`,
    );
  });

  let busy = false;
  async function install(part: "en" | "es" | "voice-en" | "voice-es" | "whisper") {
    if (busy) return;
    busy = true;
    clear(feedback);
    try {
      await Bridge.voiceInstall(part);
      feedback.append(h("div", { class: "notice ok", text: "Listo." }));
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: String(err).replace(/^Error:\s*/, "") }));
    } finally {
      busy = false;
      progress.textContent = "";
      await refresh();
    }
  }

  const dl = (label: string, part: "en" | "es" | "voice-en" | "voice-es" | "whisper") => {
    const b = h("button", { class: "primary", text: label });
    b.addEventListener("click", () => void install(part));
    return b;
  };

  async function refresh() {
    const st = await Bridge.voiceStatus();
    if (!st) return;
    clear(buttons);
    if (!st.english) buttons.append(dl("Descargar reconocimiento en inglés (~56 MB)", "en"));
    if (!st.englishVoice) buttons.append(dl("Descargar voz de Mochi en inglés (~85 MB)", "voice-en"));
    if (!st.spanish) buttons.append(dl("Añadir español (~40 MB)", "es"));
    else if (!st.spanishVoice) buttons.append(dl("Voz de Mochi en español (~63 MB)", "voice-es"));
    if ((st.english || st.spanish) && !st.whisper) {
      buttons.append(dl("Mejor reconocimiento con Whisper (~200 MB)", "whisper"));
    }

    clear(langSel);
    if (st.english) langSel.append(h("option", { value: "en", text: "English" }));
    if (st.spanish) langSel.append(h("option", { value: "es", text: "Español" }));
    langSel.value = settings.voiceLang;
    langSel.disabled = !(st.english || st.spanish);

    const name = settings.mochiName || "Mochi";
    statusLine.textContent = !(st.english || st.spanish)
      ? "Primero descarga el reconocimiento de voz. Todo se queda en tu PC."
      : st.listening
        ? `Escuchando. Di "Hey ${name}" (o "Oye ${name}" en español), espera el sonido y habla.`
        : settings.voiceEnabled
          ? "Arrancando…"
          : "Activa la voz para que Mochi escuche.";
    test.style.display = st.englishVoice || st.spanishVoice ? "" : "none";
    whisperLine.textContent = st.whisper
      ? "Whisper activo: entiende mucho mejor lo que dices después de \"Hey Mochi\"."
      : "";
  }

  void onEvent<VoiceProgress>("voice-progress", (p) => {
    const mb = (n: number) => (n / 1048576).toFixed(1);
    progress.textContent = p.total
      ? `${p.label}: ${mb(p.done)} / ${mb(p.total)} MB`
      : p.label;
  });
  void refresh();

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Voz" })),
    statusLine,
    whisperLine,
    h(
      "div",
      { class: "row" },
      h("label", { text: "Escuchar" }),
      h("label", { class: "chip-check" }, enabled, h("span", { text: "Mantiene el micrófono abierto mientras está activo" })),
    ),
    h("div", { class: "row" }, h("label", { text: "Idioma" }), langSel),
    h(
      "div",
      { class: "row" },
      h("label", { text: "Responder en voz alta" }),
      h("label", { class: "chip-check" }, speak, h("span", { text: "Cuando le hablas" })),
      test,
    ),
    h("div", { class: "row" }, h("label", { text: "Velocidad" }), speed),
    buttons,
    progress,
    feedback,
  );
}
