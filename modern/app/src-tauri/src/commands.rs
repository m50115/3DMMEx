//! Tauri IPC commands.
//!
//! All commands return `Result<T, String>` so errors surface cleanly
//! in the frontend JavaScript.

use std::io::BufReader;
use std::path::PathBuf;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::State;

use audio::decode_wav;
use chunky_format::ChunkyFile;
use engine::model::Model;
use engine::msnd::MovieSound;
use engine::scene::SceneHeader;
use engine::tag::{CTG_BMDL, CTG_MSND, CTG_SCEN, CTG_WAVE};

use crate::state::{AppState, LoadedFile};

// ── Public types returned to the frontend ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovieInfo {
    pub path: String,
    pub file_name: String,
    pub scene_count: usize,
    pub total_frames: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneInfo {
    pub scene_idx: usize,
    pub frame_count: i32,
    pub actor_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundEntry {
    pub cno: u32,
    pub name: Option<String>,
    pub sound_type: String,
    pub volume_default: i32,
}

// ── Commands ──────────────────────────────────────────────────────────────

/// Open a .3mm movie file, parse its structure, and return metadata.
#[tauri::command]
pub fn open_file(path: String, state: State<AppState>) -> Result<MovieInfo, String> {
    let pb = PathBuf::from(&path);
    let file = std::fs::File::open(&pb)
        .map_err(|e| format!("Cannot open {path}: {e}"))?;
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader)
        .map_err(|e| format!("Parse error: {e}"))?;

    // Infer content_dir from the file location: look for content-files/ sibling.
    // A .3mm lives in samples/ or projects/ next to content-files/.
    let parent = pb.parent().and_then(|p| p.parent());
    if let Some(base) = parent {
        let candidate = base.join("content-files");
        if candidate.exists() {
            *state.content_dir.lock().unwrap() = Some(candidate);
        }
    }

    // Count SCEN chunks and parse scene headers.
    let scen_entries: Vec<_> = cfl.chunks.iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .collect();

    let mut scenes: Vec<SceneInfo> = Vec::new();
    let mut total_frames: i32 = 0;

    for (idx, chunk) in scen_entries.iter().enumerate() {
        let frame_count = match cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno) {
            Ok(data) if data.len() >= SceneHeader::SIZE => {
                let arr: [u8; 16] = data[..16].try_into().unwrap();
                SceneHeader::from_bytes(&arr)
                    .map(|h| h.nfrm_mac)
                    .unwrap_or(0)
            }
            _ => 0,
        };

        // Count ACTR children of this SCEN chunk.
        let actor_count = chunk.children.len();

        total_frames += frame_count;
        scenes.push(SceneInfo { scene_idx: idx, frame_count, actor_count });
    }

    let file_name = pb.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());

    let info = MovieInfo {
        path: path.clone(),
        file_name,
        scene_count: scen_entries.len(),
        total_frames,
    };

    *state.loaded_file.lock().unwrap() = Some(LoadedFile {
        path: pb,
        cfl,
        movie_info: info.clone(),
        scenes,
    });

    Ok(info)
}

/// Return the scene list for the currently loaded file.
#[tauri::command]
pub fn get_scene_list(state: State<AppState>) -> Result<Vec<SceneInfo>, String> {
    let guard = state.loaded_file.lock().unwrap();
    match &*guard {
        Some(f) => Ok(f.scenes.clone()),
        None => Err("No file loaded".into()),
    }
}

/// Render the first renderable 3D model from tmpls.3cn to a PNG data URL.
///
/// Returns a `data:image/png;base64,...` string ready for an `<img>` src.
#[tauri::command]
pub fn render_demo_frame(
    width: u32,
    height: u32,
    state: State<AppState>,
) -> Result<String, String> {
    let content_dir = state.content_dir.lock().unwrap().clone()
        .ok_or_else(|| "Content directory not set (open a .3mm file first)".to_string())?;

    let tmpls_path = content_dir.join("tmpls.3cn");
    let file = std::fs::File::open(&tmpls_path)
        .map_err(|e| format!("Cannot open tmpls.3cn: {e}"))?;
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader)
        .map_err(|e| format!("Parse tmpls.3cn: {e}"))?;

    let model = cfl.chunks.iter()
        .filter(|c| c.id.ctg == CTG_BMDL)
        .find_map(|c| {
            let data = cfl.get_chunk_data(c.id.ctg, c.id.cno).ok()?;
            if data.len() < 80 { return None; }
            let m = Model::from_bytes(&data).ok()?;
            if m.has_valid_faces() && !m.vertices.is_empty() { Some(m) } else { None }
        })
        .ok_or_else(|| "No renderable model found in tmpls.3cn".to_string())?;

    let gpu_guard = state.gpu.lock().unwrap();
    let gpu = gpu_guard.as_ref()
        .ok_or_else(|| "No GPU adapter available".to_string())?;

    let rgba = gpu.render_model(&model, width, height);
    if rgba.is_empty() {
        return Err("Render produced empty output".into());
    }

    let png_bytes = encode_rgba_to_png(&rgba, width, height)?;
    let b64 = base64::prelude::BASE64_STANDARD.encode(&png_bytes);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// List sound entries from snds.3cn.
#[tauri::command]
pub fn list_sounds(state: State<AppState>) -> Result<Vec<SoundEntry>, String> {
    let content_dir = state.content_dir.lock().unwrap().clone()
        .ok_or_else(|| "Content directory not set".to_string())?;

    let snds_path = content_dir.join("snds.3cn");
    let file = std::fs::File::open(&snds_path)
        .map_err(|e| format!("Cannot open snds.3cn: {e}"))?;
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader)
        .map_err(|e| format!("Parse snds.3cn: {e}"))?;

    let mut sounds: Vec<SoundEntry> = Vec::new();

    for chunk in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_MSND) {
        let sound_type;
        let volume_default;

        if let Ok(data) = cfl.get_chunk_data(chunk.id.ctg, chunk.id.cno) {
            if let Ok(msnd) = MovieSound::from_bytes(&data) {
                sound_type = format!("{:?}", msnd.sty);
                volume_default = msnd.vlm_default;
            } else {
                sound_type = "Unknown".into();
                volume_default = 0;
            }
        } else {
            sound_type = "Unknown".into();
            volume_default = 0;
        }

        sounds.push(SoundEntry {
            cno: chunk.id.cno,
            name: chunk.name.clone(),
            sound_type,
            volume_default,
        });

        if sounds.len() >= 50 {
            break; // cap for MVP — avoid huge lists
        }
    }

    Ok(sounds)
}

/// Play the WAV audio for the given MSND cno from snds.3cn.
#[tauri::command]
pub fn play_sound(cno: u32, state: State<AppState>) -> Result<(), String> {
    let content_dir = state.content_dir.lock().unwrap().clone()
        .ok_or_else(|| "Content directory not set".to_string())?;

    let snds_path = content_dir.join("snds.3cn");
    let file = std::fs::File::open(&snds_path)
        .map_err(|e| format!("Cannot open snds.3cn: {e}"))?;
    let mut reader = BufReader::new(file);
    let cfl = ChunkyFile::read(&mut reader)
        .map_err(|e| format!("Parse snds.3cn: {e}"))?;

    // Find the MSND with the given cno.
    let msnd_chunk = cfl.chunks.iter()
        .find(|c| c.id.ctg == CTG_MSND && c.id.cno == cno)
        .ok_or_else(|| format!("MSND cno={cno} not found"))?;

    // Find the WAVE child (chid = 0).
    let wave_child = msnd_chunk.children.iter()
        .find(|ch| ch.id.ctg == CTG_WAVE)
        .ok_or_else(|| format!("No WAVE child in MSND cno={cno}"))?;

    let wave_data = cfl.get_chunk_data(wave_child.id.ctg, wave_child.id.cno)
        .map_err(|e| format!("Cannot get WAVE data: {e}"))?;

    let (_, samples) = decode_wav(&wave_data)
        .map_err(|e| format!("WAV decode error: {e}"))?;

    // Build a minimal in-memory WAV (16-bit PCM) for rodio.
    let pcm_wav = build_pcm_wav(&samples, 22050, 1);

    let audio_guard = state.audio_tx.lock().unwrap();
    if let Some(tx) = &*audio_guard {
        tx.send(crate::state::AudioCmd::PlayWav(pcm_wav))
            .map_err(|e| format!("Audio thread error: {e}"))?;
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Encode raw RGBA8 pixels to PNG bytes.
fn encode_rgba_to_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()
            .map_err(|e| format!("PNG header: {e}"))?;
        writer.write_image_data(rgba)
            .map_err(|e| format!("PNG data: {e}"))?;
    }
    Ok(buf)
}

/// Build a minimal 16-bit PCM WAV blob from decoded samples.
///
/// The `audio::AudioPlayer::play_wav` method accepts RIFF WAV bytes.
/// We re-encode our decoded i16 samples into a minimal PCM WAV.
fn build_pcm_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align: u16 = channels * 2;
    let data_size = num_samples * 2;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity((file_size + 8) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());   // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes());    // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());   // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}
