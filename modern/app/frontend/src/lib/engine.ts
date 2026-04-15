//! Typed wrappers around Tauri `invoke` calls to the Rust backend.

import { invoke } from "@tauri-apps/api/core";
import type { MovieInfo, SceneInfo, SoundEntry, ActorInfo, TemplateInfo } from "./types";

export async function openFile(path: string): Promise<MovieInfo> {
  return invoke<MovieInfo>("open_file", { path });
}

export async function getSceneList(): Promise<SceneInfo[]> {
  return invoke<SceneInfo[]>("get_scene_list");
}

export async function renderDemoFrame(
  width: number,
  height: number
): Promise<string> {
  return invoke<string>("render_demo_frame", { width, height });
}

// frame is 0-indexed in the frontend; backend expects 1-indexed (3DMM convention).
export async function renderSceneFrame(
  sceneIdx: number,
  frame: number,
  width: number,
  height: number,
): Promise<string> {
  return invoke<string>("render_scene_frame", { sceneIdx, frame: frame + 1, width, height });
}

export async function listSounds(): Promise<SoundEntry[]> {
  return invoke<SoundEntry[]>("list_sounds");
}

export async function playSound(cno: number): Promise<void> {
  return invoke<void>("play_sound", { cno });
}

// ── Phase 7a — Editor ─────────────────────────────────────────────────────

export async function getSceneActors(sceneIdx: number): Promise<ActorInfo[]> {
  return invoke<ActorInfo[]>("get_scene_actors", { sceneIdx });
}

export async function updateActorPosition(
  sceneIdx: number,
  actorIdx: number,
  dx: number,
  dy: number,
  dz: number,
): Promise<void> {
  return invoke<void>("update_actor_position", { sceneIdx, actorIdx, dx, dy, dz });
}

export async function updateActorFrameRange(
  sceneIdx: number,
  actorIdx: number,
  nfrmFirst: number,
  nfrmLast: number,
): Promise<void> {
  return invoke<void>("update_actor_frame_range", { sceneIdx, actorIdx, nfrmFirst, nfrmLast });
}

export async function updateActorOrientation(
  sceneIdx: number,
  actorIdx: number,
  xaDeg: number,
  yaDeg: number,
  zaDeg: number,
): Promise<void> {
  return invoke<void>("update_actor_orientation", { sceneIdx, actorIdx, xaDeg, yaDeg, zaDeg });
}

/** Save to original path (no arg) or a new path. Returns the saved path. */
export async function saveFile(path?: string): Promise<string> {
  return invoke<string>("save_file", { path: path ?? null });
}

// ── Phase 7c — Add/remove actors ─────────────────────────────────────────

/** List all TMPL chunks available in tmpls.3cn (content library). */
export async function listTemplates(): Promise<TemplateInfo[]> {
  return invoke<TemplateInfo[]>("list_templates");
}

/**
 * Add a new actor to a scene.
 * @param sceneIdx  Scene index (0-based)
 * @param tmplCno   TMPL chunk number from tmpls.3cn
 * @param dx/dy/dz  World-space offset
 * @returns cno of the newly created ACTR chunk
 */
export async function addActor(
  sceneIdx: number,
  tmplCno: number,
  dx: number,
  dy: number,
  dz: number,
): Promise<number> {
  return invoke<number>("add_actor", { sceneIdx, tmplCno, dx, dy, dz });
}

/** Remove an actor from a scene by its scene-local index. */
export async function removeActor(sceneIdx: number, actorIdx: number): Promise<void> {
  return invoke<void>("remove_actor", { sceneIdx, actorIdx });
}

// ── Phase 7e — Create movie from scratch ─────────────────────────────────

/** Create a new empty .3mm at `path`, load it, return MovieInfo. */
export async function createMovie(path: string): Promise<MovieInfo> {
  return invoke<MovieInfo>("create_movie", { path });
}
