// Launcher: media from the user's folders (opened in PotPlayer), games from the
// list in Settings, and web / YouTube searches in the default browser.
//
// Nothing goes through `cmd /C`: paths are handed to the program directly, so
// `&`, `^` or `%` in a file name stay plain characters.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::settings::{Game, Settings};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_RESULTS: usize = 40;
/// Folder walks stop here so a huge drive can't freeze the search.
const MAX_FILES_SCANNED: usize = 50_000;
const MAX_DEPTH: usize = 6;

const MUSIC_EXT: &[&str] = &["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma"];
const VIDEO_EXT: &[&str] = &["mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v", "ts", "flv"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Music,
    Video,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub name: String,
    pub path: String,
    pub category: String,
    pub score: u32,
}

// ── Media ─────────────────────────────────────────────────────────────────────

/// Files in the categories of `kind` whose name matches `query`, best first.
/// An empty query lists files in folder order.
pub fn search_media(settings: &Settings, kind: MediaKind, query: &str) -> Vec<MediaItem> {
    let words = normalize_words(query);
    let exts = match kind {
        MediaKind::Music => MUSIC_EXT,
        MediaKind::Video => VIDEO_EXT,
    };
    let mut found = Vec::new();
    let mut scanned = 0usize;
    for cat in settings.media_categories.iter().filter(|c| c.kind == kind) {
        for folder in &cat.folders {
            walk(Path::new(folder), 0, &mut scanned, &mut |path| {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if !exts.contains(&ext.as_str()) {
                    return;
                }
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                // Folder names count too: "Breaking Bad/S01E01.mkv" matches "breaking bad".
                let haystack = format!("{} {}", path.parent().map(|p| p.to_string_lossy()).unwrap_or_default(), name);
                let score = match_score(&words, &haystack, &name);
                if score > 0 || words.is_empty() {
                    found.push(MediaItem {
                        name,
                        path: path.to_string_lossy().to_string(),
                        category: cat.name.clone(),
                        score,
                    });
                }
            });
        }
    }
    found.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    found.truncate(MAX_RESULTS);
    found
}

fn walk(dir: &Path, depth: usize, scanned: &mut usize, visit: &mut impl FnMut(&Path)) {
    if depth > MAX_DEPTH || *scanned >= MAX_FILES_SCANNED {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *scanned >= MAX_FILES_SCANNED {
            return;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk(&path, depth + 1, scanned, visit),
            Ok(t) if t.is_file() => {
                *scanned += 1;
                visit(&path);
            }
            _ => {}
        }
    }
}

/// Opens a media file in PotPlayer, or the default player if PotPlayer isn't found.
pub fn open_media(settings: &Settings, path: &str) -> Result<(), String> {
    if !Path::new(path).is_file() {
        return Err("That file isn't there any more.".into());
    }
    if let Some(player) = potplayer_path(settings) {
        return Command::new(player)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Couldn't start PotPlayer: {e}"));
    }
    open_with_shell(path)
}

/// The PotPlayer set in Settings, else the one found in the registry.
pub fn potplayer_path(settings: &Settings) -> Option<PathBuf> {
    let set = settings.potplayer_path.trim();
    if !set.is_empty() && Path::new(set).is_file() {
        return Some(PathBuf::from(set));
    }
    detect_potplayer()
}

pub fn detect_potplayer() -> Option<PathBuf> {
    let out = Command::new("reg")
        .args(["query", r"HKCU\Software\DAUM\PotPlayer64", "/v", "ProgramFolder"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let folder = text
        .lines()
        .find(|l| l.contains("ProgramFolder"))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(str::trim)?;
    ["PotPlayerMini64.exe", "PotPlayer64.exe"]
        .iter()
        .map(|exe| Path::new(folder).join(exe))
        .find(|p| p.is_file())
}

// ── Games ─────────────────────────────────────────────────────────────────────

/// The game whose name or keywords best match `query`.
pub fn find_game<'a>(settings: &'a Settings, query: &str) -> Option<&'a Game> {
    let words = normalize_words(query);
    if words.is_empty() {
        return None;
    }
    let q = words.join(" ");
    settings
        .games
        .iter()
        .filter_map(|g| {
            let exact = std::iter::once(&g.name)
                .chain(g.keywords.iter())
                .any(|k| normalize_words(k).join(" ") == q);
            let score = if exact {
                1000
            } else {
                let hay = format!("{} {}", g.name, g.keywords.join(" "));
                match_score(&words, &hay, &g.name)
            };
            (score > 0).then_some((score, g))
        })
        .max_by_key(|(s, _)| *s)
        .map(|(_, g)| g)
}

pub fn launch_game(game: &Game) -> Result<(), String> {
    let target = game.path.trim();
    if target.starts_with("steam://") || target.starts_with("com.epicgames.launcher://") {
        return open_uri(target);
    }
    let path = Path::new(target);
    if !path.exists() {
        return Err(format!("Can't find {} at {target}", game.name));
    }
    let is_exe = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if is_exe {
        // Many games expect to start from their own folder.
        let mut cmd = Command::new(path);
        if let Some(dir) = path.parent() {
            cmd.current_dir(dir);
        }
        return cmd.spawn().map(|_| ()).map_err(|e| format!("Couldn't start {}: {e}", game.name));
    }
    // .lnk / .url shortcuts: let Windows resolve them.
    open_with_shell(target)
}

// ── Web ───────────────────────────────────────────────────────────────────────

pub fn web_search(query: &str) -> Result<(), String> {
    open_uri(&format!("https://duckduckgo.com/?q={}", encode(query)))
}

pub fn youtube_search(query: &str) -> Result<(), String> {
    open_uri(&format!("https://www.youtube.com/results?search_query={}", encode(query)))
}

// ── Chat commands ─────────────────────────────────────────────────────────────

/// What a chat line asked for, when it's a command rather than a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Game,
    Music,
    Video,
    Search,
    Youtube,
}

pub struct Handled {
    pub action: Action,
    pub reply: String,
}

const GAME_VERBS: &[&str] = &[
    "abre", "abrir", "abreme", "ejecuta", "lanza", "inicia", "arranca", "juega a", "juega", "jugar a", "jugar",
    "quiero jugar a", "quiero jugar", "open", "launch", "start", "run", "play",
];
const MEDIA_VERBS: &[&str] = &[
    "ponme", "pon", "reproduce", "reproducir", "quiero escuchar", "quiero ver", "play",
    "watch", "listen to",
];
const MEDIA_FILLERS: &[&str] = &[
    "la cancion", "una cancion", "el video", "un video", "la pelicula", "el capitulo", "el episodio", "la serie",
    "musica de", "the song", "the video", "the movie", "music by", "song", "video", "movie",
];
const SEARCH_VERBS: &[&str] = &[
    "busca en internet", "busca en google", "buscame", "busca", "buscar", "search for", "search", "look up", "google",
];
const SPANISH_HINTS: &[&str] = &[
    "abre", "abrir", "abreme", "ejecuta", "lanza", "inicia", "arranca", "juega", "jugar", "quiero", "ponme",
    "pon", "reproduce", "reproducir", "busca", "buscar", "buscame", "youtube",
];

/// Runs the command in `text` if there is one. None = let the model answer.
pub fn handle_command(settings: &Settings, text: &str) -> Option<Result<Handled, String>> {
    let words = normalize_words(text);
    let line = words.join(" ");
    let es = words.first().is_some_and(|w| SPANISH_HINTS.contains(&w.as_str()));
    let say = |en: String, sp: String| if es { sp } else { en };

    // "<x> en youtube", "<x> on youtube", "youtube <x>", "pon en youtube <x>"
    let yt = line
        .strip_suffix(" en youtube")
        .or_else(|| line.strip_suffix(" on youtube"))
        .or_else(|| line.strip_prefix("youtube "))
        .map(|rest| strip_verb(rest, MEDIA_VERBS).unwrap_or(rest).to_string())
        .or_else(|| {
            strip_verb(&line, MEDIA_VERBS)
                .and_then(|r| r.strip_prefix("en youtube ").or_else(|| r.strip_prefix("on youtube ")))
                .map(str::to_string)
        });
    if let Some(q) = yt.filter(|q| !q.is_empty()) {
        return Some(youtube_search(&q).map(|_| Handled {
            action: Action::Youtube,
            reply: say(format!("Opening \"{q}\" on YouTube."), format!("Abriendo \"{q}\" en YouTube.")),
        }));
    }

    if let Some(q) = strip_verb(&line, SEARCH_VERBS).filter(|q| !q.is_empty()) {
        return Some(web_search(q).map(|_| Handled {
            action: Action::Search,
            reply: say(format!("Searching the web for \"{q}\"."), format!("Buscando \"{q}\" en la web.")),
        }));
    }

    if let Some(q) = strip_verb(&line, GAME_VERBS) {
        if let Some(game) = find_game(settings, strip_filler(q, &["el juego", "the game"])) {
            let name = game.name.clone();
            return Some(launch_game(game).map(|_| Handled {
                action: Action::Game,
                reply: say(format!("Starting {name}. Have fun!"), format!("Abriendo {name}. ¡A jugar!")),
            }));
        }
    }

    if let Some(q) = strip_verb(&line, MEDIA_VERBS) {
        let q = strip_filler(q, MEDIA_FILLERS);
        if q.is_empty() {
            return None;
        }
        let music = search_media(settings, MediaKind::Music, q);
        let video = search_media(settings, MediaKind::Video, q);
        let best = [(MediaKind::Music, music.first()), (MediaKind::Video, video.first())]
            .into_iter()
            .filter_map(|(k, item)| item.map(|i| (k, i)))
            .max_by_key(|(_, i)| i.score);
        if let Some((kind, item)) = best {
            let name = item.name.clone();
            let action = if kind == MediaKind::Music { Action::Music } else { Action::Video };
            return Some(open_media(settings, &item.path).map(|_| Handled {
                action,
                reply: say(format!("Playing {name}."), format!("Reproduciendo {name}.")),
            }));
        }
        return Some(Err(say(
            format!("I couldn't find \"{q}\" in your media folders."),
            format!("No encontré \"{q}\" en tus carpetas."),
        )));
    }
    None
}

/// The rest of `line` after one of `verbs`, longest verb first.
fn strip_verb<'a>(line: &'a str, verbs: &[&str]) -> Option<&'a str> {
    let mut sorted: Vec<&&str> = verbs.iter().collect();
    sorted.sort_by_key(|v| std::cmp::Reverse(v.len()));
    sorted.into_iter().find_map(|v| {
        if line == *v {
            Some("")
        } else {
            line.strip_prefix(*v).and_then(|r| r.strip_prefix(' '))
        }
    })
}

fn strip_filler<'a>(q: &'a str, fillers: &[&str]) -> &'a str {
    for f in fillers {
        if let Some(rest) = q.strip_prefix(f).and_then(|r| r.strip_prefix(' ')) {
            return rest;
        }
    }
    q
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn open_uri(uri: &str) -> Result<(), String> {
    Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", uri])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn open_with_shell(path: &str) -> Result<(), String> {
    Command::new("explorer.exe").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Lowercase, accents removed, punctuation to spaces, split into words.
pub fn normalize_words(s: &str) -> Vec<String> {
    let folded: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect();
    folded.split_whitespace().map(str::to_string).collect()
}

/// 0 = no match. Every query word must appear; whole-word and name hits rank higher.
fn match_score(words: &[String], haystack: &str, name: &str) -> u32 {
    if words.is_empty() {
        return 0;
    }
    let hay = normalize_words(haystack);
    let name_words = normalize_words(name);
    let mut score = 0;
    for w in words {
        if name_words.iter().any(|h| h == w) {
            score += 10;
        } else if hay.iter().any(|h| h == w) {
            score += 6;
        } else if name_words.iter().any(|h| h.starts_with(w.as_str())) {
            score += 4;
        } else if hay.iter().any(|h| h.contains(w.as_str())) {
            score += 2;
        } else {
            return 0;
        }
    }
    // Shorter names that match everything are closer matches.
    score * 10 + 10u32.saturating_sub(name_words.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_accents_and_punctuation() {
        assert_eq!(normalize_words("Canción_Ñandú - (2024)"), ["cancion", "nandu", "2024"]);
    }

    #[test]
    fn scores_require_every_word() {
        let w = normalize_words("breaking bad");
        assert!(match_score(&w, "Series/Breaking Bad S01E01", "S01E01") > 0);
        assert_eq!(match_score(&w, "Series/Better Call Saul", "S01E01"), 0);
    }

    #[test]
    fn finds_games_by_keyword() {
        let mut s = Settings::default();
        s.games = vec![
            Game { name: "League of Legends".into(), keywords: vec!["lol".into()], path: "x".into() },
            Game { name: "Minecraft".into(), keywords: vec![], path: "y".into() },
        ];
        assert_eq!(find_game(&s, "LoL").map(|g| g.name.as_str()), Some("League of Legends"));
        assert_eq!(find_game(&s, "minecraft").map(|g| g.name.as_str()), Some("Minecraft"));
        assert!(find_game(&s, "tetris").is_none());
    }

    #[test]
    fn strips_command_verbs() {
        assert_eq!(strip_verb("quiero jugar a lol", GAME_VERBS), Some("lol"));
        assert_eq!(strip_verb("ponme bohemian rhapsody", MEDIA_VERBS), Some("bohemian rhapsody"));
        assert_eq!(strip_verb("que hora es", GAME_VERBS), None);
        assert_eq!(strip_filler("la cancion hello", MEDIA_FILLERS), "hello");
    }

    #[test]
    fn questions_go_to_the_model() {
        let s = Settings::default();
        assert!(handle_command(&s, "¿Qué tiempo hace hoy?").is_none());
        assert!(handle_command(&s, "Tell me a joke").is_none());
    }

    #[test]
    fn encodes_queries() {
        assert_eq!(encode("café & té"), "caf%C3%A9+%26+t%C3%A9");
    }
}
