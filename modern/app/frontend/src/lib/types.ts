export interface MovieInfo {
  path: string;
  file_name: string;
  scene_count: number;
  total_frames: number;
}

export interface SceneInfo {
  scene_idx: number;
  frame_count: number;
  actor_count: number;
}

export interface SoundEntry {
  cno: number;
  name: string | null;
  sound_type: string;
  volume_default: number;
}

export interface ActorInfo {
  actor_idx: number;
  cno: number;
  dx: number;
  dy: number;
  dz: number;
  nfrm_first: number;
  nfrm_last: number;
}
