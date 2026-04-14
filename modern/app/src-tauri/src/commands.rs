//! Tauri IPC commands.
//!
//! All commands return `Result<T, String>` so errors surface cleanly
//! in the frontend JavaScript.

use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::State;

use audio::decode_wav;
use chunky_format::collections::GenericGroup;
use chunky_format::ChunkyFile;
use engine::actor::ActorOnFile;
use engine::events::{ActorEvent, EventPayload};
use engine::model::Model;
use engine::msnd::MovieSound;
use engine::tag::{CTG_ACTR, CTG_BMDL, CTG_GGAE, CTG_MSND, CTG_PATH, CTG_SCEN, CTG_TMPL, CTG_WAVE};
use engine::transform::RoutePoint;
use renderer::convert::orient_to_rotation_mat4;

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
            *state.content_dir.lock().unwrap() = Some(candidate.clone());
            // Pre-parse tmpls.3cn once; reused by every render_scene_frame call.
            let tmpls_path = candidate.join("tmpls.3cn");
            if let Ok(f) = std::fs::File::open(&tmpls_path) {
                let mut r = BufReader::new(f);
                if let Ok(cf) = ChunkyFile::read(&mut r) {
                    *state.tmpls.lock().unwrap() = Some(Arc::new(cf));
                }
            }
        }
    }

    // Count SCEN chunks and parse scene headers.
    let scen_entries: Vec<_> = cfl.chunks.iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .collect();

    let mut scenes: Vec<SceneInfo> = Vec::new();
    let mut total_frames: i32 = 0;

    for (idx, chunk) in scen_entries.iter().enumerate() {
        // Real scene duration = max(nfrm_last) across all actors.
        // SceneHeader.nfrm_mac is NOT the frame count — it stores the
        // highest scene-level event frame, which is often 1 or 0.
        let frame_count: i32 = chunk.children.iter()
            .filter(|c| c.id.ctg == CTG_ACTR)
            .filter_map(|c| cfl.get_chunk_data(c.id.ctg, c.id.cno).ok())
            .filter(|d| d.len() >= ActorOnFile::SIZE)
            .filter_map(|d| {
                let arr: [u8; 44] = d[..44].try_into().ok()?;
                ActorOnFile::from_bytes(&arr).ok()
            })
            .map(|a| a.nfrm_last)
            .max()
            .unwrap_or(0);

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
    let tmpls = state.tmpls.lock().unwrap().clone()
        .ok_or_else(|| "tmpls.3cn not loaded (open a .3mm file first)".to_string())?;

    let model = tmpls.chunks.iter()
        .filter(|c| c.id.ctg == CTG_BMDL)
        .find_map(|c| {
            let data = tmpls.get_chunk_data(c.id.ctg, c.id.cno).ok()?;
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

/// Render actor models for a specific scene at a given frame.
///
/// `frame` is 1-indexed (matches 3DMM's internal nfrm_first/nfrm_last).
/// Actors whose active range [nfrm_first, nfrm_last] does not include
/// `frame` are omitted from the render.
#[tauri::command]
pub fn render_scene_frame(
    scene_idx: usize,
    frame: i32,
    width: u32,
    height: u32,
    state: State<AppState>,
) -> Result<String, String> {
    // --- Step 1: locate SCEN chunk and extract actor tag_tmpls + transforms --
    // Each entry: (tmpl_ctg, tmpl_cno, world_transform)
    // world_transform = translation by (route[0].position + dxyz_full_rte).
    let tag_tmpls: Vec<(u32, u32, glam::Mat4)> = {
        let guard = state.loaded_file.lock().unwrap();
        let lf = guard.as_ref().ok_or("No file loaded")?;

        let scen_chunk = lf.cfl.chunks.iter()
            .filter(|c| c.id.ctg == CTG_SCEN)
            .nth(scene_idx)
            .ok_or_else(|| format!("Scene index {scene_idx} out of range"))?;

        let mut tags = Vec::new();
        for child in scen_chunk.children.iter().filter(|c| c.id.ctg == CTG_ACTR) {
            let data = match lf.cfl.get_chunk_data(child.id.ctg, child.id.cno) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if data.len() < ActorOnFile::SIZE { continue; }
            let arr: [u8; 44] = data[..44].try_into().unwrap();
            let actf = match ActorOnFile::from_bytes(&arr) {
                Ok(a) => a,
                Err(_) => continue,
            };
            if !actf.tag_tmpl.is_null() && actf.tag_tmpl.cno != 0 {
                // Frame filter: frames are 1-indexed in 3DMM.
                // Skip actors whose active range doesn't include this frame.
                let has_range = actf.nfrm_last > actf.nfrm_first;
                if has_range && (frame < actf.nfrm_first || frame > actf.nfrm_last) {
                    continue;
                }

                // Parse PATH GL sub-chunk to get route[0] world position.
                // GL header: [bo:i16][osk:i16][cbEntry:i32=16][ivMac:i32] + RoutePoint×ivMac
                let translation = 'path: {
                    // Find PATH child of this ACTR chunk within the main .3mm file.
                    // The ACTR chunk in cfl.chunks has children; find one with CTG_PATH.
                    let actr_top = lf.cfl.chunks.iter()
                        .find(|c| c.id.ctg == CTG_ACTR && c.id.cno == child.id.cno);
                    let path_child = actr_top
                        .and_then(|a| a.children.iter().find(|ch| ch.id.ctg == CTG_PATH));
                    if let Some(pc) = path_child {
                        if let Ok(path_data) = lf.cfl.get_chunk_data(pc.id.ctg, pc.id.cno) {
                            const GL_HDR: usize = 12;
                            const RPT_SIZE: usize = RoutePoint::SIZE; // 16
                            if path_data.len() >= GL_HDR + RPT_SIZE {
                                let iv_mac = i32::from_le_bytes(
                                    path_data[8..12].try_into().unwrap()
                                );
                                if iv_mac >= 1 {
                                    let rpt_bytes: &[u8; 16] = path_data[GL_HDR..GL_HDR + RPT_SIZE]
                                        .try_into().unwrap();
                                    let rpt = RoutePoint::from_le_bytes(rpt_bytes);
                                    let dx = actf.dxyz_full_rte.x;
                                    let dy = actf.dxyz_full_rte.y;
                                    let dz = actf.dxyz_full_rte.z;
                                    let x = (rpt.position.x.0 + dx.0) as f64 / 65536.0;
                                    let y = (rpt.position.y.0 + dy.0) as f64 / 65536.0;
                                    let z = (rpt.position.z.0 + dz.0) as f64 / 65536.0;
                                    break 'path glam::Mat4::from_translation(
                                        glam::Vec3::new(x as f32, y as f32, z as f32)
                                    );
                                }
                            }
                        }
                    }
                    glam::Mat4::IDENTITY
                };

                // Parse GGAE GG sub-chunk to get actor orientation at this frame.
                // Orientation = last AEV_ORIENT event where nfrm <= frame (keyframe semantics).
                let rotation = 'ggae: {
                    let actr_top = lf.cfl.chunks.iter()
                        .find(|c| c.id.ctg == CTG_ACTR && c.id.cno == child.id.cno);
                    let ggae_child = actr_top
                        .and_then(|a| a.children.iter().find(|ch| ch.id.ctg == CTG_GGAE));
                    if let Some(gc) = ggae_child {
                        if let Ok(ggae_data) = lf.cfl.get_chunk_data(gc.id.ctg, gc.id.cno) {
                            if let Ok(gg) = GenericGroup::read(&ggae_data) {
                                let mut last_rot: Option<glam::Mat4> = None;
                                let mut last_nfrm = i32::MIN;
                                for (fixed_bytes, var_bytes) in gg.fixed_entries.iter()
                                    .zip(gg.variable_entries.iter())
                                {
                                    if fixed_bytes.len() < 20 { continue; }
                                    let fixed_arr: &[u8; 20] = fixed_bytes[..20].try_into().unwrap();
                                    if let Ok(evt) = ActorEvent::parse(fixed_arr, var_bytes) {
                                        if let EventPayload::Orient(op) = &evt.payload {
                                            if evt.header.nfrm <= frame && evt.header.nfrm >= last_nfrm {
                                                last_nfrm = evt.header.nfrm;
                                                last_rot = Some(orient_to_rotation_mat4(op));
                                            }
                                        }
                                    }
                                }
                                if let Some(rot) = last_rot {
                                    break 'ggae rot;
                                }
                            }
                        }
                    }
                    glam::Mat4::IDENTITY
                };

                let transform = translation * rotation;
                tags.push((actf.tag_tmpl.ctg, actf.tag_tmpl.cno, transform));
            }
        }
        tags
    };

    if tag_tmpls.is_empty() {
        return Err(format!("No actors with valid templates"));
    }

    // --- Step 2: look up cached tmpls.3cn (parsed once at file open) --------
    let tmpls = state.tmpls.lock().unwrap().clone()
        .ok_or_else(|| "tmpls.3cn not loaded".to_string())?;

    // Collect all renderable models for the scene (one per actor with a valid TMPL).
    let mut models: Vec<(Model, glam::Mat4)> = Vec::new();
    let mut diag: Vec<String> = Vec::new();

    for &(ctg, cno, transform) in &tag_tmpls {
        let ctg_str: String = ctg.to_be_bytes().iter()
            .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
            .collect();
        if ctg == CTG_TMPL {
            let tmpl_opt = tmpls.chunks.iter().find(|c| c.id.ctg == CTG_TMPL && c.id.cno == cno);
            let tmpl = match tmpl_opt {
                Some(t) => t,
                None => { diag.push(format!("TMPL:{cno} not in tmpls.3cn")); continue; }
            };
            let bmdl_children: Vec<_> = tmpl.children.iter().filter(|ch| ch.id.ctg == CTG_BMDL).collect();
            if bmdl_children.is_empty() {
                diag.push(format!("TMPL:{cno} has no BMDL children"));
                continue;
            }
            // Take first BMDL child with valid geometry.
            let mut found = false;
            for ch in &bmdl_children {
                match tmpls.get_chunk_data(ch.id.ctg, ch.id.cno) {
                    Err(e) => { diag.push(format!("BMDL:{} err: {e:?}", ch.id.cno)); }
                    Ok(data) => {
                        if data.len() < 80 { continue; }
                        if let Ok(m) = Model::from_bytes(&data) {
                            if m.has_valid_faces() && !m.vertices.is_empty() {
                                models.push((m, transform));
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }
            if !found {
                diag.push(format!("TMPL:{cno} — no renderable BMDL child"));
            }
        } else if ctg == CTG_BMDL {
            match tmpls.get_chunk_data(CTG_BMDL, cno) {
                Err(e) => { diag.push(format!("direct BMDL:{cno} err: {e:?}")); }
                Ok(data) => {
                    if data.len() >= 80 {
                        if let Ok(m) = Model::from_bytes(&data) {
                            if m.has_valid_faces() && !m.vertices.is_empty() {
                                models.push((m, transform));
                            }
                        }
                    }
                }
            }
        } else {
            diag.push(format!("actor ctg='{}' cno={cno} — not TMPL/BMDL", ctg_str));
        }
    }

    if models.is_empty() {
        return Err(format!("No renderable model: {}",
            if diag.is_empty() { "no actors matched".into() } else { diag.join("; ") }));
    }

    // --- Step 3: GPU render all models → PNG data URL ---------------------
    let gpu_guard = state.gpu.lock().unwrap();
    let gpu = gpu_guard.as_ref()
        .ok_or_else(|| "No GPU adapter available".to_string())?;

    let model_refs: Vec<(&Model, glam::Mat4)> = models.iter().map(|(m, t)| (m, *t)).collect();
    let rgba = gpu.render_models(&model_refs, width, height);
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
