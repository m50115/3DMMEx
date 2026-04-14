import type { SceneInfo } from "../lib/types";

interface TimelineProps {
  scenes: SceneInfo[];
  activeScene: number;
  onSelectScene: (idx: number) => void;
}

export function Timeline({ scenes, activeScene, onSelectScene }: TimelineProps) {
  if (scenes.length === 0) {
    return (
      <div className="timeline empty">
        <span>No movie loaded</span>
      </div>
    );
  }

  return (
    <div className="timeline">
      <div className="timeline-label">Scenes ({scenes.length})</div>
      <div className="timeline-scenes">
        {scenes.map((s) => (
          <button
            key={s.scene_idx}
            className={`scene-btn${s.scene_idx === activeScene ? " active" : ""}`}
            onClick={() => onSelectScene(s.scene_idx)}
            title={`${s.frame_count} frames, ${s.actor_count} actors`}
          >
            <span className="scene-num">{s.scene_idx + 1}</span>
            <span className="scene-frames">{s.frame_count}f</span>
          </button>
        ))}
      </div>
    </div>
  );
}
