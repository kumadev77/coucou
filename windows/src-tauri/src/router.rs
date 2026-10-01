// Command mode: what was said after "Hey Mochi" becomes an action.
//
// Exact phrases ("abre lol", "pon <canción>") are matched by the launcher's own
// patterns first: instant and free. Anything else goes to the local model with a
// strict JSON reply format, which copes with speech-recognition mistakes ("o
// música" for "pon música") and loose wording. Conversation only happens after
// "charlemos" / "hablemos", in chat mode.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::claude::Backend;
use crate::launcher::{self, Action, MediaKind};
use crate::settings::Settings;

/// What the island should do after a command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandReply {
    /// Short confirmation, shown and spoken.
    pub text: String,
    /// For Mochi's reaction (dance, glasses, controller).
    pub action: Option<Action>,
    /// "chat" / "commands": switch mode. "music" / "video": open that picker.
    pub ui: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Intent {
    #[serde(default)]
    action: String,
    #[serde(default)]
    query: String,
}

/// `alt` is the same speech heard by the other language's model ("" if none).
pub async fn run(settings: &Settings, text: &str, alt: &str) -> CommandReply {
    let es = settings.voice_lang == "es" || looks_spanish(text);
    let say = |en: &str, sp: &str| if es { sp.to_string() } else { en.to_string() };

    if let Some(mode) = mode_switch(text).or_else(|| mode_switch(alt)) {
        return CommandReply {
            text: if mode == "chat" {
                say("Sure, let's talk. Say \"commands mode\" when you're done.", "¡Claro, charlemos! Di \"modo comandos\" cuando acabemos.")
            } else {
                say("Commands mode. What should I do?", "Modo comandos. ¿Qué hago?")
            },
            action: None,
            ui: Some(mode.into()),
        };
    }

    // 1. Exact patterns, on either transcript. A pattern that matched but found
    // nothing ("pon <misheard title>") still lets the model try both transcripts.
    let mut pattern_error = None;
    for t in [text, alt].into_iter().filter(|t| !t.is_empty()) {
        match launcher::handle_command(settings, t) {
            Some(Ok(done)) => return CommandReply { text: done.reply, action: Some(done.action), ui: None },
            Some(Err(err)) => pattern_error = pattern_error.or(Some(err)),
            None => {}
        }
    }

    // 2. The model.
    let intent = match settings.chat_backend() {
        Backend::Local { base_url, model, .. } => classify(&base_url, &model, settings, text, alt).await,
        Backend::Anthropic { .. } => Err("no local model".into()),
    };
    let Ok(intent) = intent else {
        if let Some(err) = pattern_error {
            return CommandReply { text: err, action: None, ui: None };
        }
        return CommandReply {
            text: say(
                "I didn't get that as a command. Say \"let's talk\" to chat.",
                "No lo entendí como un comando. Di \"charlemos\" si quieres hablar.",
            ),
            action: None,
            ui: None,
        };
    };
    crate::log::line(format!("router: {:?} -> {} {:?}", text, intent.action, intent.query));
    execute(settings, &intent, es).await
}

async fn execute(settings: &Settings, intent: &Intent, es: bool) -> CommandReply {
    let say = |en: String, sp: String| if es { sp } else { en };
    let q = intent.query.trim();
    let ok = |text: String, action: Action| CommandReply { text, action: Some(action), ui: None };
    let fail = |text: String| CommandReply { text, action: None, ui: None };

    match intent.action.as_str() {
        "music" | "video" => {
            let kind = if intent.action == "music" { MediaKind::Music } else { MediaKind::Video };
            if q.is_empty() {
                return CommandReply {
                    text: say("What should I play?".into(), "¿Qué pongo?".into()),
                    action: None,
                    ui: Some(intent.action.clone()),
                };
            }
            let found = launcher::search_media(settings, kind, q);
            match found.first() {
                Some(item) => match launcher::open_media(settings, &item.path) {
                    Ok(()) => ok(
                        say(format!("Playing {}.", item.name), format!("Reproduciendo {}.", item.name)),
                        if kind == MediaKind::Music { Action::Music } else { Action::Video },
                    ),
                    Err(e) => fail(e),
                },
                None => CommandReply {
                    text: say(format!("I couldn't find \"{q}\". Pick one:"), format!("No encontré \"{q}\". Elige uno:")),
                    action: None,
                    ui: Some(intent.action.clone()),
                },
            }
        }
        "game" => match launcher::find_game(settings, q) {
            Some(game) => match launcher::launch_game(game) {
                Ok(()) => ok(say(format!("Starting {}.", game.name), format!("Abriendo {}.", game.name)), Action::Game),
                Err(e) => fail(e),
            },
            None => fail(say(
                format!("\"{q}\" isn't in your games. Add it in Settings → Games."),
                format!("\"{q}\" no está en tus juegos. Añádelo en Ajustes → Juegos."),
            )),
        },
        "search" if !q.is_empty() => match launcher::web_search(q) {
            Ok(()) => ok(say(format!("Searching for \"{q}\"."), format!("Buscando \"{q}\".")), Action::Search),
            Err(e) => fail(e),
        },
        "youtube" if !q.is_empty() => match launcher::youtube_search(q) {
            Ok(()) => ok(say(format!("\"{q}\" on YouTube."), format!("\"{q}\" en YouTube.")), Action::Youtube),
            Err(e) => fail(e),
        },
        "chat" => CommandReply {
            text: say("Sure, let's talk.".into(), "¡Claro, charlemos!".into()),
            action: None,
            ui: Some("chat".into()),
        },
        _ => fail(say(
            "That's not a command I know. Say \"let's talk\" to chat.".into(),
            "Eso no es un comando que conozca. Di \"charlemos\" si quieres hablar.".into(),
        )),
    }
}

/// "charlemos" → chat mode, "modo comandos" → commands mode.
pub fn mode_switch(text: &str) -> Option<&'static str> {
    let line = launcher::normalize_words(text).join(" ");
    const TO_CHAT: &[&str] = &[
        "charlemos", "hablemos", "vamos a hablar", "vamos a charlar", "quiero hablar", "quiero charlar",
        "lets talk", "let s talk", "let us talk", "lets chat", "let s chat", "chat mode", "modo charla",
    ];
    const TO_COMMANDS: &[&str] = &[
        "modo comandos", "modo comando", "deja de charlar", "deja de hablar", "terminamos de hablar",
        "commands mode", "command mode", "stop chatting",
    ];
    if TO_COMMANDS.iter().any(|p| line.contains(p)) {
        Some("commands")
    } else if TO_CHAT.iter().any(|p| line.contains(p)) {
        Some("chat")
    } else {
        None
    }
}

fn looks_spanish(text: &str) -> bool {
    let words = launcher::normalize_words(text);
    ["pon", "ponme", "abre", "busca", "quiero", "musica", "juego", "el", "la", "de", "que", "una", "un"]
        .iter()
        .any(|w| words.iter().any(|x| x == w))
}

/// Asks the local model to turn the text into one action. JSON only, short, no thinking.
async fn classify(base_url: &str, model: &str, settings: &Settings, text: &str, alt: &str) -> Result<Intent, String> {
    let games: Vec<String> = settings
        .games
        .iter()
        .map(|g| {
            if g.keywords.is_empty() {
                g.name.clone()
            } else {
                format!("{} ({})", g.name, g.keywords.join(", "))
            }
        })
        .collect();
    let system = format!(
        "You turn a spoken command for a desktop assistant into JSON. The text comes from speech \
recognition and often has mistakes: guess what was meant (\"o musica\" = \"pon musica\", \"mortal combat ex\" = \"Mortal Kombat X\"). \
Reply with JSON only: {{\"action\": \"music\" | \"video\" | \"game\" | \"search\" | \"youtube\" | \"chat\" | \"none\", \"query\": \"...\"}}.\n\
- music: play a song or artist from the user's files. query = song/artist, or \"\" if none was named.\n\
- video: play a film, series or video file. query = title, or \"\".\n\
- game: open a game. query = the game's name exactly as in this list: {}.\n\
- search: search the web. query = what to search.\n\
- youtube: search YouTube. query = what to search.\n\
- chat: the user wants to talk or have a conversation.\n\
- none: anything else.\n\
The user often mixes Spanish and English (Spanglish), e.g. \"pon Broken de Falling in Reverse\". You may get two \
transcripts of the same speech, one from a Spanish recognizer and one from an English one: each gets its own \
language's words right and mangles the other's. Combine them: take Spanish words from the Spanish one and English \
titles and names from the English one.\n\
Be strict. Only pick an action when the text clearly asks for it (a verb like play, open, search, pon, abre, busca, \
or an obvious request). Speech recognition also picks up background talk, music and the assistant's own voice: \
if the text is a random sentence, a fragment or doesn't make sense as a request, answer none. Never pick game \
unless one of the listed games is clearly named.",
        if games.is_empty() { "(no games yet)".to_string() } else { games.join("; ") }
    );
    let body = json!({
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": if alt.is_empty() {
                text.to_string()
            } else {
                format!("Recognizer 1: {text}\nRecognizer 2: {alt}")
            } },
        ],
        "max_tokens": 80,
        "temperature": 0,
        "stream": false,
        "response_format": { "type": "json_object" },
        "reasoning_effort": "none",
        "chat_template_kwargs": { "enable_thinking": false },
        "model": model,
    });
    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client.post(&url).json(&body).send().await.map_err(|e| e.to_string())?;
    let v: Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or("no content")?;
    // Some models wrap the JSON in prose or code fences: take the object itself.
    let start = content.find('{').ok_or("no json")?;
    let end = content.rfind('}').ok_or("no json")?;
    serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::mode_switch;

    #[test]
    fn switches_modes() {
        assert_eq!(mode_switch("Charlemos un rato"), Some("chat"));
        assert_eq!(mode_switch("let's talk"), Some("chat"));
        assert_eq!(mode_switch("vuelve al modo comandos"), Some("commands"));
        assert_eq!(mode_switch("pon música"), None);
    }
}
