// Builds the dashboard demo and serves it at http://127.0.0.1:<port>/PHPev/, the same
// path as on GitHub Pages, so links that work here work there too.
//
//   node demo/serve.mjs [port]        (default: 8000)
//
// Only for trying the demo on this machine; listens on 127.0.0.1 only.
import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { dirname, extname, join, normalize, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const site = resolve(here, '..', 'target', 'demo-site');
const port = Number(process.argv[2] ?? 8000);
const BASE = '/PHPev/';
const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.json': 'application/json; charset=utf-8', '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon', '.woff2': 'font/woff2', '.txt': 'text/plain; charset=utf-8',
};

execFileSync(process.execPath, [join(here, 'build.mjs'), site], { stdio: 'inherit' });

createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (!path.startsWith(BASE)) {
    res.writeHead(302, { Location: BASE });
    return res.end();
  }
  const file = normalize(join(site, path.slice(BASE.length) || 'index.html'));
  if (file !== site && !file.startsWith(site + sep)) {
    res.writeHead(403);
    return res.end();
  }
  try {
    const body = await readFile(file.endsWith(sep) ? join(file, 'index.html') : file);
    res.writeHead(200, { 'Content-Type': TYPES[extname(file)] ?? 'application/octet-stream', 'Cache-Control': 'no-store' });
    res.end(body);
  } catch {
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('not found');
  }
}).listen(port, '127.0.0.1', () => console.log(`demo at http://127.0.0.1:${port}${BASE} (Ctrl+C to stop)`));
