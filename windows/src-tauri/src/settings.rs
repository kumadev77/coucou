// Preferences, stored as plain JSON in %APPDATA%\Coucou\settings.json.
// No secret ever lands here — API keys live in the Windows Credential Manager.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub sound_enabled: bool,
    pub sound_volume: f64,
    pub auto_close_interval: f64,
    pub absence_interval: f64,
    pub active_integrations: Vec<String>,
    /// "primary" = the main display, "cursor" = whichever display the mouse is on.
    pub screen: String,
    pub autostart: bool,
    pub hooks_installed: bool,
    /// Claude model used by the chat. Changeable in the settings window.
    /// Defaulted explicitly so a settings.json written by an older build still loads.
    #[serde(default = "default_model")]
    pub model: String,
    /// Chat provider: "anthropic", "ollama" or "llamacpp".
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default)]
    pub ollama_model: String,
    #[serde(default = "default_llamacpp_url")]
    pub llamacpp_url: String,
    /// llama-server serves whatever model it was started with, so this is optional.
    #[serde(default)]
    pub llamacpp_model: String,
    /// Let local reasoning models think before answering. Off = faster replies.
    #[serde(default)]
    pub local_thinking: bool,
    /// Folders Mochi searches for "pon / reproduce", grouped by category.
    #[serde(default = "default_media_categories")]
    pub media_categories: Vec<MediaCategory>,
    /// Empty = detect PotPlayer from the registry.
    #[serde(default)]
    pub potplayer_path: String,
    /// Games Mochi can open by name or keyword.
    #[serde(default)]
    pub games: Vec<Game>,
    /// Personalization panel. Empty colour strings = Mochi's own colours.
    #[serde(default = "default_mochi_name")]
    pub mochi_name: String,
    #[serde(default)]
    pub mochi_body: String,
    #[serde(default)]
    pub mochi_eyes: String,
    #[serde(default)]
    pub mochi_accent: String,
    /// Free text added to the model's instructions ("sarcastic", "talks like a pirate").
    #[serde(default)]
    pub mochi_personality: String,
    /// Character size multiplier, 0.8 to 1.3.
    #[serde(default = "default_mochi_scale")]
    pub mochi_scale: f64,
    /// Sound names that never play.
    #[serde(default)]
    pub muted_sounds: Vec<String>,
    /// Glasses when searching, dancing to music, a controller for games.
    #[serde(default = "default_true")]
    pub mochi_reactions: bool,
}

fn default_mochi_name() -> String {
    "Mochi".into()
}

fn default_mochi_scale() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCategory {
    pub name: String,
    pub kind: crate::launcher::MediaKind,
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub name: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// .exe, .lnk / .url shortcut, or a steam:// link.
    pub path: String,
}

fn default_model() -> String {
    crate::claude::DEFAULT_MODEL.to_string()
}

/// Starts from the user's own Music and Videos folders.
fn default_media_categories() -> Vec<MediaCategory> {
    use crate::launcher::MediaKind;
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    vec![
        MediaCategory { name: "Music".into(), kind: MediaKind::Music, folders: vec![format!(r"{home}\Music")] },
        MediaCategory { name: "Videos".into(), kind: MediaKind::Video, folders: vec![format!(r"{home}\Videos")] },
    ]
}

fn default_provider() -> String {
    "anthropic".into()
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
}

fn default_llamacpp_url() -> String {
    "http://localhost:8080".into()
}

impl Settings {
    /// The name and personality the model is told to have.
    pub fn persona(&self) -> crate::claude::Persona {
        let name = self.mochi_name.trim();
        crate::claude::Persona {
            name: if name.is_empty() { "Mochi".into() } else { name.to_string() },
            personality: self.mochi_personality.trim().to_string(),
        }
    }

    /// Where the chat sends its next turn.
    pub fn chat_backend(&self) -> crate::claude::Backend {
        use crate::claude::Backend;
        match self.provider.as_str() {
            "ollama" => Backend::Local {
                name: "Ollama",
                base_url: self.ollama_url.clone(),
                model: self.ollama_model.clone(),
                thinking: self.local_thinking,
            },
            "llamacpp" => Backend::Local {
                name: "llama.cpp",
                base_url: self.llamacpp_url.clone(),
                model: self.llamacpp_model.clone(),
                thinking: self.local_thinking,
            },
            _ => Backend::Anthropic { model: self.model.clone() },
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_volume: 0.12,
            auto_close_interval: 15.0,
            absence_interval: 180.0,
            active_integrations: vec![
                "integration_resend".into(),
                "integration_n8n".into(),
                "integration_vercel".into(),
                "integration_github".into(),
            ],
            screen: "primary".into(),
            autostart: false,
            hooks_installed: false,
            model: default_model(),
            provider: default_provider(),
            ollama_url: default_ollama_url(),
            ollama_model: String::new(),
            llamacpp_url: default_llamacpp_url(),
            llamacpp_model: String::new(),
            local_thinking: false,
            media_categories: default_media_categories(),
            potplayer_path: String::new(),
            games: Vec::new(),
            mochi_name: default_mochi_name(),
            mochi_body: String::new(),
            mochi_eyes: String::new(),
            mochi_accent: String::new(),
            mochi_personality: String::new(),
            mochi_scale: default_mochi_scale(),
            muted_sounds: Vec::new(),
            mochi_reactions: true,
        }
    }
}

/// %APPDATA%\Coucou
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Coucou")
}

/// %LOCALAPPDATA%\Coucou — where coucou-hook.exe and the log live.
pub fn local_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Coucou")
}

pub fn hook_exe_path() -> PathBuf {
    local_dir().join("bin").join("coucou-hook.exe")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> Settings {
    match std::fs::read(settings_path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(settings_path(), json)
}
