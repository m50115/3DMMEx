import { useState, useCallback, useEffect, useRef } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

import { openFile, getSceneList, listSounds, playSound, getSceneActors, updateActorPosition, updateActorFrameRange, updateActorOrientation, saveFile, listTemplates, addActor, removeActor, createMovie } from "../lib/engine";
import type { MovieInfo, SceneInfo, SoundEntry, ActorInfo, TemplateInfo } from "../lib/types";
import { Timeline } from "./Timeline";
import { Viewport } from "./Viewport";

// ── Resolution presets ────────────────────────────────────────────────────────

const RESOLUTIONS = [
  { label: "480p (640×480)",   key: "480p",  w: 640,  h: 480  },
  { label: "720p (1280×720)",  key: "720p",  w: 1280, h: 720  },
  { label: "1080p (1920×1080)", key: "1080p", w: 1920, h: 1080 },
] as const;

type ResolutionKey = typeof RESOLUTIONS[number]["key"];

function loadStoredResolution(): ResolutionKey {
  const stored = localStorage.getItem("dmmex.resolution");
  if (stored === "720p" || stored === "1080p") return stored;
  return "480p";
}

// ── Theme ────────────────────────────────────────────────────────────────────

type Theme = "light" | "dark";

function loadStoredTheme(): Theme {
  const stored = localStorage.getItem("dmmex.theme");
  if (stored === "light" || stored === "dark") return stored;
  if (window.matchMedia("(prefers-color-scheme: light)").matches) return "light";
  return "dark";
}

function applyTheme(theme: Theme) {
  document.documentElement.setAttribute("data-theme", theme);
}

/** Build a stream:// URL for a given scene/frame. Cache-bust with timestamp.
 *  frame is 0-indexed in frontend; backend expects 1-indexed (3DMM convention).
 *  w/h are optional path segments; backend defaults to 640×480 if omitted. */
function streamUrl(scene: number, frame: number, w: number, h: number): string {
  return `stream://localhost/frame/${scene}/${frame + 1}/${w}/${h}?t=${Date.now()}`;
}

/** Resolve when the img element fires load, reject on error. */
function waitForLoad(img: HTMLImageElement): Promise<void> {
  return new Promise((resolve, reject) => {
    img.addEventListener("load",  () => resolve(), { once: true });
    img.addEventListener("error", () => reject(new Error("frame render error")), { once: true });
  });
}

export function Studio() {
  const [movie, setMovie] = useState<MovieInfo | null>(null);
  const [scenes, setScenes] = useState<SceneInfo[]>([]);
  const [activeScene, setActiveScene] = useState(0);
  const [frameSrc, setFrameSrc] = useState<string | null>(null);
  const [frameLoading, setFrameLoading] = useState(false);
  const [frameError, setFrameError] = useState<string | null>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const [sounds, setSounds] = useState<SoundEntry[]>([]);
  const [statusMsg, setStatusMsg] = useState("Ready");
  const [currentFrame, setCurrentFrame] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [fps, setFps] = useState<number | null>(null);
  const [resKey, setResKey] = useState<ResolutionKey>(loadStoredResolution);
  const [theme, setTheme] = useState<Theme>(() => {
    const t = loadStoredTheme();
    applyTheme(t);
    return t;
  });

  const handleThemeToggle = useCallback(() => {
    setTheme((prev) => {
      const next: Theme = prev === "dark" ? "light" : "dark";
      localStorage.setItem("dmmex.theme", next);
      applyTheme(next);
      return next;
    });
  }, []);

  // Derived viewport dimensions from the selected resolution preset.
  const resPreset = RESOLUTIONS.find(r => r.key === resKey) ?? RESOLUTIONS[0];
  const vpW = resPreset.w;
  const vpH = resPreset.h;

  // ── Editor state ──────────────────────────────────────────────────────
  const [actors, setActors] = useState<ActorInfo[]>([]);
  const [selectedActorIdx, setSelectedActorIdx] = useState<number | null>(null);
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTmplCno, setSelectedTmplCno] = useState<number | null>(null);
  const [editDx, setEditDx] = useState("0");
  const [editDy, setEditDy] = useState("0");
  const [editDz, setEditDz] = useState("0");
  const [editNfrmFirst, setEditNfrmFirst] = useState("0");
  const [editNfrmLast, setEditNfrmLast] = useState("0");
  const [editXaDeg, setEditXaDeg] = useState("0");
  const [editYaDeg, setEditYaDeg] = useState("0");
  const [editZaDeg, setEditZaDeg] = useState("0");

  // Playback loop — recursive setTimeout so frames don't queue when render is slow.
  const playingRef = useRef(playing);
  useEffect(() => { playingRef.current = playing; }, [playing]);

  // Keep a ref so the playback loop always reads the current resolution.
  const vpWRef = useRef(vpW);
  const vpHRef = useRef(vpH);
  useEffect(() => { vpWRef.current = vpW; vpHRef.current = vpH; }, [vpW, vpH]);

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

        const img = imgRef.current;
        if (!img) return;

        const t0 = performance.now();
        // Set stream:// src — WKWebView fetches it, Rust renders → returns BMP bytes
        img.src = streamUrl(activeScene, f, vpWRef.current, vpHRef.current);
        try {
          await waitForLoad(img);
          const elapsed = performance.now() - t0;
          if (active) {
            setFrameError(null);
            setFps(Math.round(1000 / elapsed));
          }
        } catch (err) {
          if (active) { setStatusMsg(`Playback: ${err}`); setPlaying(false); }
          return;
        }
      }
    })();

    return () => { active = false; setFps(null); };
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
    setFrameSrc(null);
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

      try {
        const tmplList = await listTemplates();
        setTemplates(tmplList);
        setSelectedTmplCno(tmplList.length > 0 ? tmplList[0].cno : null);
      } catch {
        setTemplates([]);
        setSelectedTmplCno(null);
      }

      setFrameLoading(true);
      setFrameSrc(streamUrl(0, 0, vpW, vpH)); // onLoad/onError handlers clear frameLoading
    } catch (err) {
      setStatusMsg(`Error: ${err}`);
    }
  }, []);

  const handleNewMovie = useCallback(async () => {
    const savePath = await saveDialog({
      filters: [{ name: "3D Movie Maker", extensions: ["3mm"] }],
      defaultPath: "untitled.3mm",
    });
    if (!savePath) return;

    setPlaying(false);
    setCurrentFrame(0);
    setStatusMsg("Creating…");
    setFrameSrc(null);
    setFrameError(null);
    setActors([]);
    setSelectedActorIdx(null);

    try {
      const info = await createMovie(savePath);
      setMovie(info);
      setScenes([{ scene_idx: 0, frame_count: 0, actor_count: 0 }]);
      setActiveScene(0);
      setStatusMsg(`Created: ${info.file_name}`);
      setSounds([]);
      setFrameSrc(null); // empty scene — no render yet
    } catch (err) {
      setStatusMsg(`Error: ${err}`);
    }
  }, []);

  const handleSelectScene = useCallback((idx: number) => {
    setPlaying(false);
    setActiveScene(idx);
    setCurrentFrame(0);
    setFrameLoading(true);
    setFrameError(null);
    setFrameSrc(streamUrl(idx, 0, vpW, vpH));
  }, []);

  const handlePlayPause = useCallback(() => {
    setPlaying((p) => !p);
  }, []);

  const handleFrameLoad = useCallback(() => {
    setFrameLoading(false);
    setFrameError(null);
  }, []);

  const handleFrameError = useCallback(() => {
    setFrameLoading(false);
    setFrameError("Render failed");
  }, []);

  const handleStepFrame = useCallback((delta: number) => {
    if (playing) return;
    const scene = scenes[activeScene];
    if (!scene) return;
    const next = Math.max(0, Math.min(currentFrame + delta, scene.frame_count - 1));
    setCurrentFrame(next);
    setFrameLoading(true);
    setFrameError(null);
    setFrameSrc(streamUrl(activeScene, next, vpW, vpH));
  }, [playing, activeScene, scenes, currentFrame]);

  const handlePlaySound = useCallback(async (cno: number) => {
    try {
      await playSound(cno);
    } catch (err) {
      setStatusMsg(`Sound error: ${err}`);
    }
  }, []);

  // ── Editor handlers ───────────────────────────────────────────────────

  // Load actor list whenever the active scene changes.
  useEffect(() => {
    if (!movie) return;
    getSceneActors(activeScene)
      .then(setActors)
      .catch(() => setActors([]));
    setSelectedActorIdx(null);
  }, [movie, activeScene]);

  const handleSelectActor = useCallback((actor: ActorInfo) => {
    setSelectedActorIdx(actor.actor_idx);
    setEditDx(actor.dx.toFixed(4));
    setEditDy(actor.dy.toFixed(4));
    setEditDz(actor.dz.toFixed(4));
    setEditNfrmFirst(String(actor.nfrm_first));
    setEditNfrmLast(String(actor.nfrm_last));
    setEditXaDeg(actor.xa_deg.toFixed(2));
    setEditYaDeg(actor.ya_deg.toFixed(2));
    setEditZaDeg(actor.za_deg.toFixed(2));
  }, []);

  const handleApplyPosition = useCallback(async () => {
    if (selectedActorIdx === null) return;
    const dx = parseFloat(editDx) || 0;
    const dy = parseFloat(editDy) || 0;
    const dz = parseFloat(editDz) || 0;
    try {
      await updateActorPosition(activeScene, selectedActorIdx, dx, dy, dz);
      // Refresh actors list and viewport
      const updated = await getSceneActors(activeScene);
      setActors(updated);
      setFrameLoading(true);
      setFrameError(null);
      setFrameSrc(streamUrl(activeScene, currentFrame, vpWRef.current, vpHRef.current));
      setStatusMsg(`Actor ${selectedActorIdx} position updated.`);
    } catch (err) {
      setStatusMsg(`Update failed: ${err}`);
    }
  }, [selectedActorIdx, activeScene, editDx, editDy, editDz, currentFrame]);

  const handleApplyFrameRange = useCallback(async () => {
    if (selectedActorIdx === null) return;
    const nfrmFirst = parseInt(editNfrmFirst) || 0;
    const nfrmLast  = parseInt(editNfrmLast)  || 0;
    try {
      await updateActorFrameRange(activeScene, selectedActorIdx, nfrmFirst, nfrmLast);
      const updated = await getSceneActors(activeScene);
      setActors(updated);
      setFrameLoading(true);
      setFrameError(null);
      setFrameSrc(streamUrl(activeScene, currentFrame, vpWRef.current, vpHRef.current));
      setStatusMsg(`Actor ${selectedActorIdx} frame range updated.`);
    } catch (err) {
      setStatusMsg(`Frame range update failed: ${err}`);
    }
  }, [selectedActorIdx, activeScene, editNfrmFirst, editNfrmLast, currentFrame]);

  const handleApplyOrientation = useCallback(async () => {
    if (selectedActorIdx === null) return;
    const xaDeg = parseFloat(editXaDeg) || 0;
    const yaDeg = parseFloat(editYaDeg) || 0;
    const zaDeg = parseFloat(editZaDeg) || 0;
    try {
      await updateActorOrientation(activeScene, selectedActorIdx, xaDeg, yaDeg, zaDeg);
      const updated = await getSceneActors(activeScene);
      setActors(updated);
      setFrameLoading(true);
      setFrameError(null);
      setFrameSrc(streamUrl(activeScene, currentFrame, vpWRef.current, vpHRef.current));
      setStatusMsg(`Actor ${selectedActorIdx} orientation updated.`);
    } catch (err) {
      setStatusMsg(`Orientation update failed: ${err}`);
    }
  }, [selectedActorIdx, activeScene, editXaDeg, editYaDeg, editZaDeg, currentFrame]);

  const handleSave = useCallback(async () => {
    try {
      const savedPath = await saveFile();
      setStatusMsg(`Saved: ${savedPath}`);
    } catch (err) {
      setStatusMsg(`Save failed: ${err}`);
    }
  }, []);

  const handleAddActor = useCallback(async () => {
    if (selectedTmplCno === null) return;
    try {
      const newCno = await addActor(activeScene, selectedTmplCno, 0, 0, 0);
      const updated = await getSceneActors(activeScene);
      setActors(updated);
      setFrameLoading(true);
      setFrameError(null);
      setFrameSrc(streamUrl(activeScene, currentFrame, vpWRef.current, vpHRef.current));
      setStatusMsg(`Added actor cno=${newCno}.`);
    } catch (err) {
      setStatusMsg(`Add actor failed: ${err}`);
    }
  }, [activeScene, selectedTmplCno, currentFrame]);

  const handleRemoveActor = useCallback(async (actorIdx: number) => {
    try {
      await removeActor(activeScene, actorIdx);
      const updated = await getSceneActors(activeScene);
      setActors(updated);
      if (selectedActorIdx === actorIdx) setSelectedActorIdx(null);
      setFrameLoading(true);
      setFrameError(null);
      setFrameSrc(streamUrl(activeScene, currentFrame, vpWRef.current, vpHRef.current));
      setStatusMsg(`Removed actor ${actorIdx}.`);
    } catch (err) {
      setStatusMsg(`Remove actor failed: ${err}`);
    }
  }, [activeScene, selectedActorIdx, currentFrame]);

  const activeSceneInfo = scenes[activeScene];

  // Re-render the viewport whenever the resolution changes (and a movie is loaded).
  const handleResolutionChange = useCallback((newKey: ResolutionKey) => {
    setResKey(newKey);
    localStorage.setItem("dmmex.resolution", newKey);
    if (!movie) return;
    const preset = RESOLUTIONS.find(r => r.key === newKey) ?? RESOLUTIONS[0];
    setPlaying(false);
    setFrameLoading(true);
    setFrameError(null);
    setFrameSrc(streamUrl(activeScene, currentFrame, preset.w, preset.h));
  }, [movie, activeScene, currentFrame]);

  return (
    <div className="studio">
      {/* ── Toolbar ─────────────────────────────────────────────── */}
      <div className="toolbar">
        <span className="toolbar-title">3DMMEx</span>
        <button className="btn-secondary" onClick={handleNewMovie}>
          New Movie
        </button>
        <button className="btn-primary" onClick={handleOpenFile}>
          Open .3mm
        </button>
        {/* Resolution selector */}
        <select
          value={resKey}
          onChange={(e) => handleResolutionChange(e.target.value as ResolutionKey)}
          title="Viewport resolution"
          style={{
            background: 'var(--bg)', border: '1px solid var(--border)',
            color: 'var(--text)', borderRadius: 3, padding: '3px 6px',
            fontSize: 12, cursor: 'pointer',
          }}
        >
          {RESOLUTIONS.map(r => (
            <option key={r.key} value={r.key}>{r.label}</option>
          ))}
        </select>
        <button
          className="theme-toggle-btn"
          onClick={handleThemeToggle}
          title="Toggle light/dark theme"
        >
          {theme === "dark" ? "🌙" : "☀️"}
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
            ref={imgRef}
            frameSrc={frameSrc}
            loading={frameLoading}
            error={frameError}
            onLoad={handleFrameLoad}
            onError={handleFrameError}
          />
          {/* FPS overlay — visible during playback only */}
          {fps !== null && (
            <div style={{
              position: 'absolute', top: 8, right: 8,
              background: 'rgba(0,0,0,0.65)', color: '#00ff88',
              padding: '2px 8px', fontFamily: 'monospace', fontSize: 13,
              borderRadius: 4, pointerEvents: 'none', userSelect: 'none',
            }}>
              {fps} fps
            </div>
          )}
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

        {/* Sidebar: actor editor + sounds */}
        {movie && (
          <div className="sidebar">
            {/* Actor list */}
            <div className="sidebar-title">Actors ({actors.length})</div>
            <ul className="sound-list">
              {actors.map((a) => (
                <li
                  key={a.actor_idx}
                  className="sound-item"
                  style={{ background: selectedActorIdx === a.actor_idx ? '#1e2a3a' : undefined, cursor: 'pointer' }}
                  onClick={() => handleSelectActor(a)}
                >
                  <span className="sound-name">Actor {a.actor_idx}</span>
                  <span className="sound-type" title={`cno=${a.cno}`}>cno={a.cno}</span>
                  <button
                    onClick={(e) => { e.stopPropagation(); handleRemoveActor(a.actor_idx); }}
                    title="Remove actor"
                    style={{
                      marginLeft: 'auto', background: '#4a1a1a', border: 'none',
                      color: 'var(--text)', borderRadius: 3, padding: '2px 6px',
                      fontSize: 11, cursor: 'pointer',
                    }}
                  >
                    ✕
                  </button>
                </li>
              ))}
              {actors.length === 0 && (
                <li style={{ padding: '8px 12px', color: 'var(--text-dim)', fontSize: 12 }}>
                  No actors
                </li>
              )}
            </ul>

            {/* Template picker — add actor */}
            {templates.length > 0 && (
              <div style={{ padding: '8px 12px', borderTop: '1px solid var(--border)', display: 'flex', gap: 6 }}>
                <select
                  value={selectedTmplCno ?? ''}
                  onChange={(e) => setSelectedTmplCno(Number(e.target.value))}
                  style={{
                    flex: 1, background: 'var(--bg)', border: '1px solid var(--border)',
                    color: 'var(--text)', borderRadius: 3, padding: '3px 6px', fontSize: 12,
                  }}
                >
                  {templates.map((t) => (
                    <option key={t.cno} value={t.cno}>
                      {t.name ?? `tmpl_${t.cno}`}
                    </option>
                  ))}
                </select>
                <button
                  className="btn-primary"
                  style={{ fontSize: 11, padding: '3px 8px', whiteSpace: 'nowrap' }}
                  onClick={handleAddActor}
                  disabled={selectedTmplCno === null}
                >
                  + Add
                </button>
              </div>
            )}

            {/* Position editor — shown when an actor is selected */}
            {selectedActorIdx !== null && (
              <div style={{ padding: '10px 12px', borderTop: '1px solid var(--border)' }}>
                <div className="sidebar-title" style={{ padding: '0 0 6px', border: 'none' }}>
                  Position — Actor {selectedActorIdx}
                </div>
                {[
                  { label: 'X', value: editDx, set: setEditDx },
                  { label: 'Y', value: editDy, set: setEditDy },
                  { label: 'Z', value: editDz, set: setEditDz },
                ].map(({ label, value, set }) => (
                  <div key={label} style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                    <span style={{ width: 12, fontSize: 11, color: 'var(--text-dim)' }}>{label}</span>
                    <input
                      type="number"
                      step="0.25"
                      value={value}
                      onChange={(e) => set(e.target.value)}
                      style={{
                        flex: 1, background: 'var(--bg)', border: '1px solid var(--border)',
                        color: 'var(--text)', borderRadius: 3, padding: '2px 6px',
                        fontSize: 12, fontFamily: 'monospace',
                      }}
                    />
                  </div>
                ))}
                <button
                  className="btn-primary"
                  style={{ width: '100%', fontSize: 11, padding: '4px 0', marginTop: 8 }}
                  onClick={handleApplyPosition}
                >
                  Apply Position
                </button>

                {/* Frame range */}
                <div className="sidebar-title" style={{ padding: '8px 0 4px', border: 'none', borderTop: '1px solid var(--border)', marginTop: 8 }}>
                  Frame Range
                </div>
                {[
                  { label: 'First', value: editNfrmFirst, set: setEditNfrmFirst },
                  { label: 'Last',  value: editNfrmLast,  set: setEditNfrmLast  },
                ].map(({ label, value, set }) => (
                  <div key={label} style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                    <span style={{ width: 30, fontSize: 11, color: 'var(--text-dim)' }}>{label}</span>
                    <input
                      type="number"
                      step="1"
                      value={value}
                      onChange={(e) => set(e.target.value)}
                      style={{
                        flex: 1, background: 'var(--bg)', border: '1px solid var(--border)',
                        color: 'var(--text)', borderRadius: 3, padding: '2px 6px',
                        fontSize: 12, fontFamily: 'monospace',
                      }}
                    />
                  </div>
                ))}
                <button
                  className="btn-primary"
                  style={{ width: '100%', fontSize: 11, padding: '4px 0', marginTop: 4 }}
                  onClick={handleApplyFrameRange}
                >
                  Apply Frame Range
                </button>

                {/* Rotation */}
                <div className="sidebar-title" style={{ padding: '8px 0 4px', border: 'none', borderTop: '1px solid var(--border)', marginTop: 8 }}>
                  Rotation (degrees)
                </div>
                {[
                  { label: 'Pitch', value: editXaDeg, set: setEditXaDeg },
                  { label: 'Yaw',   value: editYaDeg, set: setEditYaDeg },
                  { label: 'Roll',  value: editZaDeg, set: setEditZaDeg },
                ].map(({ label, value, set }) => (
                  <div key={label} style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                    <span style={{ width: 30, fontSize: 11, color: 'var(--text-dim)' }}>{label}</span>
                    <input
                      type="number"
                      step="1"
                      min="0"
                      max="359"
                      value={value}
                      onChange={(e) => set(e.target.value)}
                      style={{
                        flex: 1, background: 'var(--bg)', border: '1px solid var(--border)',
                        color: 'var(--text)', borderRadius: 3, padding: '2px 6px',
                        fontSize: 12, fontFamily: 'monospace',
                      }}
                    />
                  </div>
                ))}
                <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
                  <button
                    className="btn-primary"
                    style={{ flex: 1, fontSize: 11, padding: '4px 0' }}
                    onClick={handleApplyOrientation}
                  >
                    Apply Rotation
                  </button>
                  <button
                    className="btn-primary"
                    style={{ flex: 1, fontSize: 11, padding: '4px 0', background: '#1a4a1a' }}
                    onClick={handleSave}
                  >
                    Save
                  </button>
                </div>
              </div>
            )}

            {/* Sounds */}
            {sounds.length > 0 && (
              <>
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
              </>
            )}
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
