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
use engine::events::{ActorEvent, EventPayload, OrientPayload, aet};
use engine::fixedpoint::FixedAngle;
use engine::model::Model;
use engine::msnd::MovieSound;
use engine::tag::{CTG_ACTR, CTG_BMDL, CTG_GGAE, CTG_MSND, CTG_PATH, CTG_SCEN, CTG_TDF, CTG_TDT, CTG_TMPL, CTG_WAVE};
use engine::tdf::BrTdf;
use engine::tdt::{BrTdt, Tdts};
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

    // Find content-files/ by walking up from the movie file, then up from the
    // executable. Fan movies may live in a completely different directory tree
    // from the 3DMMEx project, so a simple 2-level heuristic is not enough.
    if let Some(content_dir) = find_content_dir(&pb) {
        *state.content_dir.lock().unwrap() = Some(content_dir.clone());
        // Pre-parse tmpls.3cn once; reused by every render_scene_frame call.
        let tmpls_path = content_dir.join("tmpls.3cn");
        if let Ok(f) = std::fs::File::open(&tmpls_path) {
            let mut r = BufReader::new(f);
            if let Ok(cf) = ChunkyFile::read(&mut r) {
                *state.tmpls.lock().unwrap() = Some(Arc::new(cf));
            }
        }
        // Pre-parse tdfs.3cn — fallback BMDL source for fan-made movies with TDT chunks.
        let tdfs_path = content_dir.join("tdfs.3cn");
        if let Ok(f) = std::fs::File::open(&tdfs_path) {
            let mut r = BufReader::new(f);
            if let Ok(cf) = ChunkyFile::read(&mut r) {
                *state.tdfs.lock().unwrap() = Some(Arc::new(cf));
            }
        }
        // Invalidate GPU mesh cache — new file means new geometry.
        let mut gpu_guard = state.gpu.lock().unwrap();
        if let Some(ref mut gpu) = *gpu_guard {
            gpu.clear_mesh_cache();
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

/// Compute per-character local transforms for a TDT text string (all shape variants).
///
/// Returns one `Mat4` per printable byte (≥ 0x20) in `stn`.
/// Each entry is the character's local transform; combine as `actor_transform * char_transform`.
///
/// `dxr` / `dyr` are indexed by ASCII byte value. Elements outside `dxr.len()` fall back to
/// `dxr[0]`. `dyr_max` is the maximum character height from the TDF header.
fn compose_layout(stn: &str, tdts: Tdts, dxr: &[f32], dyr: &[f32], dyr_max: f32) -> Vec<glam::Mat4> {
    use std::f32::consts::PI;

    let chars: Vec<u8> = stn.bytes().filter(|&b| b >= 0x20).collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let fallback_dx = dxr.first().copied().unwrap_or(1.0);
    let fallback_dy = dyr.first().copied().unwrap_or(0.0);

    let get_dx = |ch: u8| -> f32 {
        let i = ch as usize;
        if i < dxr.len() { dxr[i] } else { fallback_dx }
    };
    let get_dy = |ch: u8| -> f32 {
        let i = ch as usize;
        if i < dyr.len() { dyr[i] } else { fallback_dy }
    };

    let total_width: f32 = chars.iter().map(|&c| get_dx(c)).sum();
    let total_height: f32 = chars.iter().map(|&c| get_dy(c)).sum();
    let radius = if total_width > 0.0 { total_width / (2.0 * PI) } else { 1.0 };

    let mut result = Vec::with_capacity(chars.len());
    let mut cursor_x = 0.0f32;
    let mut cursor_y = 0.0f32;

    for &ch in &chars {
        let dx = get_dx(ch);
        // Character center x, relative to string center
        let xr = cursor_x + dx * 0.5 - total_width * 0.5;
        // Fractional position 0..1 along string width
        let xr_fract = if total_width > 0.0 { (cursor_x + dx * 0.5) / total_width } else { 0.5 };
        let quarter = total_width * 0.25;

        let mat = match tdts {
            Tdts::Normal => {
                glam::Mat4::from_translation(glam::vec3(xr, 0.0, 0.0))
            }
            Tdts::ArchPositive => {
                let y = quarter * (PI * xr_fract).sin();
                glam::Mat4::from_translation(glam::vec3(xr, y, 0.0))
            }
            Tdts::ArchNegative => {
                let y = quarter * (1.0 - (PI * xr_fract).sin());
                glam::Mat4::from_translation(glam::vec3(xr, y, 0.0))
            }
            Tdts::ArchZ => {
                let z = quarter * (PI * xr_fract).sin();
                glam::Mat4::from_translation(glam::vec3(xr, 0.0, z))
            }
            Tdts::CircleY => {
                let angle = 2.0 * PI * xr_fract;
                // Rotate Y then translate Z: each char sits at (0,0,r) rotated around Y
                glam::Mat4::from_rotation_y(angle)
                    * glam::Mat4::from_translation(glam::vec3(0.0, 0.0, radius))
            }
            Tdts::CircleZ => {
                let angle = 2.0 * PI * xr_fract;
                // Translate Y by (r + dyrMax) after rotating Z: chars form a ring in XY plane
                let extra_y = radius + dyr_max;
                glam::Mat4::from_translation(glam::vec3(0.0, extra_y, 0.0))
                    * glam::Mat4::from_rotation_z(-angle)
                    * glam::Mat4::from_translation(glam::vec3(0.0, radius, 0.0))
            }
            Tdts::LargeMiddle => {
                // Scale Y by (1 + sin(π*fract)): tallest in the middle
                let sy = 1.0 + (PI * xr_fract).sin();
                // In column-vector terms: S * T (scale around origin, then translate)
                glam::Mat4::from_scale(glam::vec3(1.0, sy, 1.0))
                    * glam::Mat4::from_translation(glam::vec3(xr, 0.0, 0.0))
            }
            Tdts::Vertical => {
                // x=0; y = dyrTotal - yrChar (chars stack top→bottom)
                let y = total_height - cursor_y;
                glam::Mat4::from_translation(glam::vec3(0.0, y, 0.0))
            }
            Tdts::GrowRight => {
                let sy = 1.0 + xr_fract;
                glam::Mat4::from_scale(glam::vec3(1.0, sy, 1.0))
                    * glam::Mat4::from_translation(glam::vec3(xr, 0.0, 0.0))
            }
            Tdts::GrowLeft => {
                let sy = 1.0 + (1.0 - xr_fract);
                glam::Mat4::from_scale(glam::vec3(1.0, sy, 1.0))
                    * glam::Mat4::from_translation(glam::vec3(xr, 0.0, 0.0))
            }
        };

        result.push(mat);
        cursor_x += dx;
        cursor_y += get_dy(ch);
    }

    result
}

/// Core render path: collect actors for a scene/frame, upload meshes, GPU render.
/// Returns raw RGBA8 pixels (width × height × 4 bytes).
/// Used by both the IPC command (for compat) and the `stream://` URI handler (hot path).
pub(crate) fn render_to_rgba(
    state: &AppState,
    scene_idx: usize,
    frame: i32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    // --- Step 1: locate SCEN chunk and extract actor tag_tmpls + transforms --
    // Each entry: (ctg, cno, transform, tdt_data)
    // tdt_data = Some((tdf_cno, tdts, stn)) for TDT actors; None for normal actors.
    // Extracted while holding the loaded_file lock so Step 2 needs no re-lock.
    let tag_tmpls: Vec<(u32, u32, glam::Mat4, Option<(u32, Tdts, String)>)> = {
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
                let has_range = actf.nfrm_last > actf.nfrm_first;
                if has_range && (frame < actf.nfrm_first || frame > actf.nfrm_last) {
                    continue;
                }

                let translation = 'path: {
                    let actr_top = lf.cfl.chunks.iter()
                        .find(|c| c.id.ctg == CTG_ACTR && c.id.cno == child.id.cno);
                    let path_child = actr_top
                        .and_then(|a| a.children.iter().find(|ch| ch.id.ctg == CTG_PATH));
                    if let Some(pc) = path_child {
                        if let Ok(path_data) = lf.cfl.get_chunk_data(pc.id.ctg, pc.id.cno) {
                            const GL_HDR: usize = 12;
                            const RPT_SIZE: usize = RoutePoint::SIZE;
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

                // For TDT (3D text) actors: extract (tdf_cno, tdts, stn) while we hold
                // the loaded_file lock so Step 2 can use them without re-locking.
                let tdt_data: Option<(u32, Tdts, String)> = if actf.tag_tmpl.ctg == CTG_TMPL {
                    let tmpl_local = lf.cfl.chunks.iter()
                        .find(|c| c.id.ctg == CTG_TMPL && c.id.cno == actf.tag_tmpl.cno);
                    tmpl_local.and_then(|tmpl| {
                        let tdt_child = tmpl.children.iter().find(|ch| ch.id.ctg == CTG_TDT)?;
                        let tdt_bytes = lf.cfl.get_chunk_data(CTG_TDT, tdt_child.id.cno).ok()?;
                        let tdt = BrTdt::from_bytes(&tdt_bytes).ok()?;
                        let stn = tmpl.name.clone().unwrap_or_default();
                        if stn.is_empty() { return None; }
                        Some((tdt.tag_tdf.cno, tdt.tdts, stn))
                    })
                } else {
                    None
                };
                tags.push((actf.tag_tmpl.ctg, actf.tag_tmpl.cno, translation * rotation, tdt_data));
            }
        }
        tags
    };

    if tag_tmpls.is_empty() {
        return Err("No actors with valid templates".to_string());
    }

    // --- Step 2: look up cached tmpls.3cn + GPU renderer ---------------------
    let tmpls = state.tmpls.lock().unwrap().clone()
        .ok_or_else(|| "tmpls.3cn not loaded".to_string())?;
    let tdfs = state.tdfs.lock().unwrap().clone();

    let mut gpu_guard = state.gpu.lock().unwrap();
    let gpu = gpu_guard.as_mut()
        .ok_or_else(|| "No GPU adapter available".to_string())?;

    let mut scene_entries: Vec<((u32, u32), glam::Mat4)> = Vec::new();
    let mut diag: Vec<String> = Vec::new();

    for (ctg, cno, transform, tdt_data) in &tag_tmpls {
        let (ctg, cno, transform) = (*ctg, *cno, *transform);

        // --- TDT (3D text) path — fan-movie actors with per-char glyph BMDLs ---
        if let Some((tdf_cno, tdts, stn)) = tdt_data {
            let tdf_cno = *tdf_cno;
            let tdts = *tdts;
            match &tdfs {
                None => {
                    diag.push(format!("TMPL:{cno} is TDT but tdfs.3cn not loaded"));
                    continue;
                }
                Some(tdfs_cfl) => {
                    // Find TDF font chunk in tdfs.3cn
                    let tdf_chunk = match tdfs_cfl.chunks.iter()
                        .find(|c| c.id.ctg == CTG_TDF && c.id.cno == tdf_cno)
                    {
                        Some(c) => c,
                        None => {
                            diag.push(format!("TMPL:{cno} TDF:{tdf_cno} not found in tdfs.3cn"));
                            continue;
                        }
                    };

                    // Parse TDF font metrics
                    let tdf = match tdfs_cfl.get_chunk_data(CTG_TDF, tdf_cno)
                        .ok()
                        .and_then(|d| BrTdf::from_bytes(&d).ok())
                    {
                        Some(t) => t,
                        None => {
                            diag.push(format!("TMPL:{cno} TDF:{tdf_cno} failed to parse"));
                            continue;
                        }
                    };

                    // Compute per-char transforms for the requested shape variant
                    let char_transforms = compose_layout(stn, tdts, &tdf.dxr, &tdf.dyr, tdf.dyr_max);
                    if char_transforms.is_empty() {
                        diag.push(format!("TMPL:{cno} stn has no printable chars"));
                        continue;
                    }

                    // Render each glyph; chars without a BMDL child are skipped silently
                    let printable: Vec<u8> = stn.bytes().filter(|&b| b >= 0x20).collect();
                    let mut any_rendered = false;
                    for (i, &ch) in printable.iter().enumerate() {
                        let glyph_child = match tdf_chunk.children.iter()
                            .find(|c| c.id.ctg == CTG_BMDL && c.chid == ch as u32)
                        {
                            Some(c) => c,
                            None => continue,
                        };

                        let bmdl_key = (CTG_BMDL, glyph_child.id.cno);
                        if !gpu.has_mesh(bmdl_key) {
                            if let Ok(data) = tdfs_cfl.get_chunk_data(CTG_BMDL, glyph_child.id.cno) {
                                if let Ok(m) = Model::from_bytes(&data) {
                                    gpu.ensure_mesh(bmdl_key, &m);
                                }
                            }
                        }
                        if gpu.has_mesh(bmdl_key) {
                            scene_entries.push((bmdl_key, transform * char_transforms[i]));
                            any_rendered = true;
                        }
                    }
                    if !any_rendered {
                        diag.push(format!("TMPL:{cno} stn=\"{stn}\" — no renderable glyphs"));
                    }
                    continue;
                }
            }
        }

        if ctg == CTG_TMPL {
            // --- primary path: look up TMPL in tmpls.3cn ---
            let tmpl_opt = tmpls.chunks.iter().find(|c| c.id.ctg == CTG_TMPL && c.id.cno == cno);
            if let Some(tmpl) = tmpl_opt {
                let bmdl_children: Vec<_> = tmpl.children.iter().filter(|ch| ch.id.ctg == CTG_BMDL).collect();
                if bmdl_children.is_empty() {
                    diag.push(format!("TMPL:{cno} has no BMDL children"));
                    continue;
                }
                let mut found_key: Option<(u32, u32)> = None;
                for ch in &bmdl_children {
                    let bmdl_key = (CTG_BMDL, ch.id.cno);
                    if gpu.has_mesh(bmdl_key) {
                        found_key = Some(bmdl_key);
                        break;
                    }
                    if let Ok(data) = tmpls.get_chunk_data(ch.id.ctg, ch.id.cno) {
                        if data.len() >= 80 {
                            if let Ok(m) = Model::from_bytes(&data) {
                                gpu.ensure_mesh(bmdl_key, &m);
                                if gpu.has_mesh(bmdl_key) {
                                    found_key = Some(bmdl_key);
                                    break;
                                }
                            }
                        }
                    }
                }
                match found_key {
                    Some(k) => scene_entries.push((k, transform)),
                    None => diag.push(format!("TMPL:{cno} — no renderable BMDL child")),
                }
                continue;
            }

            diag.push(format!("TMPL:{cno} not found in tmpls.3cn"));
        } else if ctg == CTG_BMDL {
            let bmdl_key = (CTG_BMDL, cno);
            if !gpu.has_mesh(bmdl_key) {
                if let Ok(data) = tmpls.get_chunk_data(CTG_BMDL, cno) {
                    if data.len() >= 80 {
                        if let Ok(m) = Model::from_bytes(&data) {
                            gpu.ensure_mesh(bmdl_key, &m);
                        }
                    }
                }
            }
            if gpu.has_mesh(bmdl_key) {
                scene_entries.push((bmdl_key, transform));
            } else {
                diag.push(format!("direct BMDL:{cno} not renderable"));
            }
        } else {
            let ctg_str: String = ctg.to_be_bytes().iter()
                .map(|&b: &u8| if b.is_ascii_graphic() { b as char } else { '.' })
                .collect();
            diag.push(format!("actor ctg='{}' cno={cno} — not TMPL/BMDL", ctg_str));
        }
    }

    if scene_entries.is_empty() {
        return Err(format!("No renderable model: {}",
            if diag.is_empty() { "no actors matched".into() } else { diag.join("; ") }));
    }

    // --- Step 3: GPU render --------------------------------------------------
    let rgba = gpu.render_scene(&scene_entries, width, height);
    if rgba.is_empty() {
        return Err("Render produced empty output".into());
    }
    Ok(rgba)
}

/// Render actor models for a specific scene at a given frame.
/// Returns a `data:image/png;base64,...` string. Kept for IPC compatibility;
/// the hot path during playback goes through the `stream://` URI scheme instead.
#[tauri::command]
pub fn render_scene_frame(
    scene_idx: usize,
    frame: i32,
    width: u32,
    height: u32,
    state: State<AppState>,
) -> Result<String, String> {
    let rgba = render_to_rgba(state.inner(), scene_idx, frame, width, height)?;
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

// ── Phase 7a — Editor commands ────────────────────────────────────────────

/// Actor metadata returned to the frontend editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    /// Scene-local index (position in SCEN's ACTR children list)
    pub actor_idx: usize,
    /// Chunk number in the .3mm file
    pub cno: u32,
    /// Position offset X (world-space, float)
    pub dx: f32,
    /// Position offset Y (world-space, float)
    pub dy: f32,
    /// Position offset Z (world-space, float)
    pub dz: f32,
    pub nfrm_first: i32,
    pub nfrm_last: i32,
    /// Initial orientation — pitch in degrees (BRA → 0-360)
    pub xa_deg: f32,
    /// Initial orientation — yaw in degrees
    pub ya_deg: f32,
    /// Initial orientation — roll in degrees
    pub za_deg: f32,
}

/// Return the list of actors in a scene with their current positions.
#[tauri::command]
pub fn get_scene_actors(
    scene_idx: usize,
    state: State<AppState>,
) -> Result<Vec<ActorInfo>, String> {
    let guard = state.loaded_file.lock().unwrap();
    let lf = guard.as_ref().ok_or("No file loaded")?;

    let scen = lf.cfl.chunks.iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .nth(scene_idx)
        .ok_or_else(|| format!("Scene {scene_idx} not found"))?;

    let mut actors = Vec::new();
    for (idx, child) in scen.children.iter().filter(|c| c.id.ctg == CTG_ACTR).enumerate() {
        let cno = child.id.cno;
        let raw = match lf.cfl.chunk_data.get(&(CTG_ACTR, cno)) {
            Some(d) => d,
            None => continue,
        };
        if raw.len() < ActorOnFile::SIZE { continue; }

        let arr: [u8; 44] = raw[..44].try_into().unwrap();
        let actor = match ActorOnFile::from_bytes(&arr) {
            Ok(a) => a,
            Err(_) => continue,
        };

        // BRS 16.16 → f32
        let dx = actor.dxyz_full_rte.x.0 as f32 / 65536.0;
        let dy = actor.dxyz_full_rte.y.0 as f32 / 65536.0;
        let dz = actor.dxyz_full_rte.z.0 as f32 / 65536.0;

        // Read initial orientation from the GGAE Orient event with the smallest nfrm.
        let mut xa_deg = 0.0f32;
        let mut ya_deg = 0.0f32;
        let mut za_deg = 0.0f32;
        {
            let actr_top = lf.cfl.chunks.iter()
                .find(|c| c.id.ctg == CTG_ACTR && c.id.cno == cno);
            if let Some(actr) = actr_top {
                if let Some(ggae_ref) = actr.children.iter().find(|ch| ch.id.ctg == CTG_GGAE) {
                    if let Ok(ggae_data) = lf.cfl.get_chunk_data(ggae_ref.id.ctg, ggae_ref.id.cno) {
                        if let Ok(gg) = GenericGroup::read(&ggae_data) {
                            let mut best_nfrm = i32::MAX;
                            for (fe, ve) in gg.fixed_entries.iter().zip(gg.variable_entries.iter()) {
                                if fe.len() < 20 { continue; }
                                let fixed_arr: &[u8; 20] = fe[..20].try_into().unwrap();
                                if let Ok(evt) = ActorEvent::parse(fixed_arr, ve) {
                                    if let EventPayload::Orient(op) = &evt.payload {
                                        if evt.header.nfrm < best_nfrm {
                                            best_nfrm = evt.header.nfrm;
                                            xa_deg = op.xa.to_degrees() as f32;
                                            ya_deg = op.ya.to_degrees() as f32;
                                            za_deg = op.za.to_degrees() as f32;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        actors.push(ActorInfo {
            actor_idx: idx,
            cno,
            dx,
            dy,
            dz,
            nfrm_first: actor.nfrm_first,
            nfrm_last: actor.nfrm_last,
            xa_deg,
            ya_deg,
            za_deg,
        });
    }

    Ok(actors)
}

/// Edit an actor's position offset (dxyz_full_rte) in memory.
///
/// Changes are reflected immediately in subsequent stream:// renders.
/// Call `save_file` to persist to disk.
#[tauri::command]
pub fn update_actor_position(
    scene_idx: usize,
    actor_idx: usize,
    dx: f32,
    dy: f32,
    dz: f32,
    state: State<AppState>,
) -> Result<(), String> {
    let mut guard = state.loaded_file.lock().unwrap();
    let lf = guard.as_mut().ok_or("No file loaded")?;

    // Resolve (scene_idx, actor_idx) → ACTR cno
    let actr_cno = {
        let scen = lf.cfl.chunks.iter()
            .filter(|c| c.id.ctg == CTG_SCEN)
            .nth(scene_idx)
            .ok_or_else(|| format!("Scene {scene_idx} not found"))?;
        scen.children.iter()
            .filter(|c| c.id.ctg == CTG_ACTR)
            .nth(actor_idx)
            .ok_or_else(|| format!("Actor {actor_idx} not found in scene {scene_idx}"))?
            .id.cno
    };

    let raw = lf.cfl.chunk_data
        .get(&(CTG_ACTR, actr_cno))
        .ok_or_else(|| format!("ACTR chunk cno={actr_cno} not in chunk_data"))?
        .clone();

    if raw.len() < ActorOnFile::SIZE {
        return Err(format!("ACTR chunk cno={actr_cno} too small: {} bytes", raw.len()));
    }

    // Write dxyz_full_rte directly as BRS 16.16 fixed-point i32 at offsets 4-15
    let mut new_raw = raw;
    let x_fixed = (dx * 65536.0) as i32;
    let y_fixed = (dy * 65536.0) as i32;
    let z_fixed = (dz * 65536.0) as i32;
    new_raw[4..8].copy_from_slice(&x_fixed.to_le_bytes());
    new_raw[8..12].copy_from_slice(&y_fixed.to_le_bytes());
    new_raw[12..16].copy_from_slice(&z_fixed.to_le_bytes());

    lf.cfl.chunk_data.insert((CTG_ACTR, actr_cno), new_raw);
    Ok(())
}

/// Edit an actor's frame range (nfrm_first / nfrm_last) in memory.
///
/// Changes are reflected immediately in subsequent stream:// renders.
/// Call `save_file` to persist to disk.
#[tauri::command]
pub fn update_actor_frame_range(
    scene_idx: usize,
    actor_idx: usize,
    nfrm_first: i32,
    nfrm_last: i32,
    state: State<AppState>,
) -> Result<(), String> {
    let mut guard = state.loaded_file.lock().unwrap();
    let lf = guard.as_mut().ok_or("No file loaded")?;

    let actr_cno = {
        let scen = lf.cfl.chunks.iter()
            .filter(|c| c.id.ctg == CTG_SCEN)
            .nth(scene_idx)
            .ok_or_else(|| format!("Scene {scene_idx} not found"))?;
        scen.children.iter()
            .filter(|c| c.id.ctg == CTG_ACTR)
            .nth(actor_idx)
            .ok_or_else(|| format!("Actor {actor_idx} not found in scene {scene_idx}"))?
            .id.cno
    };

    let raw = lf.cfl.chunk_data
        .get(&(CTG_ACTR, actr_cno))
        .ok_or_else(|| format!("ACTR chunk cno={actr_cno} not in chunk_data"))?
        .clone();

    if raw.len() < ActorOnFile::SIZE {
        return Err(format!("ACTR chunk cno={actr_cno} too small: {} bytes", raw.len()));
    }

    // nfrm_first=[20..24], nfrm_last=[24..28]
    let mut new_raw = raw;
    new_raw[20..24].copy_from_slice(&nfrm_first.to_le_bytes());
    new_raw[24..28].copy_from_slice(&nfrm_last.to_le_bytes());
    lf.cfl.chunk_data.insert((CTG_ACTR, actr_cno), new_raw);
    Ok(())
}

/// Edit an actor's initial orientation (xa/ya/za in degrees) in memory.
///
/// Modifies the GGAE Orient event at nfrm=0; inserts one if absent.
/// Returns an error if the actor has no GGAE chunk.
#[tauri::command]
pub fn update_actor_orientation(
    scene_idx: usize,
    actor_idx: usize,
    xa_deg: f32,
    ya_deg: f32,
    za_deg: f32,
    state: State<AppState>,
) -> Result<(), String> {
    let mut guard = state.loaded_file.lock().unwrap();
    let lf = guard.as_mut().ok_or("No file loaded")?;

    // Resolve (scene_idx, actor_idx) → ACTR cno
    let actr_cno = {
        let scen = lf.cfl.chunks.iter()
            .filter(|c| c.id.ctg == CTG_SCEN)
            .nth(scene_idx)
            .ok_or_else(|| format!("Scene {scene_idx} not found"))?;
        scen.children.iter()
            .filter(|c| c.id.ctg == CTG_ACTR)
            .nth(actor_idx)
            .ok_or_else(|| format!("Actor {actor_idx} not found in scene {scene_idx}"))?
            .id.cno
    };

    // Find GGAE child cno
    let ggae_cno = {
        let actr_top = lf.cfl.chunks.iter()
            .find(|c| c.id.ctg == CTG_ACTR && c.id.cno == actr_cno)
            .ok_or_else(|| format!("ACTR top-level chunk cno={actr_cno} not found"))?;
        actr_top.children.iter()
            .find(|ch| ch.id.ctg == CTG_GGAE)
            .ok_or_else(|| format!("Actor {actor_idx} has no GGAE chunk; cannot set orientation"))?
            .id.cno
    };

    let ggae_raw = lf.cfl.chunk_data
        .get(&(CTG_GGAE, ggae_cno))
        .ok_or_else(|| format!("GGAE chunk cno={ggae_cno} not in chunk_data"))?
        .clone();

    let mut gg = GenericGroup::read(&ggae_raw)
        .map_err(|e| format!("Parse GGAE: {e}"))?;

    let new_orient = OrientPayload {
        xa: FixedAngle::from_degrees(xa_deg as f64),
        ya: FixedAngle::from_degrees(ya_deg as f64),
        za: FixedAngle::from_degrees(za_deg as f64),
    };

    // Find existing Orient event at nfrm=0
    let orient_idx = gg.fixed_entries.iter().position(|fe| {
        if fe.len() < 8 { return false; }
        let aet_val = i32::from_le_bytes(fe[0..4].try_into().unwrap());
        let nfrm   = i32::from_le_bytes(fe[4..8].try_into().unwrap());
        aet_val == aet::ORIENT && nfrm == 0
    });

    if let Some(idx) = orient_idx {
        gg.variable_entries[idx] = new_orient.to_le_bytes().to_vec();
    } else {
        // Insert new Orient event: 20-byte fixed header (aet=ORIENT, nfrm=0, rtel=zeros)
        let fixed_size = gg.fixed_size as usize;
        let mut fixed = vec![0u8; fixed_size];
        if fixed_size >= 4 {
            fixed[0..4].copy_from_slice(&aet::ORIENT.to_le_bytes());
        }
        gg.fixed_entries.push(fixed);
        gg.variable_entries.push(new_orient.to_le_bytes().to_vec());
    }

    lf.cfl.chunk_data.insert((CTG_GGAE, ggae_cno), gg.write());
    Ok(())
}

/// Serialize the loaded file to disk (overwrite original path, or a new path).
///
/// Returns the path that was written.
#[tauri::command]
pub fn save_file(path: Option<String>, state: State<AppState>) -> Result<String, String> {
    let guard = state.loaded_file.lock().unwrap();
    let lf = guard.as_ref().ok_or("No file loaded")?;

    let save_path = match &path {
        Some(p) => std::path::PathBuf::from(p),
        None => lf.path.clone(),
    };

    let bytes = lf.cfl.to_bytes_passthrough()
        .map_err(|e| format!("Serialize error: {e}"))?;

    std::fs::write(&save_path, &bytes)
        .map_err(|e| format!("Write failed: {e}"))?;

    Ok(save_path.to_string_lossy().into_owned())
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Encode raw RGBA8 pixels to 24-bit BMP bytes (no compression).
/// Uses negative biHeight so pixel rows are top-to-bottom (matches wgpu output).
/// For 640×480 this takes ~1ms vs ~37ms for PNG — eliminates the encode bottleneck.
pub(crate) fn encode_rgba_to_bmp(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row_bytes = width as usize * 3;
    let row_padded = (row_bytes + 3) & !3; // BMP rows must be 4-byte aligned
    let pixel_data_size = row_padded * height as usize;
    let file_size = 54 + pixel_data_size;

    let mut bmp = vec![0u8; file_size];

    // BITMAPFILEHEADER (14 bytes)
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
    // bytes 6-9: reserved = 0
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes()); // pixel data offset

    // BITMAPINFOHEADER (40 bytes)
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());   // biSize
    bmp[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes()); // negative = top-down
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());    // biPlanes
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());   // biBitCount
    // bytes 30-53: compression=0, sizeImage, DPI, clrUsed, clrImportant = 0
    bmp[34..38].copy_from_slice(&(pixel_data_size as u32).to_le_bytes());

    // Pixel data: RGBA → BGR, with row padding
    let pixel_start = 54;
    for y in 0..height as usize {
        let row_dst = pixel_start + y * row_padded;
        for x in 0..width as usize {
            let src = (y * width as usize + x) * 4;
            let dst = row_dst + x * 3;
            bmp[dst]     = rgba[src + 2]; // B
            bmp[dst + 1] = rgba[src + 1]; // G
            bmp[dst + 2] = rgba[src];     // R
        }
    }
    bmp
}

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

/// Find the content-files/ directory by walking up the directory tree.
///
/// Searches three roots in order:
/// 1. Walk up from the .3mm file's location.
/// 2. Walk up from the current working directory (Tauri dev sets cwd to src-tauri/).
/// 3. Walk up from the current executable's location.
///
/// This handles fan movies stored in a directory tree unrelated to the
/// 3DMMEx project (e.g. /Users/Shared/exports/) — the cwd or executable lives
/// inside the project tree where content-files/ is easily found.
fn find_content_dir(movie_path: &std::path::Path) -> Option<PathBuf> {
    let roots: Vec<PathBuf> = {
        let mut v = Vec::new();
        // Root 1: directory containing the movie file
        if let Some(p) = movie_path.parent() { v.push(p.to_path_buf()); }
        // Root 2: current working directory
        if let Ok(cwd) = std::env::current_dir() { v.push(cwd); }
        // Root 3: directory containing the executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(p) = exe.parent() { v.push(p.to_path_buf()); }
        }
        v
    };

    for root in roots {
        let mut dir: Option<&std::path::Path> = Some(root.as_path());
        while let Some(d) = dir {
            let candidate = d.join("content-files");
            if candidate.is_dir() {
                return Some(candidate);
            }
            dir = d.parent();
        }
    }
    None
}
