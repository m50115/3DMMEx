import { forwardRef } from "react";

interface ViewportProps {
  frameSrc: string | null;
  loading: boolean;
  error: string | null;
  onLoad?: () => void;
  onError?: () => void;
}

export const Viewport = forwardRef<HTMLImageElement, ViewportProps>(
  function Viewport({ frameSrc, loading, error, onLoad, onError }, imgRef) {
    return (
      <div className="viewport">
        {loading && <div className="viewport-overlay">Rendering…</div>}
        {error && <div className="viewport-overlay error">{error}</div>}
        {frameSrc && (
          <img
            ref={imgRef}
            className="viewport-frame"
            src={frameSrc}
            alt="3D frame"
            draggable={false}
            onLoad={onLoad}
            onError={onError}
            style={{ display: loading ? 'none' : undefined }}
          />
        )}
        {!frameSrc && !loading && !error && (
          <div className="viewport-overlay hint">
            Open a .3mm file to begin
          </div>
        )}
      </div>
    );
  }
);
