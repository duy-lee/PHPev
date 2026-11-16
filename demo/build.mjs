// Builds the dashboard demo for GitHub Pages: the real dashboard files, copied, with
// sample data instead of the PHPev daemon.
//
//   node demo/build.mjs [out-dir]        (default: target/demo-site)
//
// Only reads crates/daemon/dashboard and assets/, and writes to out-dir: the
// dashboard that ships in phpev-daemon.exe is left as it is. In the copy:
// - index.html gets relative links, an import map (modules import '/vendor/…',
//   which on a project page such as /PHPev/ would point outside the site), the
//   fake API and a small demo label;
// - absolute links in CSS and in the one <img> of app.js become relative.
// Everything else (/api/v1, /i18n) is answered by demo/fake-api.js in the browser.
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dashboard = join(repo, 'crates', 'daemon', 'dashboard');
const out = resolve(repo, process.argv[2] ?? join('target', 'demo-site'));
if (out === dashboard || out.startsWith(dashboard + sep) || out.startsWith(join(repo, 'crates') + sep)) {
  throw new Error(`refusing to write into the sources: ${out}`);
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
cpSync(dashboard, out, { recursive: true });

// The same extra files the daemon serves (crates/daemon/build.rs, EXTRA_ASSETS).
const buildRs = readFileSync(join(repo, 'crates', 'daemon', 'build.rs'), 'utf8');
const extras = [...buildRs.matchAll(/\("\/([\w.-]+)",\s*"\.\.\/\.\.\/([\w./-]+)"\)/g)];
if (extras.length === 0) throw new Error('EXTRA_ASSETS not found in crates/daemon/build.rs');
for (const [, url, source] of extras) cpSync(join(repo, source), join(out, url));

mkdirSync(join(out, 'demo'));
for (const f of ['fake-api.js', 'fixtures.js']) cpSync(join(repo, 'demo', f), join(out, 'demo', f));
writeFileSync(join(out, '.nojekyll'), '');

/** Replaces `from` (must occur `count` times, so a change in the dashboard fails the build). */
function rewrite(file, from, to, count) {
  const path = join(out, file);
  const text = readFileSync(path, 'utf8');
  const found = text.split(from).length - 1;
  if (count !== undefined && found !== count) throw new Error(`${file}: expected ${count} × ${JSON.stringify(from)}, found ${found}`);
  writeFileSync(path, text.split(from).join(to));
  return found;
}

const DEMO_HEAD = `<script type="importmap">{ "imports": { "/vendor/": "./vendor/" } }</script>
<script type="module" src="./demo/fake-api.js"></script>
<style>
  .demo-label { position: fixed; top: 8px; left: 50%; transform: translateX(-50%); z-index: 90; padding: 3px 10px;
    border: 1px solid var(--line); border-radius: var(--r-ctl); background: var(--pop); color: var(--text-3); font: 12px/18px var(--font); }
  .demo-label a { color: var(--accent-text); }
</style>
`;
const DEMO_BODY = `<div class="demo-label" role="note">Demo with sample data · <a href="https://github.com/duy-lee/PHPev">github.com/duy-lee/PHPev</a></div>
`;

const absLinks = rewrite('index.html', '="/', '="./');
rewrite('index.html', '<script type="module" src="./app.js"></script>', DEMO_HEAD + '<script type="module" src="./app.js"></script>', 1);
rewrite('index.html', '<div id="app"></div>', '<div id="app"></div>\n' + DEMO_BODY, 1);
rewrite('app.js', '<img src="/mark.svg"', '<img src="./mark.svg"', 1);

// CSS: url(/…) relative to the stylesheet.
const walk = (dir) => readdirSync(dir).flatMap((n) => (statSync(join(dir, n)).isDirectory() ? walk(join(dir, n)) : [join(dir, n)]));
let cssLinks = 0;
for (const file of walk(out).filter((f) => f.endsWith('.css') && !f.includes(`${sep}vendor${sep}`))) {
  const up = relative(dirname(file), out).split(sep).join('/') || '.';
  cssLinks += rewrite(relative(out, file), 'url(/', `url(${up}/`);
}

// Nothing absolute may be left besides what the import map and the fake API answer.
const left = [];
for (const file of walk(out).filter((f) => /\.(html|css|js|mjs)$/.test(f) && !f.includes(`${sep}vendor${sep}`) && !f.includes(`${sep}demo${sep}`))) {
  const text = readFileSync(file, 'utf8');
  for (const m of text.matchAll(/(?:src|href)="\/(?!\/)[^"]*"|url\(\/[^)]*\)|(?:import|from)\s*['"]\/(?!vendor\/)[^'"]*['"]/g)) left.push(`${relative(out, file)}: ${m[0]}`);
}
if (left.length) throw new Error(`absolute links left in the demo:\n${left.join('\n')}`);
if (!existsSync(join(out, 'mark.svg'))) throw new Error('mark.svg missing');

console.log(`demo built in ${relative(repo, out) || '.'}: ${absLinks} links in index.html, ${cssLinks} in CSS, ${extras.length} extra files`);
