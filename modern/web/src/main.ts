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

// ── State ────────────────────────────────────────────────────────────────────

let engine: WasmEngine | null = null;
let currentScene = 0;
let currentFileName = 'movie.3mm';

// ── Bootstrap ────────────────────────────────────────────────────────────────

async function bootstrap() {
  await init();
  setupDropzone();
  setupFileInput();
  setupDownload();
  setupContentFiles();
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

  const bytes = new Uint8Array(await file.arrayBuffer());
  try {
    engine = WasmEngine.open_file(bytes);
  } catch (err) {
    showError(`Parse error: ${err}`);
    return;
  }

  document.getElementById('drop')!.style.display = 'none';
  document.getElementById('layout')!.style.display = 'grid';

  renderSceneList();
  selectScene(0);
  setStatus(`Loaded: ${file.name} — ${engine.chunk_count()} chunks`);
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
  renderActors();
  renderFrame();
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

async function renderFrame() {
  if (!engine) return;
  clearError();
  try {
    const bmp = await engine.render_frame(currentScene, 1, 640, 480);
    const blob = new Blob([bmp.slice()], { type: 'image/bmp' });
    const img = await createImageBitmap(blob);
    const canvas = document.getElementById('viewport') as HTMLCanvasElement;
    canvas.getContext('2d')!.drawImage(img, 0, 0);
  } catch (err) {
    showError(`Render error: ${err}`);
  }
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
