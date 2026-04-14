import { useState, useCallback, useEffect, useRef } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { openFile, getSceneList, renderSceneFrame, listSounds, playSound } from "../lib/engine";
import type { MovieInfo, SceneInfo, SoundEntry } from "../lib/types";
import { Timeline } from "./Timeline";
import { Viewport } from "./Viewport";

export function Studio() {
  const [movie, setMovie] = useState<MovieInfo | null>(null);
  const [scenes, setScenes] = useState<SceneInfo[]>([]);
  const [activeScene, setActiveScene] = useState(0);
  const [frameDataUrl, setFrameDataUrl] = useState<string | null>(null);
  const [frameLoading, setFrameLoading] = useState(false);
  const [frameError, setFrameError] = useState<string | null>(null);
  const [sounds, setSounds] = useState<SoundEntry[]>([]);
  const [statusMsg, setStatusMsg] = useState("Ready");
  const [currentFrame, setCurrentFrame] = useState(0);
  const [playing, setPlaying] = useState(false);

  // Playback loop — recursive setTimeout so frames don't queue when render is slow.
  const playingRef = useRef(playing);
  useEffect(() => { playingRef.current = playing; }, [playing]);

  useEffect(() => {
    if (!playing) return;
    const scene = scenes[activeScene];
    if (!scene || scene.frame_count <= 1) {
      setPlaying(false);
      return;
    }

    let active = true;
    let f = currentFrame;

    (async function loop() {
      while (active && playingRef.current) {
        f += 1;
        if (f >= scene.frame_count) {
          if (active) { setCurrentFrame(scene.frame_count - 1); setPlaying(false); }
          return;
        }
        if (active) setCurrentFrame(f);
        try {
          const url = await renderSceneFrame(activeScene, f, 640, 480);
          if (active) { setFrameDataUrl(url); setFrameError(null); }
        } catch (err) {
          if (active) { setStatusMsg(`Playback: ${err}`); setPlaying(false); }
          return;
        }
        // ~8 fps target; actual rate limited by render time
        await new Promise<void>((res) => setTimeout(res, 125));
      }
    })();

    return () => { active = false; };
  }, [playing, activeScene]); // currentFrame intentionally omitted — captured at loop start

  const handleOpenFile = useCallback(async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: "3D Movie Maker", extensions: ["3mm"] }],
    });
    if (!selected) return;
    const path = typeof selected === "string" ? selected : selected[0];
    if (!path) return;

    setPlaying(false);
    setCurrentFrame(0);
    setStatusMsg("Opening…");
    setFrameDataUrl(null);
    setFrameError(null);

    try {
      const info = await openFile(path);
      setMovie(info);

      const sceneList = await getSceneList();
      setScenes(sceneList);
      setActiveScene(0);
      setStatusMsg(`Loaded: ${info.file_name} — ${info.scene_count} scenes, ${info.total_frames} frames`);

      try {
        const s = await listSounds();
        setSounds(s);
      } catch {
        setSounds([]);
      }

      setFrameLoading(true);
      try {
        const url = await renderSceneFrame(0, 0, 640, 480);
        setFrameDataUrl(url);
        setFrameError(null);
      } catch (err) {
        setFrameError(String(err));
      } finally {
        setFrameLoading(false);
      }
    } catch (err) {
      setStatusMsg(`Error: ${err}`);
    }
  }, []);

  const handleSelectScene = useCallback(async (idx: number) => {
    setPlaying(false);
    setActiveScene(idx);
    setCurrentFrame(0);
    setFrameLoading(true);
    setFrameError(null);
    try {
      const url = await renderSceneFrame(idx, 0, 640, 480);
      setFrameDataUrl(url);
      setFrameError(null);
    } catch (err) {
      setFrameDataUrl(null);
      setFrameError(String(err));
      setStatusMsg(`Scene ${idx + 1}: ${err}`);
    } finally {
      setFrameLoading(false);
    }
  }, []);

  const handlePlayPause = useCallback(() => {
    setPlaying((p) => !p);
  }, []);

  const handleStepFrame = useCallback(async (delta: number) => {
    if (playing) return;
    const scene = scenes[activeScene];
    if (!scene) return;
    const next = Math.max(0, Math.min(currentFrame + delta, scene.frame_count - 1));
    setCurrentFrame(next);
    setFrameLoading(true);
    try {
      const url = await renderSceneFrame(activeScene, next, 640, 480);
      setFrameDataUrl(url);
      setFrameError(null);
    } catch (err) {
      setStatusMsg(`Frame ${next}: ${err}`);
    } finally {
      setFrameLoading(false);
    }
  }, [playing, activeScene, scenes, currentFrame]);

  const handlePlaySound = useCallback(async (cno: number) => {
    try {
      await playSound(cno);
    } catch (err) {
      setStatusMsg(`Sound error: ${err}`);
    }
  }, []);

  const activeSceneInfo = scenes[activeScene];

  return (
    <div className="studio">
      {/* ── Toolbar ─────────────────────────────────────────────── */}
      <div className="toolbar">
        <span className="toolbar-title">3DMMEx</span>
        <button className="btn-primary" onClick={handleOpenFile}>
          Open .3mm
        </button>
        {movie && (
          <span className="toolbar-info">
            {movie.file_name} · {movie.scene_count} scenes · {movie.total_frames} frames
          </span>
        )}
      </div>

      {/* ── Main area ────────────────────────────────────────────── */}
      <div className="main-area">
        {/* Viewport */}
        <div className="viewport-panel">
          <Viewport
            frameDataUrl={frameDataUrl}
            loading={frameLoading}
            error={frameError}
          />
          {/* Playback controls overlay */}
          {movie && (
            <div className="playback-bar">
              <button
                className="playback-btn"
                onClick={() => handleStepFrame(-1)}
                disabled={playing || currentFrame <= 0}
                title="Previous frame"
              >
                ◀
              </button>
              <button
                className="playback-btn playback-playpause"
                onClick={handlePlayPause}
                disabled={!activeSceneInfo || activeSceneInfo.frame_count <= 1}
                title={playing ? "Pause" : "Play"}
              >
                {playing ? "⏸" : "▶"}
              </button>
              <button
                className="playback-btn"
                onClick={() => handleStepFrame(1)}
                disabled={playing || !activeSceneInfo || currentFrame >= activeSceneInfo.frame_count - 1}
                title="Next frame"
              >
                ▶▶
              </button>
              <span className="playback-frame">
                {currentFrame + 1} / {activeSceneInfo?.frame_count ?? 0}
              </span>
            </div>
          )}
        </div>

        {/* Sidebar: sounds */}
        {sounds.length > 0 && (
          <div className="sidebar">
            <div className="sidebar-title">Sounds ({sounds.length})</div>
            <ul className="sound-list">
              {sounds.map((s) => (
                <li key={s.cno} className="sound-item">
                  <button
                    className="sound-play"
                    onClick={() => handlePlaySound(s.cno)}
                    title={`cno=${s.cno} type=${s.sound_type}`}
                  >
                    ▶
                  </button>
                  <span className="sound-name">
                    {s.name ?? `sound_${s.cno}`}
                  </span>
                  <span className="sound-type">{s.sound_type}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {/* ── Timeline ─────────────────────────────────────────────── */}
      <Timeline
        scenes={scenes}
        activeScene={activeScene}
        onSelectScene={handleSelectScene}
      />

      {/* ── Status bar ───────────────────────────────────────────── */}
      <div className="statusbar">{statusMsg}</div>
    </div>
  );
}
