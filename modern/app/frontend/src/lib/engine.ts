//! Typed wrappers around Tauri `invoke` calls to the Rust backend.

import { invoke } from "@tauri-apps/api/core";
import type { MovieInfo, SceneInfo, SoundEntry, ActorInfo } from "./types";

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

/** Save to original path (no arg) or a new path. Returns the saved path. */
export async function saveFile(path?: string): Promise<string> {
  return invoke<string>("save_file", { path: path ?? null });
}
