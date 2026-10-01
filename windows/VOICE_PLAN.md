# Voice assistant plan (Windows)

Goal: say "Hey Mochi", the island pops up straight into the chat, you speak, Mochi
answers out loud and can play your media, play YouTube, and open web searches.
Everything runs on the PC. Not aimed at coding.

## Choices made

| Area | Choice |
|---|---|
| Wake word + speech to text | Vosk, offline |
| Speaking answers | Piper, offline |
| Media | Local files from folders you set per category, always opened in PotPlayer. YouTube in the browser. |
| Web search | Opens the search in the browser. The model does not read results. |
| Chat model | The local Ollama / llama.cpp support already on the `local-llm` branch |

## How a request flows

1. **Listening.** A background thread reads the microphone and feeds Vosk with a
   tiny grammar that only knows the wake phrase. Cheap on CPU.
2. **"Hey Mochi".** The island opens on the chat view, plays a short sound and
   shows a "listening" state.
3. **Daily chat.** First wake of the day starts a new chat. Every later wake that
   day continues the same chat. Chats are saved to
   `%APPDATA%\Coucou\chats\YYYY-MM-DD.json` so the day's chat survives restarts.
4. **Transcription.** Vosk switches to full recognition until about 1 s of
   silence, and the text appears as your message.
5. **Commands first, then the model.** The text is matched against simple
   command patterns (fast, works even with a small model):
   - "play / watch / put on <name> [from <category>]": search the media folders,
     open the best match in PotPlayer.
   - "play <name> on YouTube" / "YouTube <name>": open YouTube results in the browser.
   - "search <query>" / "look up <query>": open a web search in the browser.
   - "stop listening", "new chat", "close".
   Anything else goes to the local model. The model also gets the same actions as
   tools (`play_media`, `play_youtube`, `web_search`) for requests the patterns miss.
6. **Answer.** Mochi shows the reply and Piper speaks it. Saying "Hey Mochi"
   while it talks stops the speech.

## Settings additions

- **Voice:** on/off (off by default), microphone choice, wake phrase test button,
  Piper voice choice, speaking speed, "speak answers" on/off.
- **Media:** a list of categories, each with a name and one or more folders
  (for example Movies, Series, Anime, Music). PotPlayer path, auto-detected
  from the registry (`HKCU\Software\DAUM\PotPlayer64\ProgramFolder`, found on
  this PC at `C:\Program Files\DAUM\PotPlayer`).

## Downloads (one time)

- Vosk small model for your language: about 40-50 MB.
- Vosk runtime library (bundled in the installer, so it grows).
- Piper program plus one voice: about 20-60 MB per voice.

These are downloaded from Settings the first time you turn voice on, not
bundled, so the installer stays small. Sizes to be confirmed before building.

## Conflicts with the project rules

- **"0 % CPU when the island is hidden."** Wake-word listening must run while the
  island is hidden. Voice stays off by default and is a clear opt-in toggle; with
  it off the rule still holds.
- **"No third-party dependencies unless truly unavoidable."** Needed: a microphone
  capture crate (`cpal`) and Vosk bindings. Piper runs as a separate program, so
  it adds no Rust dependency.
- **"Never approve a Claude Code permission without an explicit click."** Voice
  will not approve or deny permissions.

## Risks to check first

1. **"Mochi" may not be in Vosk's vocabulary.** Vosk's grammar mode can only use
   words its model knows. First step is a test: if "mochi" is unknown, fall back
   to matching a near-sounding phrase, or pick a different wake phrase.
2. **Tool calls with small local models** can be unreliable. The command patterns
   in step 5 cover the main actions without the model.
3. **Media name matching.** Spoken titles won't match file names exactly. Plan:
   fuzzy matching on cleaned file names, and Mochi asks when two matches are close.

## Step 1 result: wake phrase check (2026-10-01)

Read the vocabulary straight from each model's `graph/Gr.fst` symbol table, using
HTTP range requests (about 1.5 MB fetched per model instead of the full zip).

| Model | Zip size | Words | "mochi" | Others |
|---|---|---|---|---|
| `vosk-model-small-en-us-0.15` | 41.2 MB | 152,217 | yes | hey, hi, okay: yes |
| `vosk-model-small-es-0.42` | 39.8 MB | 100,006 | yes | oye, hola, hey: yes |

Wake phrases: "hey mochi" (English), "oye mochi" or "hey mochi" (Spanish). Risk 1 is
cleared. Both models include `[unk]`, so the wake grammar can be
`["hey mochi", "oye mochi", "[unk]"]`: everything else is recognized as unknown
instead of being forced into the wake phrase.

## Step 2 prep: how Vosk gets into the app

- The `vosk` crate (0.3.1, last updated 2024-10) links `libvosk` at build time, so
  the app would fail to start without the DLL even with voice off. Not used.
- Instead: load `libvosk.dll` at runtime with `libloading` and call the handful of
  C functions needed (model new/free, recognizer new with grammar, accept waveform,
  result, final result). With voice off, nothing is loaded.
- Runtime source: `vosk-win64-0.3.45.zip` (14.9 MB) from the alphacep/vosk-api
  v0.3.45 GitHub release. The latest tag, v0.3.50, has no Windows build attached.
  Downloaded with the English model when voice is first turned on, into
  `%LOCALAPPDATA%\Coucou\voice\`.
- Microphone: `cpal` (0.18.2), resampled to 16 kHz mono for Vosk.
- New Rust dependencies: `cpal`, `libloading`, plus a zip extractor for the downloads.
- First download when turning voice on: about 15 MB runtime + 41 MB English model.
  Spanish adds 40 MB, only when clicked.

## Build order

1. Vosk wake-word test (answers risk 1).
2. Microphone + Vosk in Rust, "Hey Mochi" opens the island.
3. Daily chat persistence.
4. Transcription into the chat.
5. Command patterns + media folders + PotPlayer + YouTube + web search.
6. Piper speech.
7. Model tool calls for requests the patterns miss.
8. Settings UI for all of the above.

Each step is built and tested on GitHub (no local Rust needed) only when you say so.

## Languages

- **English** is the default. Its Vosk model and Piper voice are downloaded when
  voice is first turned on.
- **Spanish** is downloaded only when you click "Add Spanish" in Settings. Until
  then nothing Spanish is fetched. Once added, a language picker chooses which one
  Mochi listens and speaks in. Command patterns exist in both languages
  ("pon / reproduce X", "busca X", "X en YouTube").

## Open questions

- "First time of the day" resets at midnight for now. It can become a setting later
  (for example 4 am) if needed.
