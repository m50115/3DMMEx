import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import {
  writeFileSync,
  appendFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
} from 'node:fs';
import { dirname, basename } from 'node:path';

// Where the browser-side debug payloads are written so the assistant can read them.
// Lives outside the repo on purpose (per workspace convention: ~/www/assets/<project>/).
const DEBUG_DIR = '/Users/devuser/www/assets/3dmmex';
const DEBUG_LATEST = `${DEBUG_DIR}/debug-latest.json`;
const DEBUG_TRAIL = `${DEBUG_DIR}/debug-trail.jsonl`;
const DEBUG_FRAME_LATEST = `${DEBUG_DIR}/debug-frame-latest.png`;

// Content files live outside the web app — serve them through a controlled
// middleware so the browser can auto-fetch tmpls.3cn / bkgds.3cn without the
// user dragging them every reload. Whitelist-only to avoid arbitrary FS reads.
const CONTENT_ROOT = '/Users/devuser/www/3DMMEx/content-files';
const CONTENT_ALLOW = new Set([
  'tmpls.3cn',
  'bkgds.3cn',
  'mtrls.3cn',
  'snds.3cn',
  'tdfs.3cn',
]);
const SAMPLES_ROOT = '/Users/devuser/www/3DMMEx/samples';

function debugSinkPlugin() {
  return {
    name: '3dmmex-debug-sink',
    configureServer(server: any) {
      mkdirSync(DEBUG_DIR, { recursive: true });

      // GET /__content/<file> → serves whitelisted content files from disk.
      server.middlewares.use('/__content/', (req: any, res: any) => {
        if (req.method !== 'GET') {
          res.statusCode = 405;
          res.end('GET only');
          return;
        }
        const name = basename(req.url ?? '');
        if (!CONTENT_ALLOW.has(name)) {
          res.statusCode = 404;
          res.end(`not whitelisted: ${name}`);
          return;
        }
        try {
          const full = `${CONTENT_ROOT}/${name}`;
          const stat = statSync(full);
          res.setHeader('Content-Type', 'application/octet-stream');
          res.setHeader('Content-Length', String(stat.size));
          res.end(readFileSync(full));
        } catch (e: any) {
          res.statusCode = 500;
          res.end(`read error: ${e?.message ?? e}`);
        }
      });

      // GET /__samples → JSON list of available .3mm sample movies.
      server.middlewares.use('/__samples', (req: any, res: any) => {
        if (req.method !== 'GET') {
          res.statusCode = 405;
          res.end('GET only');
          return;
        }
        try {
          const names: string[] = readdirSync(SAMPLES_ROOT)
            .filter((n: string) => n.toLowerCase().endsWith('.3mm'))
            .sort();
          res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify({ samples: names }));
        } catch (e: any) {
          res.statusCode = 500;
          res.end(`list error: ${e?.message ?? e}`);
        }
      });

      // GET /__sample/<file> → serves a sample .3mm.
      server.middlewares.use('/__sample/', (req: any, res: any) => {
        if (req.method !== 'GET') {
          res.statusCode = 405;
          res.end('GET only');
          return;
        }
        const name = basename(req.url ?? '');
        if (!name.toLowerCase().endsWith('.3mm')) {
          res.statusCode = 400;
          res.end('not a .3mm');
          return;
        }
        try {
          const full = `${SAMPLES_ROOT}/${name}`;
          const stat = statSync(full);
          res.setHeader('Content-Type', 'application/octet-stream');
          res.setHeader('Content-Length', String(stat.size));
          res.end(readFileSync(full));
        } catch (e: any) {
          res.statusCode = 404;
          res.end(`read error: ${e?.message ?? e}`);
        }
      });
      server.middlewares.use('/__debug-frame', (req: any, res: any) => {
        if (req.method !== 'POST') {
          res.statusCode = 405;
          res.end('POST only');
          return;
        }
        const chunks: Buffer[] = [];
        req.on('data', (c: Buffer) => chunks.push(c));
        req.on('end', () => {
          const body = Buffer.concat(chunks).toString('utf8');
          // Expecting "data:image/png;base64,<...>" — strip prefix.
          const comma = body.indexOf(',');
          const b64 = comma > 0 ? body.slice(comma + 1) : body;
          try {
            const png = Buffer.from(b64, 'base64');
            mkdirSync(dirname(DEBUG_FRAME_LATEST), { recursive: true });
            writeFileSync(DEBUG_FRAME_LATEST, png);
            res.statusCode = 204;
            res.end();
          } catch (e: any) {
            res.statusCode = 400;
            res.end(`bad image: ${e?.message ?? e}`);
          }
        });
      });

      server.middlewares.use('/__debug', (req: any, res: any) => {
        if (req.method !== 'POST') {
          res.statusCode = 405;
          res.end('POST only');
          return;
        }
        const chunks: Buffer[] = [];
        req.on('data', (c: Buffer) => chunks.push(c));
        req.on('end', () => {
          const body = Buffer.concat(chunks).toString('utf8');
          try {
            // Validate JSON before persisting so a malformed payload doesn't pollute the trail.
            JSON.parse(body);
            mkdirSync(dirname(DEBUG_LATEST), { recursive: true });
            writeFileSync(DEBUG_LATEST, body);
            appendFileSync(DEBUG_TRAIL, body.replace(/\n/g, ' ') + '\n');
            res.statusCode = 204;
            res.end();
          } catch (e: any) {
            res.statusCode = 400;
            res.end(`Bad JSON: ${e?.message ?? e}`);
          }
        });
      });
    },
  };
}

export default defineConfig({
  plugins: [wasm(), topLevelAwait(), debugSinkPlugin()],
  server: {
    fs: {
      // allow serving from modern/web/pkg (same dir) and above
      allow: ['.'],
    },
  },
});
