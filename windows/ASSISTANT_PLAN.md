# Mochi as a home assistant (Windows)

Requested 2026-10-01. Mochi is for music, videos, games, quick web searches and
chat, not coding. Voice comes later (see VOICE_PLAN.md) and reuses everything here.

## Phase 1: home, media, games, search (text first)

- **New home view** replacing the Claude Code overview: four buttons,
  Charlar, Poner música, Reproducir videos, Búsqueda web rápida.
- **Music / videos:** a search field plus a list of matches from the folders set
  in Settings. Choosing one opens it in PotPlayer (auto-detected from
  `HKCU\Software\DAUM\PotPlayer64\ProgramFolder`).
- **Quick web search:** a field; Enter opens the search in the browser.
- **Games:** a manual list in Settings. Each game has a name, keywords
  ("lol", "league") and a path (.exe, shortcut .lnk/.url, or a steam:// link).
- **Chat commands, before the model:** "abre / juega / open / play <game>",
  "pon / reproduce / play <song or video>", "busca / search <query>",
  "<x> en YouTube / on YouTube". Anything else goes to the model.

## Phase 2: Mochi customization panel

- Colours (body, eyes, accent), name, personality (added to the model's
  instructions), size, sounds on/off and which ones.
- Renaming Mochi also changes the wake word once voice exists. Vosk only knows
  words in its vocabulary, so a new name is checked against it.

## Phase 3: Mochi reacts to what it's doing

Drawn in code on the existing canvas, no image assets:
- **Music playing:** Mochi bobs to a beat with music-note particles.
- **Web search:** round glasses over the eyes.
- **Game launched:** holds a small controller in its hands.

These run for a while after the action (Mochi can't see when the song or game
ends without more work).

## Not shown any more

The Claude Code overview (sessions, pills, approvals) leaves the home view.
The hooks still work: approvals and questions still pop up when Claude Code asks.
