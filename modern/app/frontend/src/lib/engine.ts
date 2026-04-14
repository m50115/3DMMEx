//! Typed wrappers around Tauri `invoke` calls to the Rust backend.

import { invoke } from "@tauri-apps/api/core";
import type { MovieInfo, SceneInfo, SoundEntry } from "./types";

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

export async function listSounds(): Promise<SoundEntry[]> {
  return invoke<SoundEntry[]>("list_sounds");
}

export async function playSound(cno: number): Promise<void> {
  return invoke<void>("play_sound", { cno });
}
