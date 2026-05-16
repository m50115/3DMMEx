import init, { WasmEngine } from '../pkg/wasm_bindings.js';

// ── Types matching WasmEngine serde output ───────────────────────────────────

interface SceneInfo {
  scene_idx: number;
  frame_count: number;
  actor_count: number;
}

interface ActorInfo {
  actor_idx: number;
  cno: number;
  dx: number;
  dy: number;
  dz: number;
  nfrm_first: number;
  nfrm_last: number;
  xa_deg: number;
  ya_deg: number;
  za_deg: number;
}

// ── Resolution presets ────────────────────────────────────────────────────────

const RESOLUTIONS: Record<string, { w: number; h: number }> = {
  '480p':  { w: 640,  h: 480  },
  '720p':  { w: 1280, h: 720  },
  '1080p': { w: 1920, h: 1080 },
};

// ── State ────────────────────────────────────────────────────────────────────

let engine: WasmEngine | null = null;
let currentScene = 0;
let currentFileName = 'movie.3mm';
/** Cached tmpls.3cn bytes once fetched from the dev server. Loaded into every
 * new engine instance so the user never has to drag the same file twice. */
let cachedTmplsBytes: Uint8Array | null = null;
let tmplsPrefetch: Promise<Uint8Array | null> | null = null;

/** Current viewport resolution key, persisted in localStorage. */
let currentResKey: string = localStorage.getItem('dmmex.resolution') ?? '480p';
if (!RESOLUTIONS[currentResKey]) currentResKey = '480p';

function getVpDims(): { w: number; h: number } {
  return RESOLUTIONS[currentResKey] ?? RESOLUTIONS['480p'];
}

/** Cached Blob URLs for scene thumbnails. Revoked on new movie load. */
const thumbCache = new Map<number, string>();
/** Abort flag — set to true when a new movie is loaded to cancel in-flight thumb generation. */
let thumbGenId = 0;

// ── Bootstrap ────────────────────────────────────────────────────────────────

async function bootstrap() {
  await init();
  setupDebug();
  setupThemeToggle();
  setupResolutionSelector();
  setupDropzone();
  setupFileInput();
  setupDownload();
  setupContentFiles();
  setupSceneTextToggle();
  setupSamplePicker();
  // Kick off tmpls.3cn prefetch in the background. Loaded into engine as
  // soon as a movie is opened — user no longer drags it manually.
  prefetchTmpls();
}

async function prefetchTmpls(): Promise<Uint8Array | null> {
  if (cachedTmplsBytes) return cachedTmplsBytes;
  if (tmplsPrefetch) return tmplsPrefetch;
  tmplsPrefetch = (async () => {
    try {
      const r = await fetch('/__content/tmpls.3cn');
      if (!r.ok) return null;
      const buf = new Uint8Array(await r.arrayBuffer());
      cachedTmplsBytes = buf;
      return buf;
    } catch {
      return null;
    }
  })();
  return tmplsPrefetch;
}

async function autoLoadTmpls() {
  if (!engine) return;
  const bytes = await prefetchTmpls();
  if (!bytes) return;
  try {
    engine.load_content_file(bytes);
    const statusEl = document.getElementById('tmpls-status');
    if (statusEl) {
      statusEl.textContent = `tmpls.3cn: auto-loaded ✓ (${(bytes.length / 1024 / 1024).toFixed(1)} MB)`;
      statusEl.classList.add('loaded');
    }
  } catch (err) {
    console.warn('auto-load tmpls failed', err);
  }
}

async function setupSamplePicker() {
  // Populate the sample-movie dropdown from /__samples (dev-only endpoint).
  const sel = document.getElementById('sample-picker') as HTMLSelectElement | null;
  if (!sel) return;
  try {
    const r = await fetch('/__samples');
    if (!r.ok) return;
    const { samples } = (await r.json()) as { samples: string[] };
    sel.innerHTML =
      '<option value="">— sample movie —</option>' +
      samples.map((n) => `<option value="${n}">${n}</option>`).join('');
    sel.addEventListener('change', async () => {
      const name = sel.value;
      if (!name) return;
      const resp = await fetch(`/__sample/${encodeURIComponent(name)}`);
      if (!resp.ok) {
        showError(`Sample fetch failed: ${resp.status}`);
        return;
      }
      const buf = new Uint8Array(await resp.arrayBuffer());
      // Wrap in a File so loadFile reuses the same code path.
      const file = new File([buf], name, { type: 'application/octet-stream' });
      await loadFile(file);
    });
  } catch {
    /* dev endpoint missing → hide the picker */
    sel.style.display = 'none';
  }
}

// ── Debug mode ────────────────────────────────────────────────────────────────
// Enable with `?debug=1` in the URL or localStorage key `dmmex.debug=1`.
// When on: every render samples 5 pixels from the canvas, gathers scene/actor
// metadata, captures console.error, and POSTs the bundle to `/__debug` so the
// dev server writes it to ~/www/assets/3dmmex/debug-latest.json. A floating
// panel mirrors the same JSON for human inspection / screenshots.

interface DebugPayload {
  ts: string;
  scene_idx: number;
  scene_count: number;
  frame: number;
  vp: { w: number; h: number };
  resolution_key: string;
  actor_count: number;
  actors: ActorInfo[];
  pixels: { label: string; x: number; y: number; r: number; g: number; b: number; a: number }[];
  pixel_uniform: boolean;
  tmpls_loaded: boolean;
  movie_file: string;
  webgpu: { available: boolean; adapter?: any };
  last_errors: string[];
  notes: string[];
}

let debugEnabled = false;
const debugErrors: string[] = [];
let webgpuInfo: { available: boolean; adapter?: any } = { available: false };

function debugFlagFromUrl(): boolean {
  const url = new URL(window.location.href);
  if (url.searchParams.get('debug') === '1') return true;
  if (localStorage.getItem('dmmex.debug') === '1') return true;
  return false;
}

function setupDebug() {
  const btn = document.getElementById('debug-toggle');
  if (btn) {
    btn.addEventListener('click', () => {
      const next = !debugEnabled;
      localStorage.setItem('dmmex.debug', next ? '1' : '0');
      window.location.reload();
    });
  }

  debugEnabled = debugFlagFromUrl();
  if (!debugEnabled) return;
  document.documentElement.setAttribute('data-debug', '1');

  // Inject debug panel.
  const panel = document.createElement('pre');
  panel.id = 'debug-panel';
  panel.textContent = 'debug mode: waiting for first render…';
  document.body.appendChild(panel);

  // Capture console.error and unhandled errors so they show up in the dump.
  const origErr = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    debugErrors.push(args.map((a) => String(a)).join(' '));
    if (debugErrors.length > 50) debugErrors.shift();
    origErr(...args);
  };
  window.addEventListener('error', (e) => {
    debugErrors.push(`window.error: ${e.message} @ ${e.filename}:${e.lineno}`);
    if (debugErrors.length > 50) debugErrors.shift();
  });
  window.addEventListener('unhandledrejection', (e) => {
    debugErrors.push(`unhandledrejection: ${e.reason}`);
    if (debugErrors.length > 50) debugErrors.shift();
  });

  // Probe WebGPU once.
  void probeWebGPU();
}

async function probeWebGPU() {
  const nav = navigator as any;
  if (!nav.gpu) {
    webgpuInfo = { available: false };
    return;
  }
  try {
    const adapter = await nav.gpu.requestAdapter();
    if (!adapter) {
      webgpuInfo = { available: false };
      return;
    }
    const info = adapter.info ?? {};
    webgpuInfo = {
      available: true,
      adapter: {
        vendor: info.vendor,
        architecture: info.architecture,
        device: info.device,
        description: info.description,
        // limits/features are large — keep a small subset.
        features: Array.from((adapter.features ?? []) as Iterable<string>).slice(0, 20),
      },
    };
  } catch (e) {
    webgpuInfo = { available: false };
    debugErrors.push(`webgpu probe failed: ${e}`);
  }
}

async function captureDebug(frame: number) {
  if (!debugEnabled || !engine) return;
  const canvas = document.getElementById('viewport') as HTMLCanvasElement | null;
  if (!canvas) return;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return;

  const w = canvas.width;
  const h = canvas.height;
  const points: { label: string; x: number; y: number }[] = [
    { label: 'tl', x: 1, y: 1 },
    { label: 'tr', x: w - 2, y: 1 },
    { label: 'bl', x: 1, y: h - 2 },
    { label: 'br', x: w - 2, y: h - 2 },
    { label: 'mid', x: Math.floor(w / 2), y: Math.floor(h / 2) },
  ];
  const pixels = points.map((p) => {
    const d = ctx.getImageData(p.x, p.y, 1, 1).data;
    return { label: p.label, x: p.x, y: p.y, r: d[0], g: d[1], b: d[2], a: d[3] };
  });
  const first = pixels[0];
  const uniform = pixels.every(
    (p) => p.r === first.r && p.g === first.g && p.b === first.b && p.a === first.a,
  );

  let actors: ActorInfo[] = [];
  try {
    actors = (engine.get_scene_actors(currentScene) as ActorInfo[]) ?? [];
  } catch {
    /* ignore */
  }

  const notes: string[] = [];
  if (uniform) {
    notes.push(
      `viewport is uniform-fill ${first.r},${first.g},${first.b} — render reached clear stage but no draws applied`,
    );
  }
  if (actors.length === 0) {
    notes.push('scene has zero actors — try a different scene or load tmpls.3cn');
  }

  const payload: DebugPayload = {
    ts: new Date().toISOString(),
    scene_idx: currentScene,
    scene_count: (engine.get_scene_list() as SceneInfo[] | null)?.length ?? 0,
    frame,
    vp: { w, h },
    resolution_key: currentResKey,
    actor_count: actors.length,
    actors: actors.slice(0, 10),
    pixels,
    pixel_uniform: uniform,
    tmpls_loaded:
      (document.getElementById('tmpls-status')?.classList.contains('loaded')) ?? false,
    movie_file: currentFileName,
    webgpu: webgpuInfo,
    last_errors: debugErrors.slice(-10),
    notes,
  };

  const json = JSON.stringify(payload, null, 2);
  const panel = document.getElementById('debug-panel');
  if (panel) panel.textContent = json;

  try {
    await Promise.all([
      fetch('/__debug', { method: 'POST', body: json }),
      fetch('/__debug-frame', {
        method: 'POST',
        body: canvas.toDataURL('image/png'),
      }),
    ]);
  } catch (e) {
    // Sink not reachable in `vite preview` / static hosting — that's fine.
    console.warn('debug sink unreachable', e);
  }
}

// ── Theme toggle ──────────────────────────────────────────────────────────────

type Theme = 'light' | 'dark';

function applyTheme(theme: Theme) {
  document.documentElement.setAttribute('data-theme', theme);
  const btn = document.getElementById('theme-toggle');
  if (btn) btn.textContent = theme === 'dark' ? '🌙' : '☀️';
}

function setupThemeToggle() {
  // Determine initial theme: stored → prefers-color-scheme → dark default.
  const stored = localStorage.getItem('dmmex.theme') as Theme | null;
  let theme: Theme;
  if (stored === 'light' || stored === 'dark') {
    theme = stored;
  } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
    theme = 'light';
  } else {
    theme = 'dark';
  }
  applyTheme(theme);

  const btn = document.getElementById('theme-toggle');
  if (!btn) return;
  btn.addEventListener('click', () => {
    const current = document.documentElement.getAttribute('data-theme') as Theme;
    const next: Theme = current === 'light' ? 'dark' : 'light';
    localStorage.setItem('dmmex.theme', next);
    applyTheme(next);
  });
}

function setupResolutionSelector() {
  const sel = document.getElementById('res') as HTMLSelectElement | null;
  if (!sel) return;
  // Restore saved selection.
  sel.value = currentResKey;
  sel.addEventListener('change', async () => {
    const newKey = sel.value;
    if (!RESOLUTIONS[newKey]) return;
    currentResKey = newKey;
    localStorage.setItem('dmmex.resolution', newKey);
    // Re-render current scene at new resolution.
    await renderFrame();
  });
}

function setupSceneTextToggle() {
  const toggle = document.getElementById('scenes-text-toggle')!;
  const list = document.getElementById('scenes-text-list')!;
  toggle.addEventListener('click', () => {
    const visible = list.style.display !== 'none';
    list.style.display = visible ? 'none' : 'block';
    toggle.textContent = (visible ? '▶' : '▼') + ' lista de texto';
  });
}

// ── Drop zone ────────────────────────────────────────────────────────────────

function setupDropzone() {
  const drop = document.getElementById('drop')!;
  const input = document.getElementById('file-input') as HTMLInputElement;

  drop.addEventListener('click', () => input.click());
  drop.addEventListener('dragover', e => {
    e.preventDefault();
    drop.classList.add('hover');
  });
  drop.addEventListener('dragleave', () => drop.classList.remove('hover'));
  drop.addEventListener('drop', async e => {
    e.preventDefault();
    drop.classList.remove('hover');
    const file = e.dataTransfer?.files[0];
    if (file) await loadFile(file);
  });
}

function setupFileInput() {
  const input = document.getElementById('file-input') as HTMLInputElement;
  input.addEventListener('change', async () => {
    const file = input.files?.[0];
    if (file) await loadFile(file);
  });
}

async function loadFile(file: File) {
  currentFileName = file.name;
  setStatus(`Parsing ${file.name}…`);
  clearError();

  // Revoke stale thumbnail Blob URLs from the previous movie.
  revokeAllThumbs();

  const bytes = new Uint8Array(await file.arrayBuffer());
  try {
    engine = WasmEngine.open_file(bytes);
  } catch (err) {
    showError(`Parse error: ${err}`);
    return;
  }

  document.getElementById('drop')!.style.display = 'none';
  document.getElementById('layout')!.style.display = 'grid';

  // Auto-load cached tmpls.3cn into the fresh engine BEFORE rendering so the
  // first frame already has external templates available.
  await autoLoadTmpls();

  renderSceneList();
  renderThumbGrid();
  selectScene(0);
  setStatus(`Loaded: ${file.name} — ${engine.chunk_count()} chunks`);

  // Kick off async thumbnail generation (does not block UI).
  generateAllThumbs();
}

// ── Scene list ────────────────────────────────────────────────────────────────

function renderSceneList() {
  if (!engine) return;
  const scenes = engine.get_scene_list() as SceneInfo[];
  const el = document.getElementById('scenes')!;
  el.innerHTML = scenes.map(s =>
    `<div class="scene-row" data-idx="${s.scene_idx}">
      Scene ${s.scene_idx} &mdash; ${s.actor_count} actor${s.actor_count !== 1 ? 's' : ''}, ${s.frame_count} frames
    </div>`
  ).join('') || '<div style="color:#666;font-size:0.8rem">No scenes</div>';

  el.querySelectorAll<HTMLElement>('.scene-row').forEach(row => {
    row.addEventListener('click', () => selectScene(parseInt(row.dataset.idx!)));
  });
}

function selectScene(idx: number) {
  if (!engine) return;
  currentScene = idx;
  document.querySelectorAll('.scene-row').forEach((r, i) =>
    r.classList.toggle('active', i === idx)
  );
  document.querySelectorAll('.scene-thumb-card').forEach((c, i) =>
    c.classList.toggle('active', i === idx)
  );
  renderActors();
  renderFrame();
}

// ── Thumbnail gallery ────────────────────────────────────────────────────────

const THUMB_W = 160;
const THUMB_H = 120;

/** Revoke all cached Blob URLs and clear the map. */
function revokeAllThumbs() {
  thumbGenId++; // invalidate any in-flight generation loop
  thumbCache.forEach(url => URL.revokeObjectURL(url));
  thumbCache.clear();
}

/**
 * Render the placeholder grid immediately (dark cards with scene labels).
 * Thumbnails are filled in later by generateAllThumbs().
 */
function renderThumbGrid() {
  if (!engine) return;
  const scenes = engine.get_scene_list() as SceneInfo[];
  const container = document.getElementById('scene-thumbs')!;
  container.innerHTML = scenes.map(s => `
    <div class="scene-thumb-card" data-idx="${s.scene_idx}">
      <div class="thumb-placeholder" id="thumb-ph-${s.scene_idx}">…</div>
      <div class="thumb-label">Scene ${s.scene_idx} · ${s.actor_count} actor${s.actor_count !== 1 ? 's' : ''}</div>
    </div>`).join('');

  container.querySelectorAll<HTMLElement>('.scene-thumb-card').forEach(card => {
    card.addEventListener('click', () => selectScene(parseInt(card.dataset.idx!)));
  });
}

/**
 * Async loop: render each scene's frame 1 at thumbnail size, cache the Blob URL,
 * and swap it into the DOM. Aborts if thumbGenId changes (new movie loaded).
 */
async function generateAllThumbs() {
  if (!engine) return;
  const genId = ++thumbGenId;
  const scenes = engine.get_scene_list() as SceneInfo[];

  for (const s of scenes) {
    if (thumbGenId !== genId) return; // new movie loaded — abort
    if (!engine) return;

    try {
      const bmp = await withEngine(() =>
        engine!.render_frame(s.scene_idx, 1, THUMB_W, THUMB_H),
      );
      if (thumbGenId !== genId) return;

      const blob = new Blob([bmp.slice()], { type: 'image/bmp' });
      const url = URL.createObjectURL(blob);
      thumbCache.set(s.scene_idx, url);

      const ph = document.getElementById(`thumb-ph-${s.scene_idx}`);
      if (ph) {
        const img = document.createElement('img');
        img.src = url;
        img.width = THUMB_W;
        img.height = THUMB_H;
        img.alt = `Scene ${s.scene_idx}`;
        ph.replaceWith(img);
      }
    } catch {
      // Leave the dark placeholder in place — matches existing stream:// graceful failure.
      const ph = document.getElementById(`thumb-ph-${s.scene_idx}`);
      if (ph) ph.textContent = '✗';
    }
  }
}

// ── Actor list ────────────────────────────────────────────────────────────────

function renderActors() {
  if (!engine) return;
  const actors = engine.get_scene_actors(currentScene) as ActorInfo[];
  const container = document.getElementById('actors')!;
  if (actors.length === 0) {
    container.innerHTML = '<div style="color:#666;font-size:0.8rem">No actors</div>';
    return;
  }
  container.innerHTML = actors.map(a => `
    <div class="actor-row" data-actor="${a.actor_idx}">
      <div class="actor-info">#${a.actor_idx} cno=${a.cno} &nbsp; frames ${a.nfrm_first}–${a.nfrm_last}</div>
      <div class="actor-edit">
        <label>dX <input type="number" step="1" value="${a.dx.toFixed(2)}" data-axis="dx"></label>
        <label>dY <input type="number" step="1" value="${a.dy.toFixed(2)}" data-axis="dy"></label>
        <label>dZ <input type="number" step="1" value="${a.dz.toFixed(2)}" data-axis="dz"></label>
        <button class="apply-btn">Aplicar</button>
      </div>
    </div>`).join('');

  container.querySelectorAll<HTMLElement>('.apply-btn').forEach(btn => {
    btn.addEventListener('click', () => applyActorPosition(btn.closest('[data-actor]') as HTMLElement));
  });
}

async function applyActorPosition(row: HTMLElement) {
  if (!engine) return;
  const actorIdx = parseInt(row.dataset.actor!);
  const dx = parseFloat((row.querySelector('[data-axis="dx"]') as HTMLInputElement).value);
  const dy = parseFloat((row.querySelector('[data-axis="dy"]') as HTMLInputElement).value);
  const dz = parseFloat((row.querySelector('[data-axis="dz"]') as HTMLInputElement).value);
  try {
    engine.update_actor_position(currentScene, actorIdx, dx, dy, dz);
    setStatus(`Actor #${actorIdx} actualizado — re-renderizando…`);
    await renderFrame();
    setStatus(`Actor #${actorIdx} posición aplicada`);
  } catch (err) {
    showError(`Error actualizando actor #${actorIdx}: ${err}`);
  }
}

// ── Render ────────────────────────────────────────────────────────────────────

// JS-side mutex for WasmEngine. wasm-bindgen rejects concurrent `&mut self`
// calls with "recursive use of an object detected"; serialize anything that
// can fire simultaneously (user click + thumbnail generator + actor edit).
let engineGate: Promise<void> = Promise.resolve();
function withEngine<T>(fn: () => Promise<T>): Promise<T> {
  const prev = engineGate;
  let done!: () => void;
  engineGate = new Promise<void>((r) => (done = r));
  return prev.then(fn).finally(done);
}

async function renderFrame() {
  if (!engine) return;
  clearError();
  const { w, h } = getVpDims();
  await withEngine(async () => {
    try {
      const bmp = await engine!.render_frame(currentScene, 1, w, h);
      const blob = new Blob([bmp.slice()], { type: 'image/bmp' });
      const img = await createImageBitmap(blob);
      const canvas = document.getElementById('viewport') as HTMLCanvasElement;
      canvas.width = w;
      canvas.height = h;
      canvas.getContext('2d')!.drawImage(img, 0, 0);
      await captureDebug(1);
    } catch (err) {
      showError(`Render error: ${err}`);
      await captureDebug(1);
    }
  });
}

// ── Content files (tmpls.3cn) ─────────────────────────────────────────────────

function setupContentFiles() {
  const btn = document.getElementById('load-tmpls-btn')!;
  const input = document.getElementById('tmpls-input') as HTMLInputElement;

  btn.addEventListener('click', () => input.click());
  input.addEventListener('change', async () => {
    const file = input.files?.[0];
    if (!file || !engine) return;
    setStatus(`Cargando ${file.name}…`);
    clearError();
    const bytes = new Uint8Array(await file.arrayBuffer());
    try {
      engine.load_content_file(bytes);
      const statusEl = document.getElementById('tmpls-status')!;
      statusEl.textContent = `${file.name}: cargado ✓`;
      statusEl.classList.add('loaded');
      setStatus(`Listo. Re-renderizando…`);
      await renderFrame();
      setStatus(`${file.name} cargado — render actualizado`);
      // Regenerate thumbnails now that external templates are available.
      revokeAllThumbs();
      renderThumbGrid();
      // Re-apply active state after grid rebuild.
      document.querySelectorAll('.scene-thumb-card').forEach((c, i) =>
        c.classList.toggle('active', i === currentScene)
      );
      generateAllThumbs();
    } catch (err) {
      showError(`Error cargando ${file.name}: ${err}`);
    }
  });
}

// ── Download ──────────────────────────────────────────────────────────────────

function setupDownload() {
  document.getElementById('download')!.addEventListener('click', () => {
    if (!engine) return;
    try {
      const bytes = engine.to_bytes();
      const blob = new Blob([bytes.slice()], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = currentFileName;
      a.click();
      URL.revokeObjectURL(url);
      setStatus('Downloaded.');
    } catch (err) {
      showError(`Download error: ${err}`);
    }
  });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function setStatus(msg: string) {
  document.getElementById('status')!.textContent = msg;
}

function showError(msg: string) {
  const el = document.getElementById('error')!;
  el.textContent = msg;
  el.style.display = 'block';
}

function clearError() {
  const el = document.getElementById('error')!;
  el.style.display = 'none';
  el.textContent = '';
}

// ── Start ─────────────────────────────────────────────────────────────────────

bootstrap();
