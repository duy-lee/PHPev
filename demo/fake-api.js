// Stands in for the PHPev daemon in the demo build: answers /api/v1 from sample data
// and serves /i18n from the copy next to the page. Loaded before app.js, only by the
// demo's index.html (demo/build.mjs); the real dashboard never includes this file.
import { get, requestStats, processes, logLines } from './fixtures.js';

const NOTHING = 'Demo with sample data: nothing was changed.';
const NOT_HERE = 'Not available in the demo.';
const realFetch = window.fetch.bind(window);

const reply = (status, data) =>
  new Response(JSON.stringify(data), { status, headers: { 'Content-Type': 'application/json' } });

// Long operations show the job dialog with a short, fake run.
const JOBS = new Map();
let nextJob = 1;
function startJob(title) {
  const id = `demo-${nextJob++}`;
  JOBS.set(id, title);
  return { job: id };
}

/** What a state change answers: a job for installs, an error for file pickers, else a note. */
function change(method, path, body) {
  if (/\/(pick|pick-folder)$/.test(path) || path === '/php/add') return reply(409, { error: NOT_HERE });
  if (path === '/php/install') return reply(200, startJob(`Installing PHP ${body.version ?? ''}`));
  if (path === '/mariadb/install') return reply(200, startJob(`Installing MariaDB ${body.version ?? ''}`));
  if (path === '/composer/install') return reply(200, startJob(`Installing Composer ${body.version ?? ''}`));
  if (path.endsWith('/export')) return reply(200, startJob(`Exporting ${body.name ?? 'database'}`));
  if (path === '/settings/ports' || path === '/settings/tld') return reply(200, { message: NOTHING, restarting: false });
  return reply(200, { message: NOTHING });
}

window.fetch = async (input, init = {}) => {
  const url = typeof input === 'string' ? input : input.url;
  if (url.startsWith('/i18n/')) return realFetch(`.${url}`, init);
  if (!url.startsWith('/api/v1/')) return realFetch(input, init);
  const [path, search = ''] = url.slice('/api/v1'.length).split('?');
  const method = (init.method ?? 'GET').toUpperCase();
  if (method !== 'GET') {
    let body = {};
    try {
      body = JSON.parse(init.body ?? '{}');
    } catch {
      /* not JSON */
    }
    return change(method, path, body);
  }
  const data = get(path, new URLSearchParams(search));
  return data === undefined ? reply(404, { error: NOT_HERE }) : reply(200, data);
};

/** Server-sent events of the daemon, played from sample data. */
class DemoEventSource {
  constructor(url) {
    this.url = url;
    this.readyState = 1;
    this.onmessage = null;
    this.onerror = null;
    this.timers = [];
    const [path, search = ''] = url.slice('/api/v1'.length).split('?');
    const q = new URLSearchParams(search);
    const send = (data) => this.readyState === 1 && this.onmessage?.({ data: JSON.stringify(data) });
    const every = (ms, f) => this.timers.push(setInterval(f, ms));
    const later = (ms, f) => this.timers.push(setTimeout(f, ms));

    if (path === '/metrics/stream') {
      const tick = () => send({ requests: requestStats(Number(q.get('window') ?? 60), q.get('host') || null, q.get('static') === '1'), processes: processes() });
      later(50, tick);
      every(1000, tick);
    } else if (path === '/logs/stream') {
      const source = q.get('source') ?? 'daemon';
      later(50, () => send({ reset: false, lines: logLines(source) }));
      if (source === 'daemon') {
        let n = 0;
        every(4000, () => {
          const t = new Date().toISOString().replace('T', ' ').slice(0, 19);
          const ms = 20 + ((n++ * 37) % 180);
          send({ reset: false, lines: [`${t} INFO  shop GET /products → 200 (${ms} ms)`] });
        });
      }
    } else if (path.startsWith('/jobs/')) {
      const id = path.split('/')[2];
      const title = JOBS.get(id) ?? 'Demo';
      const total = 31_457_280;
      let done = 0;
      const lines = ['Demo: no download, nothing is installed.'];
      every(250, () => {
        done = Math.min(total, done + total / 10);
        if (done < total) {
          send({ title, status: 'running', progress: ['download.zip', done, total], lines, error: null, cancellable: false });
        } else {
          send({ title, status: 'ok', progress: null, lines: [...lines, 'done'], error: null, cancellable: false });
          this.close();
        }
      });
    }
    // /events stays open and quiet: the page treats it as PHPev running.
  }

  addEventListener(type, f) {
    if (type === 'message') this.onmessage = f;
    if (type === 'error') this.onerror = f;
  }

  removeEventListener() {}

  close() {
    this.readyState = 2;
    this.timers.forEach((t) => {
      clearInterval(t);
      clearTimeout(t);
    });
  }
}

const RealEventSource = window.EventSource;
window.EventSource = function EventSource(url, options) {
  return String(url).startsWith('/api/v1/') ? new DemoEventSource(String(url)) : new RealEventSource(url, options);
};
