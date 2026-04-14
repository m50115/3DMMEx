import { useState, useCallback } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import { openFile, getSceneList, renderDemoFrame, listSounds, playSound } from "../lib/engine";
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

  const handleOpenFile = useCallback(async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: "3D Movie Maker", extensions: ["3mm"] }],
    });
    if (!selected) return;
    const path = typeof selected === "string" ? selected : selected[0];
    if (!path) return;

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

      // Load sounds
      try {
        const s = await listSounds();
        setSounds(s);
      } catch {
        setSounds([]);
      }

      // Render demo viewport
      setFrameLoading(true);
      try {
        const url = await renderDemoFrame(640, 480);
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

  const handleSelectScene = useCallback((idx: number) => {
    setActiveScene(idx);
  }, []);

  const handlePlaySound = useCallback(async (cno: number) => {
    try {
      await playSound(cno);
    } catch (err) {
      setStatusMsg(`Sound error: ${err}`);
    }
  }, []);

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
