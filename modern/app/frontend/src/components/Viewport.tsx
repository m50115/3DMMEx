interface ViewportProps {
  frameDataUrl: string | null;
  loading: boolean;
  error: string | null;
}

export function Viewport({ frameDataUrl, loading, error }: ViewportProps) {
  return (
    <div className="viewport">
      {loading && <div className="viewport-overlay">Rendering…</div>}
      {error && <div className="viewport-overlay error">{error}</div>}
      {frameDataUrl && !loading && (
        <img
          className="viewport-frame"
          src={frameDataUrl}
          alt="3D frame"
          draggable={false}
        />
      )}
      {!frameDataUrl && !loading && !error && (
        <div className="viewport-overlay hint">
          Open a .3mm file to begin
        </div>
      )}
    </div>
  );
}
