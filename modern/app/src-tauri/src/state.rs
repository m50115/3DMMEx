//! Shared application state (Tauri managed state).
//!
//! `cpal::Stream` (inside `rodio::OutputStream` inside `AudioPlayer`) is not `Send`
//! due to a `PhantomData<*mut ()>` guard. Tauri's managed state requires `Send + Sync`.
//!
//! Solution: the audio player lives on its own dedicated thread. Commands are sent
//! to it via a `std::sync::mpsc::SyncSender`, which IS `Send`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chunky_format::ChunkyFile;
use renderer::headless::HeadlessRenderer;

use crate::commands::{MovieInfo, SceneInfo};

/// A .3mm file that has been opened and parsed.
pub struct LoadedFile {
    pub path: PathBuf,
    pub cfl: ChunkyFile,
    pub movie_info: MovieInfo,
    pub scenes: Vec<SceneInfo>,
}

/// Commands for the background audio thread.
pub enum AudioCmd {
    PlayWav(Vec<u8>),
    Stop,
}

/// Global application state — all fields must be Send + Sync.
pub struct AppState {
    /// Path to the content-files/ directory.
    pub content_dir: Mutex<Option<PathBuf>>,
    /// Currently opened .3mm file.
    pub loaded_file: Mutex<Option<LoadedFile>>,
    /// Parsed tmpls.3cn — cached at open_file, reused every render_scene_frame.
    pub tmpls: Mutex<Option<Arc<ChunkyFile>>>,
    /// Parsed tdfs.3cn — fallback BMDL source for fan-made movies using TDT chunks.
    pub tdfs: Mutex<Option<Arc<ChunkyFile>>>,
    /// Parsed bkgds.3cn — provides BrCamera for each BKGD chunk.
    pub bkgds: Mutex<Option<Arc<ChunkyFile>>>,
    /// Headless GPU renderer (None if no GPU adapter found).
    pub gpu: Mutex<Option<HeadlessRenderer>>,
    /// Channel to the dedicated audio thread (None if audio init failed).
    pub audio_tx: Mutex<Option<std::sync::mpsc::SyncSender<AudioCmd>>>,
}

impl AppState {
    pub fn new() -> Self {
        // Try to spawn an audio thread.
        let audio_tx = Self::try_spawn_audio_thread();

        Self {
            content_dir: Mutex::new(None),
            loaded_file: Mutex::new(None),
            tmpls: Mutex::new(None),
            tdfs: Mutex::new(None),
            bkgds: Mutex::new(None),
            gpu: Mutex::new(HeadlessRenderer::try_new()),
            audio_tx: Mutex::new(audio_tx),
        }
    }

    fn try_spawn_audio_thread() -> Option<std::sync::mpsc::SyncSender<AudioCmd>> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<AudioCmd>(8);
        // One-shot channel to report whether AudioPlayer init succeeded.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<bool>();

        // AudioPlayer is NOT Send (cpal::Stream contains non-Send raw pointers on macOS).
        // Solution: create it INSIDE the spawned thread — it never needs to cross thread boundaries.
        // The closure only captures `rx` (Receiver<AudioCmd>) and `ready_tx`, both of which ARE Send.
        std::thread::spawn(move || {
            let player = audio::AudioPlayer::try_new();
            let _ = ready_tx.send(player.is_some());
            if let Some(p) = player {
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        AudioCmd::PlayWav(wav_bytes) => { let _ = p.play_wav(wav_bytes); }
                        AudioCmd::Stop => { p.stop(); }
                    }
                }
            }
        });

        // Block until the audio thread confirms whether a player was created.
        match ready_rx.recv() {
            Ok(true) => Some(tx),
            _ => None,
        }
    }
}
