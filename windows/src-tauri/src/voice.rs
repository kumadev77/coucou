// Voice: "Hey Mochi" wake word and speech to text with Vosk, spoken answers with
// Piper. Everything runs on this PC.
//
// Nothing here is a build dependency. The Vosk DLL, the speech models, Piper and
// its voices are downloaded from Settings into %LOCALAPPDATA%\Coucou\voice the
// first time voice is turned on, and libvosk.dll is loaded at runtime only then.
// The microphone is read through winmm's waveIn, which converts to 16 kHz mono
// for us, and answers are played with PlaySound. With voice off, none of this
// runs and nothing is loaded.

use std::ffi::{c_char, c_void, CStr, CString};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::settings::{self, Settings};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SAMPLE_RATE: u32 = 16_000;
/// 100 ms of 16-bit mono audio per buffer.
const BUF_BYTES: usize = (SAMPLE_RATE as usize / 10) * 2;
const BUFFERS: usize = 4;
/// How long Mochi waits for a command after the wake word.
const LISTEN_SECS: u64 = 8;

// ── What gets downloaded ──────────────────────────────────────────────────────

const VOSK_RUNTIME: &str = "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/vosk-win64-0.3.45.zip";
const VOSK_DIR: &str = "vosk-win64-0.3.45";
const PIPER_ZIP: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip";
const VOICES_BASE: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main/";

struct Lang {
    code: &'static str,
    model_zip: &'static str,
    model_dir: &'static str,
    /// Piper voice path inside the piper-voices repo, without ".onnx".
    voice: &'static str,
    wake_words: &'static [&'static str],
    /// Said while Mochi talks: it stops. Words its answers rarely contain.
    stop_words: &'static [&'static str],
}

const LANGS: &[Lang] = &[
    Lang {
        code: "en",
        model_zip: "https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip",
        model_dir: "vosk-model-small-en-us-0.15",
        voice: "en/en_US/lessac/medium/en_US-lessac-medium",
        wake_words: &["hey", "hi", "okay"],
        stop_words: &["stop", "quiet", "shut up", "enough"],
    },
    Lang {
        code: "es",
        model_zip: "https://alphacephei.com/vosk/models/vosk-model-small-es-0.42.zip",
        model_dir: "vosk-model-small-es-0.42",
        voice: "es/es_MX/claude/high/es_MX-claude-high",
        wake_words: &["oye", "hey", "hola"],
        stop_words: &["basta", "silencio", "calla", "stop"],
    },
];

fn lang(code: &str) -> &'static Lang {
    LANGS.iter().find(|l| l.code == code).unwrap_or(&LANGS[0])
}

pub fn voice_dir() -> PathBuf {
    settings::local_dir().join("voice")
}

fn voice_file(l: &Lang) -> PathBuf {
    let name = l.voice.rsplit('/').next().unwrap_or(l.voice);
    voice_dir().join("voices").join(format!("{name}.onnx"))
}

fn piper_exe() -> PathBuf {
    voice_dir().join("piper").join("piper.exe")
}

// ── Status and install ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    pub runtime: bool,
    pub english: bool,
    pub spanish: bool,
    pub piper: bool,
    pub english_voice: bool,
    pub spanish_voice: bool,
    pub listening: bool,
}

pub fn status() -> VoiceStatus {
    let d = voice_dir();
    let model = |code: &str| d.join(lang(code).model_dir).join("am").is_dir();
    VoiceStatus {
        runtime: d.join(VOSK_DIR).join("libvosk.dll").is_file(),
        english: model("en"),
        spanish: model("es"),
        piper: piper_exe().is_file(),
        english_voice: voice_file(lang("en")).is_file(),
        spanish_voice: voice_file(lang("es")).is_file(),
        listening: LISTENER.get().is_some_and(|l| l.lock().unwrap().is_some()),
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    label: String,
    done: u64,
    total: u64,
}

/// `part`: "en" / "es" (speech recognition), "voice-en" / "voice-es" (Mochi
/// speaking). Skips whatever is already there.
pub async fn install(app: &AppHandle, part: &str) -> Result<(), String> {
    let d = voice_dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    match part {
        "en" | "es" => {
            if !d.join(VOSK_DIR).join("libvosk.dll").is_file() {
                fetch_zip(app, VOSK_RUNTIME, "Motor de voz", &d).await?;
            }
            let l = lang(part);
            if !d.join(l.model_dir).join("am").is_dir() {
                fetch_zip(app, l.model_zip, "Modelo de reconocimiento", &d).await?;
            }
        }
        "voice-en" | "voice-es" => {
            if !piper_exe().is_file() {
                fetch_zip(app, PIPER_ZIP, "Piper", &d).await?;
            }
            let l = lang(&part[6..]);
            let out = voice_file(l);
            if !out.is_file() {
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
                let json = PathBuf::from(format!("{}.json", out.display()));
                fetch(app, &format!("{VOICES_BASE}{}.onnx.json", l.voice), "Voz (config)", &json).await?;
                fetch(app, &format!("{VOICES_BASE}{}.onnx", l.voice), "Voz de Mochi", &out).await?;
            }
        }
        _ => return Err(format!("Unknown voice part {part}")),
    }
    Ok(())
}

/// Downloads to `<dest>.part` and renames when complete, so a cut connection
/// never leaves a file that looks finished.
async fn fetch(app: &AppHandle, url: &str, label: &str, dest: &Path) -> Result<(), String> {
    use std::io::Write;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut res = client.get(url).send().await.map_err(|e| format!("Download failed: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Download failed: {} ({url})", res.status()));
    }
    let total = res.content_length().unwrap_or(0);
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut done = 0u64;
    let mut last_emit = Instant::now();
    while let Some(chunk) = res.chunk().await.map_err(|e| format!("Download interrupted: {e}"))? {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        if last_emit.elapsed() > Duration::from_millis(250) {
            last_emit = Instant::now();
            let _ = app.emit("voice-progress", Progress { label: label.into(), done, total });
        }
    }
    drop(file);
    let _ = app.emit("voice-progress", Progress { label: label.into(), done, total: done });
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Download + unzip with Windows' own tar.exe (bsdtar reads zip files).
async fn fetch_zip(app: &AppHandle, url: &str, label: &str, into: &Path) -> Result<(), String> {
    let zip = into.join("download.zip");
    fetch(app, url, label, &zip).await?;
    let _ = app.emit("voice-progress", Progress { label: format!("{label}: descomprimiendo"), done: 0, total: 0 });
    let tar = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
        .join("System32")
        .join("tar.exe");
    let out = Command::new(tar)
        .arg("-xf")
        .arg(&zip)
        .arg("-C")
        .arg(into)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Couldn't unzip: {e}"))?;
    let _ = std::fs::remove_file(&zip);
    if !out.status.success() {
        return Err(format!("Couldn't unzip: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

// ── Win32, declared by hand: kernel32 and winmm are always there ──────────────

#[repr(C, packed)]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    cb_size: u16,
}

#[repr(C)]
struct WaveHdr {
    data: *mut c_char,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHdr,
    reserved: usize,
}

const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
const WAVE_FORMAT_PCM: u16 = 1;
const WHDR_DONE: u32 = 0x1;
const SND_ASYNC: u32 = 0x0001;
const SND_NODEFAULT: u32 = 0x0002;
const SND_FILENAME: u32 = 0x0002_0000;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn SetDllDirectoryW(path: *const u16) -> i32;
}

#[link(name = "winmm")]
extern "system" {
    fn waveInOpen(h: *mut *mut c_void, dev: u32, fmt: *const WaveFormatEx, cb: usize, inst: usize, flags: u32) -> u32;
    fn waveInPrepareHeader(h: *mut c_void, hdr: *mut WaveHdr, size: u32) -> u32;
    fn waveInUnprepareHeader(h: *mut c_void, hdr: *mut WaveHdr, size: u32) -> u32;
    fn waveInAddBuffer(h: *mut c_void, hdr: *mut WaveHdr, size: u32) -> u32;
    fn waveInStart(h: *mut c_void) -> u32;
    fn waveInReset(h: *mut c_void) -> u32;
    fn waveInClose(h: *mut c_void) -> u32;
    fn PlaySoundW(sound: *const u16, module: *mut c_void, flags: u32) -> i32;
}

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

// ── Vosk, loaded at runtime ───────────────────────────────────────────────────

type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type ModelFree = unsafe extern "C" fn(*mut c_void);
type RecNew = unsafe extern "C" fn(*mut c_void, f32) -> *mut c_void;
type RecNewGrm = unsafe extern "C" fn(*mut c_void, f32, *const c_char) -> *mut c_void;
type RecAccept = unsafe extern "C" fn(*mut c_void, *const c_char, i32) -> i32;
type RecText = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type RecVoid = unsafe extern "C" fn(*mut c_void);
type SetLogLevel = unsafe extern "C" fn(i32);

struct Vosk {
    model_new: ModelNew,
    model_free: ModelFree,
    rec_new: RecNew,
    rec_new_grm: RecNewGrm,
    accept: RecAccept,
    result: RecText,
    partial: RecText,
    final_result: RecText,
    reset: RecVoid,
    rec_free: RecVoid,
}

// The function pointers are plain C entry points; sharing them is fine.
unsafe impl Send for Vosk {}
unsafe impl Sync for Vosk {}

static VOSK: OnceLock<Result<Vosk, String>> = OnceLock::new();

fn vosk() -> Result<&'static Vosk, String> {
    VOSK.get_or_init(load_vosk).as_ref().map_err(Clone::clone)
}

fn load_vosk() -> Result<Vosk, String> {
    let dir = voice_dir().join(VOSK_DIR);
    let dll = dir.join("libvosk.dll");
    if !dll.is_file() {
        return Err("Voice isn't downloaded yet.".into());
    }
    unsafe {
        // libvosk.dll needs the MinGW runtime DLLs sitting next to it.
        SetDllDirectoryW(wide(&dir).as_ptr());
        let module = LoadLibraryW(wide(&dll).as_ptr());
        SetDllDirectoryW(std::ptr::null());
        if module.is_null() {
            return Err("Couldn't load libvosk.dll.".into());
        }
        macro_rules! sym {
            ($name:literal, $ty:ty) => {{
                let p = GetProcAddress(module, concat!($name, "\0").as_ptr() as *const c_char);
                if p.is_null() {
                    return Err(format!("libvosk.dll has no {}", $name));
                }
                std::mem::transmute::<*mut c_void, $ty>(p)
            }};
        }
        let set_log: SetLogLevel = sym!("vosk_set_log_level", SetLogLevel);
        set_log(-1);
        Ok(Vosk {
            model_new: sym!("vosk_model_new", ModelNew),
            model_free: sym!("vosk_model_free", ModelFree),
            rec_new: sym!("vosk_recognizer_new", RecNew),
            rec_new_grm: sym!("vosk_recognizer_new_grm", RecNewGrm),
            accept: sym!("vosk_recognizer_accept_waveform", RecAccept),
            result: sym!("vosk_recognizer_result", RecText),
            partial: sym!("vosk_recognizer_partial_result", RecText),
            final_result: sym!("vosk_recognizer_final_result", RecText),
            reset: sym!("vosk_recognizer_reset", RecVoid),
            rec_free: sym!("vosk_recognizer_free", RecVoid),
        })
    }
}

/// `{"text": "..."}` or `{"partial": "..."}` → the words.
fn json_text(ptr: *const c_char, key: &str) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let raw = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
    serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|v| v.get(key).and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

// ── Listening ─────────────────────────────────────────────────────────────────

struct Listener {
    stop: Arc<AtomicBool>,
    /// What it was started with; a change restarts it.
    key: String,
}

static LISTENER: OnceLock<Mutex<Option<Listener>>> = OnceLock::new();
/// True while Mochi talks, so it doesn't hear itself.
static SPEAKING: AtomicBool = AtomicBool::new(false);

/// Starts, restarts or stops listening to match the settings.
pub fn sync(app: &AppHandle, s: &Settings) {
    let _ = APP.set(app.clone());
    // Before taking the lock: status() reads it too.
    let ready = status();
    let slot = LISTENER.get_or_init(|| Mutex::new(None));
    let mut current = slot.lock().unwrap();
    let l = lang(&s.voice_lang);
    let installed = ready.runtime && if l.code == "es" { ready.spanish } else { ready.english };
    let want = s.voice_enabled && installed;
    let key = format!("{}|{}", l.code, s.mochi_name.trim().to_lowercase());

    if let Some(running) = current.as_ref() {
        if want && running.key == key {
            return;
        }
        running.stop.store(true, Ordering::Relaxed);
        *current = None;
    }
    if !want {
        return;
    }
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let app = app.clone();
    let name = s.mochi_name.trim().to_lowercase();
    std::thread::spawn(move || {
        if let Err(err) = listen(&app, l, &name, &thread_stop) {
            crate::log::line(format!("voice: {err}"));
            let _ = app.emit("voice-error", err);
        }
    });
    *current = Some(Listener { stop, key });
}

enum Mode {
    Wake,
    Command { until: Instant },
}

fn listen(app: &AppHandle, l: &'static Lang, name: &str, stop: &AtomicBool) -> Result<(), String> {
    let v = vosk()?;
    let model_path = CString::new(voice_dir().join(l.model_dir).to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    let name_words: Vec<String> = crate::launcher::normalize_words(name);
    let name = if name_words.is_empty() { "mochi".to_string() } else { name_words.join(" ") };
    let grammar: Vec<String> = l
        .wake_words
        .iter()
        .map(|w| format!("{w} {name}"))
        .chain([name.clone()])
        .chain(l.stop_words.iter().map(|w| w.to_string()))
        .chain(["[unk]".to_string()])
        .collect();
    let stop_words: Vec<String> = l.stop_words.iter().map(|w| w.to_string()).collect();
    let grammar = CString::new(serde_json::to_string(&grammar).unwrap()).unwrap();

    unsafe {
        let model = (v.model_new)(model_path.as_ptr());
        if model.is_null() {
            return Err("Couldn't load the speech model.".into());
        }
        let wake = (v.rec_new_grm)(model, SAMPLE_RATE as f32, grammar.as_ptr());
        let full = (v.rec_new)(model, SAMPLE_RATE as f32);
        let result = if wake.is_null() || full.is_null() {
            Err("Couldn't start speech recognition.".into())
        } else {
            capture(stop, |pcm| feed(app, v, wake, full, pcm, &name, &stop_words))
        };
        if !wake.is_null() {
            (v.rec_free)(wake);
        }
        if !full.is_null() {
            (v.rec_free)(full);
        }
        (v.model_free)(model);
        result
    }
}

thread_local! {
    static MODE: std::cell::RefCell<Mode> = const { std::cell::RefCell::new(Mode::Wake) };
}

/// One 100 ms chunk of audio through whichever recognizer is active.
/// `wake` and `full` are live recognizers owned by `listen`.
fn feed(
    app: &AppHandle,
    v: &Vosk,
    wake: *mut c_void,
    full: *mut c_void,
    pcm: &[u8],
    name: &str,
    stop_words: &[String],
) {
    let speaking = SPEAKING.load(Ordering::Relaxed);
    MODE.with(|mode| unsafe {
        let mut mode = mode.borrow_mut();
        // While Mochi talks only the wake grammar runs: that's the barge-in.
        if speaking && matches!(*mode, Mode::Command { .. }) {
            *mode = Mode::Wake;
        }
        match *mode {
            Mode::Wake => {
                let done = (v.accept)(wake, pcm.as_ptr() as *const c_char, pcm.len() as i32) == 1;
                let heard = if done { json_text((v.result)(wake), "text") } else { json_text((v.partial)(wake), "partial") };
                if done && heard.split_whitespace().any(|w| w != "[unk]") {
                    crate::log::line(format!("voice wake grammar heard: {heard:?}{}", if speaking { " (while speaking)" } else { "" }));
                }
                if speaking && stop_words.iter().any(|w| heard.contains(w.as_str())) {
                    (v.reset)(wake);
                    stop_speaking();
                    let _ = app.emit("voice-stopped", ());
                    return;
                }
                if heard.split_whitespace().any(|w| name.split_whitespace().any(|n| n == w)) {
                    if speaking {
                        stop_speaking();
                    }
                    (v.reset)(wake);
                    (v.reset)(full);
                    *mode = Mode::Command { until: Instant::now() + Duration::from_secs(LISTEN_SECS) };
                    let _ = app.emit("voice-wake", ());
                }
            }
            Mode::Command { until } => {
                let done = (v.accept)(full, pcm.as_ptr() as *const c_char, pcm.len() as i32) == 1;
                if done {
                    let text = json_text((v.result)(full), "text");
                    crate::log::line(format!("voice heard: {text:?}"));
                    if !text.is_empty() {
                        let _ = app.emit("voice-text", text);
                        *mode = Mode::Wake;
                    }
                    return;
                }
                let partial = json_text((v.partial)(full), "partial");
                if !partial.is_empty() {
                    let _ = app.emit("voice-partial", partial);
                }
                if Instant::now() > until {
                    let text = json_text((v.final_result)(full), "text");
                    let _ = app.emit(if text.is_empty() { "voice-timeout" } else { "voice-text" }, text);
                    *mode = Mode::Wake;
                }
            }
        }
    });
}

/// Reads the default microphone as 16 kHz mono 16-bit until `stop` is set.
fn capture(stop: &AtomicBool, mut on_audio: impl FnMut(&[u8])) -> Result<(), String> {
    unsafe {
        let fmt = WaveFormatEx {
            format_tag: WAVE_FORMAT_PCM,
            channels: 1,
            samples_per_sec: SAMPLE_RATE,
            avg_bytes_per_sec: SAMPLE_RATE * 2,
            block_align: 2,
            bits_per_sample: 16,
            cb_size: 0,
        };
        let mut h: *mut c_void = std::ptr::null_mut();
        let rc = waveInOpen(&mut h, WAVE_MAPPER, &fmt, 0, 0, 0);
        if rc != 0 {
            return Err(format!("Couldn't open the microphone (error {rc}). Is one connected and allowed?"));
        }
        let hdr_size = std::mem::size_of::<WaveHdr>() as u32;
        let mut data: Vec<Vec<u8>> = (0..BUFFERS).map(|_| vec![0u8; BUF_BYTES]).collect();
        let mut hdrs: Vec<Box<WaveHdr>> = data
            .iter_mut()
            .map(|d| {
                Box::new(WaveHdr {
                    data: d.as_mut_ptr() as *mut c_char,
                    buffer_length: BUF_BYTES as u32,
                    bytes_recorded: 0,
                    user: 0,
                    flags: 0,
                    loops: 0,
                    next: std::ptr::null_mut(),
                    reserved: 0,
                })
            })
            .collect();
        for hdr in hdrs.iter_mut() {
            waveInPrepareHeader(h, &mut **hdr, hdr_size);
            waveInAddBuffer(h, &mut **hdr, hdr_size);
        }
        waveInStart(h);

        let mut next = 0;
        while !stop.load(Ordering::Relaxed) {
            let hdr: &mut WaveHdr = &mut hdrs[next];
            // The driver sets WHDR_DONE from its own thread.
            if std::ptr::read_volatile(&hdr.flags) & WHDR_DONE == 0 {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            let n = hdr.bytes_recorded as usize;
            on_audio(&data[next][..n]);
            hdr.flags &= !WHDR_DONE;
            waveInAddBuffer(h, hdr, hdr_size);
            next = (next + 1) % BUFFERS;
        }

        waveInReset(h);
        for hdr in hdrs.iter_mut() {
            waveInUnprepareHeader(h, &mut **hdr, hdr_size);
        }
        waveInClose(h);
        drop(data);
        Ok(())
    }
}

// ── Speaking ──────────────────────────────────────────────────────────────────

/// Bumped by every new answer and every stop: a playback that sees a newer
/// number than its own knows it has been cut off.
static SPEECH_GEN: AtomicU64 = AtomicU64::new(0);
static APP: OnceLock<AppHandle> = OnceLock::new();

fn set_speaking(on: bool) {
    SPEAKING.store(on, Ordering::Relaxed);
    if let Some(app) = APP.get() {
        let _ = app.emit("voice-speaking", on);
    }
}

/// Says `text` with Piper in the background. A new call interrupts the old one.
pub fn speak(app: &AppHandle, s: &Settings, text: &str) -> Result<(), String> {
    let _ = APP.set(app.clone());
    let l = lang(&s.voice_lang);
    let model = voice_file(l);
    if !piper_exe().is_file() || !model.is_file() {
        return Err("Mochi's voice isn't downloaded yet.".into());
    }
    let text = text.to_string();
    let speed = (1.0 / s.voice_speed.clamp(0.5, 2.0)).to_string();
    stop_speaking();
    let gen = SPEECH_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::spawn(move || {
        let wav = voice_dir().join(format!("answer-{}.wav", gen % 2));
        let child = Command::new(piper_exe())
            .arg("--model")
            .arg(&model)
            .arg("--length_scale")
            .arg(&speed)
            .arg("--output_file")
            .arg(&wav)
            .current_dir(voice_dir().join("piper"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        let Ok(mut child) = child else { return };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }
        if !child.wait().is_ok_and(|s| s.success()) || SPEECH_GEN.load(Ordering::Relaxed) != gen {
            return; // failed, or stopped while Piper was still writing
        }
        let Some(length) = wav_duration(&wav) else { return };
        // Async playback is the kind PlaySound(NULL) can cut off from any thread.
        set_speaking(true);
        unsafe {
            PlaySoundW(wide(&wav).as_ptr(), std::ptr::null_mut(), SND_FILENAME | SND_NODEFAULT | SND_ASYNC);
        }
        // A short tail so the room echo isn't taken for speech.
        let until = Instant::now() + length + Duration::from_millis(300);
        while Instant::now() < until {
            if SPEECH_GEN.load(Ordering::Relaxed) != gen {
                return; // stop_speaking already cleared the flag
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if SPEECH_GEN.load(Ordering::Relaxed) == gen {
            set_speaking(false);
        }
    });
    Ok(())
}

/// Length of a PCM WAV from its header.
fn wav_duration(path: &Path) -> Option<Duration> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]) as u64;
    let u32_at = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as u64;
    let channels = u16_at(22).max(1);
    let rate = u32_at(24).max(1);
    let bits = u16_at(34).max(8);
    let data = bytes.len() as u64 - 44;
    Some(Duration::from_millis(data * 1000 / (rate * channels * bits / 8)))
}

pub fn stop_speaking() {
    SPEECH_GEN.fetch_add(1, Ordering::Relaxed);
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
    if SPEAKING.load(Ordering::Relaxed) {
        set_speaking(false);
    }
}

/// Longest answer read aloud; the rest stays on screen.
const MAX_SPOKEN_CHARS: usize = 320;

/// Markdown and symbols read badly aloud, and long answers are cut at the last
/// sentence that fits: the full text is in the chat.
pub fn plain_for_speech(text: &str) -> String {
    let plain = text
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '>' | '~' | '|'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if plain.chars().count() <= MAX_SPOKEN_CHARS {
        return plain;
    }
    let cut: String = plain.chars().take(MAX_SPOKEN_CHARS).collect();
    match cut.rfind(['.', '!', '?']) {
        Some(end) if end > MAX_SPOKEN_CHARS / 3 => cut[..=end].to_string(),
        _ => format!("{}…", cut.trim_end()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_answers_are_cut_at_a_sentence() {
        let long = "Primera frase. ".repeat(40);
        let spoken = plain_for_speech(&long);
        assert!(spoken.chars().count() <= MAX_SPOKEN_CHARS);
        assert!(spoken.ends_with('.'));
        assert_eq!(plain_for_speech("**Hola** _mundo_"), "Hola mundo");
    }
}
