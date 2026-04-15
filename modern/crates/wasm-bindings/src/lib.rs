//! WASM bindings for the 3DMM viewer/editor.
//!
//! Exposes `WasmEngine` to JavaScript via wasm-bindgen.
//! GPU rendering requires async init (see `render_frame` docs).

use std::io::Cursor;

use wasm_bindgen::prelude::*;
use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};

use chunky_format::cfl::ChunkyFile;
use chunky_format::collections::GenericGroup;
use engine::actor::ActorOnFile;
use engine::events::{ActorEvent, EventPayload};
use engine::tag::{CTG_ACTR, CTG_GGAE, CTG_SCEN};

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
/// const scenes = engine.get_scene_list();
/// const actors = engine.get_scene_actors(0);
/// const bmp = engine.render_frame(0, 1, 640, 480);
/// engine.update_actor_position(0, 0, 1.0, 0.0, 0.0);
/// const bytes = engine.to_bytes();
/// ```
#[wasm_bindgen]
pub struct WasmEngine {
    cfl: ChunkyFile,
    scenes: Vec<SceneInfo>,
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

        Ok(WasmEngine { cfl, scenes })
    }

    /// Return an array of `{ scene_idx, frame_count, actor_count }` objects.
    #[wasm_bindgen]
    pub fn get_scene_list(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.scenes)
            .unwrap_or(JsValue::NULL)
    }

    /// Return an array of `ActorInfo` objects for the given scene.
    #[wasm_bindgen]
    pub fn get_scene_actors(&self, scene_idx: usize) -> JsValue {
        match build_actor_list(&self.cfl, scene_idx) {
            Ok(actors) => serde_wasm_bindgen::to_value(&actors).unwrap_or(JsValue::NULL),
            Err(_) => JsValue::NULL,
        }
    }

    /// Render a scene frame, returning raw BMP bytes as Uint8Array.
    ///
    /// **Current limitation**: GPU rendering via wgpu requires async init that
    /// is not yet wired up for WASM. Returns a dark-gray placeholder BMP.
    ///
    /// TODO Ph8a.2: async GPU init via wasm_bindgen_futures; readback via
    /// Promise<Uint8Array> instead of sync Vec<u8>.
    #[wasm_bindgen]
    pub fn render_frame(&self, _scene_idx: usize, _frame: i32, w: u32, h: u32) -> Uint8Array {
        let bmp = encode_gray_bmp(w, h, 30, 30, 30);
        let out = Uint8Array::new_with_length(bmp.len() as u32);
        out.copy_from(&bmp);
        out
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
/// Spike gate: proves wasm-pack build succeeds end-to-end.
#[wasm_bindgen]
pub fn parse_movie_info(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let mut cursor = Cursor::new(bytes);
    let cfl = ChunkyFile::read(&mut cursor)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(JsValue::from_f64(cfl.chunks.len() as f64))
}

// ── Internal helpers ──────────────────────────────────────────────────────

fn build_scene_list(cfl: &ChunkyFile) -> Vec<SceneInfo> {
    let mut scenes = Vec::new();
    for (idx, chunk) in cfl.chunks.iter().filter(|c| c.id.ctg == CTG_SCEN).enumerate() {
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

        let actor_count = chunk.children.iter().filter(|c| c.id.ctg == CTG_ACTR).count();
        scenes.push(SceneInfo { scene_idx: idx, frame_count, actor_count });
    }
    scenes
}

fn build_actor_list(cfl: &ChunkyFile, scene_idx: usize) -> Result<Vec<ActorInfo>, String> {
    let scen = cfl.chunks.iter()
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
        if raw.len() < ActorOnFile::SIZE { continue; }

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
            let actr_top = cfl.chunks.iter().find(|c| c.id.ctg == CTG_ACTR && c.id.cno == cno);
            if let Some(actr) = actr_top {
                if let Some(ggae_ref) = actr.children.iter().find(|ch| ch.id.ctg == CTG_GGAE) {
                    if let Ok(ggae_data) = cfl.get_chunk_data(ggae_ref.id.ctg, ggae_ref.id.cno) {
                        if let Ok(gg) = GenericGroup::read(&ggae_data) {
                            let mut best_nfrm = i32::MAX;
                            for (fe, ve) in gg.fixed_entries.iter().zip(gg.variable_entries.iter()) {
                                if fe.len() < 20 { continue; }
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
    let scen = cfl.chunks.iter()
        .filter(|c| c.id.ctg == CTG_SCEN)
        .nth(scene_idx)
        .ok_or_else(|| format!("Scene {scene_idx} not found"))?;
    scen.children.iter()
        .filter(|c| c.id.ctg == CTG_ACTR)
        .nth(actor_idx)
        .ok_or_else(|| format!("Actor {actor_idx} not found in scene {scene_idx}"))
        .map(|c| c.id.cno)
}

/// Encode a solid-color 24-bit BMP.
fn encode_gray_bmp(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let row_size = ((width * 3 + 3) / 4) * 4;
    let pixel_data_size = row_size * height;
    let file_size = 54 + pixel_data_size;

    let mut bmp = Vec::with_capacity(file_size as usize);

    // BMP file header (14 bytes)
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes()); // reserved
    bmp.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset

    // DIB header — BITMAPINFOHEADER (40 bytes)
    bmp.extend_from_slice(&40u32.to_le_bytes());  // header size
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&(height as i32).to_le_bytes()); // positive = bottom-up
    bmp.extend_from_slice(&1u16.to_le_bytes());   // color planes
    bmp.extend_from_slice(&24u16.to_le_bytes());  // bits per pixel
    bmp.extend_from_slice(&0u32.to_le_bytes());   // compression = none
    bmp.extend_from_slice(&pixel_data_size.to_le_bytes());
    bmp.extend_from_slice(&2835u32.to_le_bytes()); // H pixels/meter
    bmp.extend_from_slice(&2835u32.to_le_bytes()); // V pixels/meter
    bmp.extend_from_slice(&0u32.to_le_bytes());   // colors in table
    bmp.extend_from_slice(&0u32.to_le_bytes());   // important colors

    // Pixel data — BMP stores BGR, bottom row first
    for _ in 0..height {
        let mut row = Vec::with_capacity(row_size as usize);
        for _ in 0..width {
            row.push(b); // B
            row.push(g); // G
            row.push(r); // R
        }
        // Pad row to multiple of 4 bytes
        while row.len() < row_size as usize {
            row.push(0);
        }
        bmp.extend_from_slice(&row);
    }

    bmp
}
