//! WASM bindings for the 3DMM viewer/editor.
//!
//! Exposes `WasmEngine` to JavaScript via wasm-bindgen.
//! GPU rendering is async — `render_frame` returns a `Promise<Uint8Array>`.

use std::io::Cursor;

use glam::Mat4;
use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use chunky_format::cfl::ChunkyFile;
use chunky_format::collections::GenericGroup;
use engine::actor::ActorOnFile;
use engine::events::{ActorEvent, EventPayload};
use engine::model::Model;
use engine::tag::{CTG_ACTR, CTG_BMDL, CTG_GGAE, CTG_PATH, CTG_SCEN, CTG_TMPL};
use engine::transform::RoutePoint;
use renderer::convert::orient_to_rotation_mat4;
use renderer::headless::HeadlessRenderer;

// ── DTOs serialized to JS ─────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct SceneInfo {
    scene_idx: usize,
    frame_count: i32,
    actor_count: usize,
}

#[derive(Serialize, Deserialize)]
struct ActorInfo {
    actor_idx: usize,
    cno: u32,
    dx: f32,
    dy: f32,
    dz: f32,
    nfrm_first: i32,
    nfrm_last: i32,
    xa_deg: f32,
    ya_deg: f32,
    za_deg: f32,
}

// ── WasmEngine ────────────────────────────────────────────────────────────

/// The main engine object exposed to JavaScript.
///
/// Usage:
/// ```js
/// const engine = WasmEngine.open_file(bytes);
/// engine.load_content_file(tmplsBytes); // optional: enables external-template actors
/// const scenes = engine.get_scene_list();
/// const actors = engine.get_scene_actors(0);
/// const bmp = await engine.render_frame(0, 1, 640, 480); // async
/// engine.update_actor_position(0, 0, 1.0, 0.0, 0.0);
/// const bytes = engine.to_bytes();
/// ```
#[wasm_bindgen]
pub struct WasmEngine {
    cfl: ChunkyFile,
    scenes: Vec<SceneInfo>,
    /// Optional external content file (tmpls.3cn). When loaded, actors whose
    /// TMPL chunk lives in this file (sid > 0) can be rendered.
    tmpls: Option<ChunkyFile>,
    /// Lazy-initialized GPU renderer. `None` until first `render_frame` call.
    /// `None` also when WebGPU is unavailable in the browser.
    renderer: Option<HeadlessRenderer>,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Parse a .3mm file from raw bytes.
    ///
    /// Call `load_content_file` after this to supply tmpls.3cn / tdfs.3cn
    /// for external template rendering.
    #[wasm_bindgen]
    pub fn open_file(bytes: &[u8]) -> Result<WasmEngine, JsValue> {
        let mut cursor = Cursor::new(bytes);
        let cfl = ChunkyFile::read(&mut cursor)
            .map_err(|e| JsValue::from_str(&format!("Parse error: {e}")))?;

        let scenes = build_scene_list(&cfl);

        Ok(WasmEngine { cfl, scenes, tmpls: None, renderer: None })
    }

    /// Load an external content file (e.g. `tmpls.3cn`) from raw bytes.
    ///
    /// Once loaded, actors whose templates live in this file (sid > 0) will
    /// be included in subsequent `render_frame` calls.
    /// Safe to call multiple times — replaces the previously loaded file.
    #[wasm_bindgen]
    pub fn load_content_file(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let mut cursor = Cursor::new(bytes);
        let cfl = ChunkyFile::read(&mut cursor)
            .map_err(|e| JsValue::from_str(&format!("Content file parse error: {e}")))?;
        self.tmpls = Some(cfl);
        // Clear mesh cache so external models are re-uploaded on next render.
        if let Some(r) = &mut self.renderer {
            r.clear_mesh_cache();
        }
        Ok(())
    }

    /// Return an array of `{ scene_idx, frame_count, actor_count }` objects.
    #[wasm_bindgen]
    pub fn get_scene_list(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.scenes).unwrap_or(JsValue::NULL)
    }

    /// Return an array of `ActorInfo` objects for the given scene.
    #[wasm_bindgen]
    pub fn get_scene_actors(&self, scene_idx: usize) -> JsValue {
        match build_actor_list(&self.cfl, scene_idx) {
            Ok(actors) => serde_wasm_bindgen::to_value(&actors).unwrap_or(JsValue::NULL),
            Err(_) => JsValue::NULL,
        }
    }

    /// Render a scene frame, returning BMP bytes as Uint8Array.
    ///
    /// GPU renderer is initialized lazily on first call.
    /// Scenes whose actors all reference external templates (tmpls.3cn) will
    /// render as clear color only until Ph8a.3 adds external file loading.
    ///
    /// Returns `Err` if WebGPU is unavailable in the browser.
    #[wasm_bindgen]
    pub async fn render_frame(
        &mut self,
        scene_idx: usize,
        frame: i32,
        w: u32,
        h: u32,
    ) -> Result<Uint8Array, JsValue> {
        // Lazy-init renderer on first call.
        if self.renderer.is_none() {
            self.renderer = HeadlessRenderer::new_async().await;
            if self.renderer.is_none() {
                return Err(JsValue::from_str("WebGPU not available in this browser"));
            }
        }
        let r = self.renderer.as_mut().unwrap();

        // Build render entries: (key, transform). Resolves sid=0 local BMDLs
        // and, when tmpls is loaded, sid>0 external templates.
        let entries = build_scene_entries(&self.cfl, self.tmpls.as_ref(), scene_idx, frame);

        // Ensure each model is uploaded to the GPU mesh cache.
        // Try local file first, then external content file.
        for (key, _) in &entries {
            if !r.has_mesh(*key) {
                let model = load_model_from(&self.cfl, *key)
                    .or_else(|| self.tmpls.as_ref().and_then(|t| load_model_from(t, *key)));
                if let Some(m) = model {
                    r.ensure_mesh(*key, &m);
                }
            }
        }

        // Render frame.
        let rgba = r.render_scene_async(&entries, w, h).await;

        // Encode to BMP for canvas display.
        let bmp = encode_rgba_to_bmp(&rgba, w, h);
        let out = Uint8Array::new_with_length(bmp.len() as u32);
        out.copy_from(&bmp);
        Ok(out)
    }

    /// Edit an actor's position offset (dxyz_full_rte) in memory.
    ///
    /// Changes are reflected in subsequent `to_bytes()` calls.
    #[wasm_bindgen]
    pub fn update_actor_position(
        &mut self,
        scene_idx: usize,
        actor_idx: usize,
        dx: f32,
        dy: f32,
        dz: f32,
    ) -> Result<(), JsValue> {
        let actr_cno = resolve_actor_cno(&self.cfl, scene_idx, actor_idx)
            .map_err(|e| JsValue::from_str(&e))?;

        let raw = self.cfl.chunk_data
            .get(&(CTG_ACTR, actr_cno))
            .ok_or_else(|| JsValue::from_str(&format!("ACTR cno={actr_cno} not in chunk_data")))?
            .clone();

        if raw.len() < ActorOnFile::SIZE {
            return Err(JsValue::from_str("ACTR chunk too small"));
        }

        let mut new_raw = raw;
        new_raw[4..8].copy_from_slice(&((dx * 65536.0) as i32).to_le_bytes());
        new_raw[8..12].copy_from_slice(&((dy * 65536.0) as i32).to_le_bytes());
        new_raw[12..16].copy_from_slice(&((dz * 65536.0) as i32).to_le_bytes());

        self.cfl.chunk_data.insert((CTG_ACTR, actr_cno), new_raw);
        Ok(())
    }

    /// Serialize the (possibly edited) movie back to .3mm bytes for download.
    #[wasm_bindgen]
    pub fn to_bytes(&self) -> Result<Uint8Array, JsValue> {
        let bytes = self.cfl.to_bytes_passthrough()
            .map_err(|e| JsValue::from_str(&format!("Serialize error: {e}")))?;
        let out = Uint8Array::new_with_length(bytes.len() as u32);
        out.copy_from(&bytes);
        Ok(out)
    }

    /// Return the chunk count of the loaded file (sanity check / diagnostics).
    #[wasm_bindgen]
    pub fn chunk_count(&self) -> usize {
        self.cfl.chunks.len()
    }
}

// ── Free parse helper (spike proof-of-concept) ────────────────────────────

/// Parse a .3mm file from raw bytes and return chunk count.
#[wasm_bindgen]
pub fn parse_movie_info(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let mut cursor = Cursor::new(bytes);
    let cfl = ChunkyFile::read(&mut cursor)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(JsValue::from_f64(cfl.chunks.len() as f64))
}

// ── Scene entry resolver (MVP: sid=0 local BMDLs only) ───────────────────

/// Build render entries for a scene frame.
///
/// Returns `Vec<((ctg, cno), world_transform)>`. Resolves:
/// - sid=0 actors: TMPL + BMDL from `cfl` (the .3mm file itself)
/// - sid>0 actors: TMPL from `tmpls` (tmpls.3cn), BMDL also from `tmpls`
///
/// TODO Ph8a.3 follow-up: extract to engine::scene_resolver when a third caller appears.
fn build_scene_entries(
    cfl: &ChunkyFile,
    tmpls: Option<&ChunkyFile>,
    scene_idx: usize,
    frame: i32,
) -> Vec<((u32, u32), Mat4)> {
    let scen = match cfl.chunks.iter().filter(|c| c.id.ctg == CTG_SCEN).nth(scene_idx) {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut entries = Vec::new();

    for child in scen.children.iter().filter(|c| c.id.ctg == CTG_ACTR) {
        let data = match cfl.get_chunk_data(child.id.ctg, child.id.cno) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() < ActorOnFile::SIZE {
            continue;
        }
        let arr: [u8; 44] = match data[..44].try_into() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let actf = match ActorOnFile::from_bytes(&arr) {
            Ok(a) => a,
            Err(_) => continue,
        };

        // Skip null template or out-of-frame actors.
        if actf.tag_tmpl.is_null() || actf.tag_tmpl.cno == 0 {
            continue;
        }
        let has_range = actf.nfrm_last > actf.nfrm_first;
        if has_range && (frame < actf.nfrm_first || frame > actf.nfrm_last) {
            continue;
        }

        // Only TMPL actors (direct BMDL refs skipped for MVP).
        if actf.tag_tmpl.ctg != CTG_TMPL {
            continue;
        }
        let tmpl_cno = actf.tag_tmpl.cno;

        // Resolve TMPL: try local cfl first (sid=0), then external tmpls (sid>0).
        let bmdl_keys: Vec<(u32, u32)> = if let Some(tmpl) =
            cfl.chunks.iter().find(|c| c.id.ctg == CTG_TMPL && c.id.cno == tmpl_cno)
        {
            // Local TMPL — BMDL data in cfl.
            tmpl.children
                .iter()
                .filter(|ch| ch.id.ctg == CTG_BMDL)
                .map(|ch| (CTG_BMDL, ch.id.cno))
                .collect()
        } else if let Some(tmpls_cfl) = tmpls {
            // External TMPL (sid>0) — BMDL data in tmpls.3cn.
            if let Some(tmpl) =
                tmpls_cfl.chunks.iter().find(|c| c.id.ctg == CTG_TMPL && c.id.cno == tmpl_cno)
            {
                tmpl.children
                    .iter()
                    .filter(|ch| ch.id.ctg == CTG_BMDL)
                    .map(|ch| (CTG_BMDL, ch.id.cno))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![] // tmpls.3cn not loaded yet — skip external actor
        };
        if bmdl_keys.is_empty() {
            continue;
        }

        // Compute translation from PATH (first keyframe) + dxyz_full_rte.
        let translation = {
            let actr_top =
                cfl.chunks.iter().find(|c| c.id.ctg == CTG_ACTR && c.id.cno == child.id.cno);
            let path_child =
                actr_top.and_then(|a| a.children.iter().find(|ch| ch.id.ctg == CTG_PATH));
            if let Some(pc) = path_child {
                if let Ok(path_data) = cfl.get_chunk_data(pc.id.ctg, pc.id.cno) {
                    const GL_HDR: usize = 12;
                    const RPT_SIZE: usize = RoutePoint::SIZE;
                    if path_data.len() >= GL_HDR + RPT_SIZE {
                        let iv_mac =
                            i32::from_le_bytes(path_data[8..12].try_into().unwrap_or([0; 4]));
                        if iv_mac >= 1 {
                            if let Ok(rpt_bytes) =
                                path_data[GL_HDR..GL_HDR + RPT_SIZE].try_into()
                            {
                                let rpt = RoutePoint::from_le_bytes(rpt_bytes);
                                let dx = actf.dxyz_full_rte.x;
                                let dy = actf.dxyz_full_rte.y;
                                let dz = actf.dxyz_full_rte.z;
                                let x = (rpt.position.x.0 + dx.0) as f64 / 65536.0;
                                let y = (rpt.position.y.0 + dy.0) as f64 / 65536.0;
                                let z = (rpt.position.z.0 + dz.0) as f64 / 65536.0;
                                Mat4::from_translation(glam::Vec3::new(x as f32, y as f32, z as f32))
                            } else {
                                Mat4::IDENTITY
                            }
                        } else {
                            Mat4::IDENTITY
                        }
                    } else {
                        Mat4::IDENTITY
                    }
                } else {
                    Mat4::IDENTITY
                }
            } else {
                Mat4::IDENTITY
            }
        };

        // Compute rotation from GGAE orient event (last event at or before `frame`).
        let rotation = {
            let actr_top =
                cfl.chunks.iter().find(|c| c.id.ctg == CTG_ACTR && c.id.cno == child.id.cno);
            let ggae_child =
                actr_top.and_then(|a| a.children.iter().find(|ch| ch.id.ctg == CTG_GGAE));
            if let Some(gc) = ggae_child {
                if let Ok(ggae_data) = cfl.get_chunk_data(gc.id.ctg, gc.id.cno) {
                    if let Ok(gg) = GenericGroup::read(&ggae_data) {
                        let mut last_rot: Option<Mat4> = None;
                        let mut last_nfrm = i32::MIN;
                        for (fixed_bytes, var_bytes) in
                            gg.fixed_entries.iter().zip(gg.variable_entries.iter())
                        {
                            if fixed_bytes.len() < 20 {
                                continue;
                            }
                            let fixed_arr: &[u8; 20] = match fixed_bytes[..20].try_into() {
                                Ok(a) => a,
                                Err(_) => continue,
                            };
                            if let Ok(evt) = ActorEvent::parse(fixed_arr, var_bytes) {
                                if let EventPayload::Orient(op) = &evt.payload {
                                    if evt.header.nfrm <= frame && evt.header.nfrm >= last_nfrm {
                                        last_nfrm = evt.header.nfrm;
                                        last_rot = Some(orient_to_rotation_mat4(op));
                                    }
                                }
                            }
                        }
                        last_rot.unwrap_or(Mat4::IDENTITY)
                    } else {
                        Mat4::IDENTITY
                    }
                } else {
                    Mat4::IDENTITY
                }
            } else {
                Mat4::IDENTITY
            }
        };

        let transform = translation * rotation;
        for key in bmdl_keys {
            entries.push((key, transform));
        }
    }

    entries
}

/// Load a BMDL model from the given ChunkyFile.
fn load_model_from(cfl: &ChunkyFile, key: (u32, u32)) -> Option<Model> {
    let data = cfl.get_chunk_data(key.0, key.1).ok()?;
    Model::from_bytes(&data).ok()
}

// ── BMP encoder ────────────────────────────────────────────────────────────

/// Encode raw RGBA8 pixels (top-down) to a 24-bit BMP byte stream.
///
/// Copied from commands.rs — TODO Ph8a.3: extract to engine::bmp if a third
/// caller appears.
fn encode_rgba_to_bmp(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row_bytes = width as usize * 3;
    let row_padded = (row_bytes + 3) & !3; // BMP rows must be 4-byte aligned
    let pixel_data_size = row_padded * height as usize;
    let file_size = 54 + pixel_data_size;

    let mut bmp = vec![0u8; file_size];

    // BITMAPFILEHEADER (14 bytes)
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes()); // pixel data offset

    // BITMAPINFOHEADER (40 bytes)
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes()); // negative = top-down
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[34..38].copy_from_slice(&(pixel_data_size as u32).to_le_bytes());

    // Pixel data: RGBA → BGR
    let pixel_start = 54;
    for y in 0..height as usize {
        let row_dst = pixel_start + y * row_padded;
        for x in 0..width as usize {
            let src = (y * width as usize + x) * 4;
            let dst = row_dst + x * 3;
            if src + 3 < rgba.len() {
                bmp[dst] = rgba[src + 2]; // B
                bmp[dst + 1] = rgba[src + 1]; // G
                bmp[dst + 2] = rgba[src]; // R
            }
        }
    }
    bmp
}

// ── Internal helpers ──────────────────────────────────────────────────────

fn build_scene_list(cfl: &ChunkyFile) -> Vec<SceneInfo> {
    let mut scenes = Vec::new();
    for (idx, chunk) in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_SCEN).enumerate() {
        let frame_count: i32 = chunk
            .children
            .iter()
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

        let actor_count = chunk.children.iter().filter(|c| c.id.ctg == CTG_ACTR).count();
        scenes.push(SceneInfo { scene_idx: idx, frame_count, actor_count });
    }
    scenes
}

fn build_actor_list(cfl: &ChunkyFile, scene_idx: usize) -> Result<Vec<ActorInfo>, String> {
    let scen = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .nth(scene_idx)
        .ok_or_else(|| format!("Scene {scene_idx} not found"))?;

    let mut actors = Vec::new();
    for (idx, child) in scen.children.iter().filter(|c| c.id.ctg == CTG_ACTR).enumerate() {
        let cno = child.id.cno;
        let raw = match cfl.chunk_data.get(&(CTG_ACTR, cno)) {
            Some(d) => d,
            None => continue,
        };
        if raw.len() < ActorOnFile::SIZE {
            continue;
        }

        let arr: [u8; 44] = match raw[..44].try_into() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let actor = match ActorOnFile::from_bytes(&arr) {
            Ok(a) => a,
            Err(_) => continue,
        };

        let dx = actor.dxyz_full_rte.x.0 as f32 / 65536.0;
        let dy = actor.dxyz_full_rte.y.0 as f32 / 65536.0;
        let dz = actor.dxyz_full_rte.z.0 as f32 / 65536.0;

        let mut xa_deg = 0.0f32;
        let mut ya_deg = 0.0f32;
        let mut za_deg = 0.0f32;
        {
            let actr_top =
                cfl.chunks.iter().find(|c| c.id.ctg == CTG_ACTR && c.id.cno == cno);
            if let Some(actr) = actr_top {
                if let Some(ggae_ref) = actr.children.iter().find(|ch| ch.id.ctg == CTG_GGAE) {
                    if let Ok(ggae_data) = cfl.get_chunk_data(ggae_ref.id.ctg, ggae_ref.id.cno) {
                        if let Ok(gg) = GenericGroup::read(&ggae_data) {
                            let mut best_nfrm = i32::MAX;
                            for (fe, ve) in
                                gg.fixed_entries.iter().zip(gg.variable_entries.iter())
                            {
                                if fe.len() < 20 {
                                    continue;
                                }
                                let fixed_arr: &[u8; 20] = match fe[..20].try_into() {
                                    Ok(a) => a,
                                    Err(_) => continue,
                                };
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

fn resolve_actor_cno(cfl: &ChunkyFile, scene_idx: usize, actor_idx: usize) -> Result<u32, String> {
    let scen = cfl
        .chunks
        .iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .nth(scene_idx)
        .ok_or_else(|| format!("Scene {scene_idx} not found"))?;
    scen.children
        .iter()
        .filter(|c| c.id.ctg == CTG_ACTR)
        .nth(actor_idx)
        .ok_or_else(|| format!("Actor {actor_idx} not found in scene {scene_idx}"))
        .map(|c| c.id.cno)
}
