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
  document.getElementById('actors')!.innerHTML =
    actors.length === 0
      ? '<div style="color:#666;font-size:0.8rem">No actors</div>'
      : actors.map(a =>
          `<div class="actor-row">
            #${a.actor_idx} cno=${a.cno}
            &nbsp; pos (${a.dx.toFixed(2)}, ${a.dy.toFixed(2)}, ${a.dz.toFixed(2)})
            &nbsp; frames ${a.nfrm_first}–${a.nfrm_last}
          </div>`
        ).join('');
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
