// PHPev dashboard. Presentation only: every action is an
// API v1 call, so another UI (Slint, TUI) can replace this file without
// touching the daemon. Strings come from /i18n/<lang>.json.
import { html, render, useState, useEffect, useCallback, useRef } from '/vendor/htm-preact-standalone.mjs';

// ------------------------------------------------------------------ i18n

let STRINGS = {};
const t = (key, vars = {}) => (STRINGS[key] ?? key).replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? '');

// ------------------------------------------------------------------ API

async function api(method, path, body) {
  const options = { method, credentials: 'same-origin', headers: {} };
  if (method !== 'GET') {
    // The daemon only accepts JSON for state changes (CSRF protection).
    options.headers['Content-Type'] = 'application/json';
    options.body = JSON.stringify(body ?? {});
  }
  const response = await fetch('/api/v1' + path, options);
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || response.statusText);
  return data;
}

// Rust `Result<T, String>` arrives as {"Ok": T} or {"Err": "message"}.
const res = (r) => (r && 'Ok' in r ? { ok: true, value: r.Ok } : { ok: false, error: r?.Err ?? '?' });
const mb = (kb) => (kb == null ? '—' : `${(kb / 1024).toFixed(1)} MB`);
const uptime = (s) => (s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m`);

// Loads `path`, reloading every `everyMs` while the tab is visible.
function useApi(path, everyMs) {
  const [state, setState] = useState({ data: null, error: null });
  const load = useCallback(() => {
    api('GET', path).then((data) => setState({ data, error: null }), (e) => setState((s) => ({ ...s, error: e.message })));
  }, [path]);
  useEffect(() => {
    load();
    if (!everyMs) return;
    const timer = setInterval(() => document.visibilityState === 'visible' && load(), everyMs);
    return () => clearInterval(timer);
  }, [load, everyMs]);
  return [state, load];
}

// ------------------------------------------------------------------ shell

let showToast = () => {};
function Toast() {
  const [msg, setMsg] = useState(null);
  showToast = (text, error = false) => {
    setMsg({ text, error });
    setTimeout(() => setMsg(null), error ? 8000 : 3500);
  };
  return msg && html`<div class="toast ${msg.error ? 'error' : ''}">${msg.text}</div>`;
}

// Runs an API action, reports the outcome, then refreshes.
async function act(promise, refresh) {
  try {
    const out = await promise;
    if (out?.message) showToast(out.message);
  } catch (e) {
    showToast(t('common.error', { message: e.message }), true);
  }
  refresh?.();
}

// Follows a long operation (install) over server-sent events.
function JobDialog({ id, onClose }) {
  const [job, setJob] = useState(null);
  useEffect(() => {
    const source = new EventSource(`/api/v1/jobs/${id}/stream`);
    source.onmessage = (e) => {
      const info = JSON.parse(e.data);
      setJob(info);
      if (info.status !== 'running') source.close();
    };
    return () => source.close();
  }, [id]);
  if (!job) return html`<div class="overlay"><div class="dialog">${t('common.loading')}</div></div>`;
  const [label, done, total] = job.progress ?? [];
  return html`<div class="overlay"><div class="dialog">
    <h2>${job.title}</h2>
    <p>${job.status === 'running' ? t('job.running') : job.status === 'ok' ? t('job.ok') : t('job.error', { message: job.error })}</p>
    ${job.progress && html`<div class="mono">${label}</div><progress value=${done} max=${total ?? undefined}></progress>`}
    <div class="log mono">${job.lines.join('\n')}</div>
    <p><button onClick=${onClose} disabled=${job.status === 'running'}>${t('common.close')}</button></p>
  </div></div>`;
}

// ------------------------------------------------------------------ pages

function Overview() {
  const [{ data, error }, reload] = useApi('/overview', 2000);
  if (!data) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const s = data.status;
  const r = s.router;
  const dash = res(s.dashboard);
  return html`
    <h1>${t('nav.overview')}</h1>
    <div class="cards">
      <div class="card"><h3>${t('overview.daemon')}</h3>
        <div class="big">v${data.version}</div>
        <div class="muted">pid ${s.pid} · ${t('overview.uptime', { uptime: uptime(s.uptime_secs) })} · ${t('overview.ram', { mb: mb(s.memory_kb) })}</div></div>
      <div class="card"><h3>${t('overview.router')}</h3>
        ${r.error ? html`<span class="badge fail">${r.error}</span>` : html`<div class="big">*.${r.tld}</div><div class="muted mono">${r.listening.join(', ')}</div>`}</div>
      <div class="card"><h3>${t('overview.https')}</h3>
        ${r.https_error ? html`<span class="badge fail">${r.https_error}</span>`
          : r.https_listening.length ? html`<div class="muted mono">${r.https_listening.join(', ')}</div>` : html`<div class="muted">${t('overview.https_off')}</div>`}</div>
      <div class="card"><h3>${t('overview.mariadb')}</h3>
        ${s.mariadb ? html`<div class="big">${s.mariadb.version} <span class="badge ok">${s.mariadb.state}</span></div>
          <div class="muted">127.0.0.1:${s.mariadb.port} · ${t('overview.ram', { mb: mb(s.mariadb.memory_kb) })}</div>
          <p><button onClick=${() => act(api('POST', '/mariadb/stop'), reload)}>${t('common.stop')}</button></p>`
        : html`<div class="big">${t('common.stopped')}</div><p><button onClick=${() => act(api('POST', '/mariadb/start'), reload)}>${t('common.start')}</button></p>`}</div>
      <div class="card"><h3>${t('overview.php_pools')}</h3>
        ${s.php_pools.length ? s.php_pools.map((p) => html`<div>${t('overview.pool', { version: p.version, count: p.workers.length, mb: mb(p.workers.reduce((a, w) => a + (w.memory_kb ?? 0), 0)) })}</div>`)
          : html`<div class="muted">${t('overview.no_pools')}</div>`}</div>
      <div class="card"><h3>${t('overview.dashboard')}</h3>
        ${dash.ok ? html`<div class="mono">${dash.value}</div>` : html`<span class="badge fail">${dash.error}</span>`}</div>
    </div>
    <h2>${t('overview.checks')}</h2>
    <ul class="checks">${data.checks.map((c) => html`<li>
      <span class="badge ${c.status}">${c.status}</span> ${c.title}
      ${(c.details.length > 0 || c.hint) && html`<ul>${c.details.map((d) => html`<li>${d}</li>`)}${c.hint && html`<li>→ <code>${c.hint}</code></li>`}</ul>`}
    </li>`)}</ul>`;
}

function Sites() {
  const [{ data: sites, error }, reload] = useApi('/sites', 5000);
  const [{ data: phpInfo }] = useApi('/php');
  const [path, setPath] = useState('');
  const [name, setName] = useState('');
  if (!sites) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const versions = phpInfo?.installed.map((v) => v.version.split('.').slice(0, 2).join('.')) ?? [];
  const lines = [...new Set(versions)];
  const open = (s, target) => act(api('POST', `/sites/${s.name}/open`, { target }));
  return html`
    <h1>${t('sites.title')}</h1>
    ${sites.length === 0 ? html`<p class="muted">${t('sites.empty')}</p>` : html`<table>
      <tr><th>${t('sites.url')}</th><th>${t('sites.kind')}</th><th>${t('sites.php')}</th><th>${t('sites.https')}</th><th>${t('sites.folder')}</th><th>${t('sites.actions')}</th></tr>
      ${sites.map((s) => {
        const php = res(s.php);
        return html`<tr>
          <td><a href=${s.url} target="_blank" rel="noopener">${s.url}</a></td>
          <td>${s.kind}</td>
          <td>
            <select value=${s.php_pinned ?? ''} onChange=${(e) => act(api('PUT', `/sites/${s.name}/php`, { version: e.target.value || null }), reload)}>
              <option value="">${t('sites.php_default')}</option>
              ${lines.map((l) => html`<option value=${l}>${l}</option>`)}
            </select>
            <div class="muted">${php.ok ? php.value : html`<span class="badge warn">${t('sites.php_unavailable', { error: php.error })}</span>`}</div>
          </td>
          <td><input type="checkbox" checked=${s.secure} title=${t('sites.secure_hint')}
            onChange=${(e) => act(api('PUT', `/sites/${s.name}/secure`, { secure: e.target.checked }), reload)} /></td>
          <td class="mono">${s.path}</td>
          <td class="row">
            <button onClick=${() => open(s, 'browser')}>${t('sites.open')}</button>
            <button onClick=${() => open(s, 'folder')}>${t('sites.open_folder')}</button>
            <button onClick=${() => open(s, 'terminal')}>${t('sites.open_terminal')}</button>
            ${s.kind === 'linked' && html`<button onClick=${() => confirm(t('common.confirm_unlink', { name: s.name })) && act(api('DELETE', `/sites/${s.name}`), reload)}>${t('sites.unlink')}</button>`}
          </td></tr>`;
      })}</table>`}
    <p class="muted">${t('sites.secure_hint')}</p>
    <h2>${t('sites.link_title')}</h2>
    <div class="row">
      <input type="text" placeholder=${t('sites.link_path')} value=${path} onInput=${(e) => setPath(e.target.value)} />
      <input type="text" placeholder=${t('sites.link_name')} value=${name} onInput=${(e) => setName(e.target.value)} style="min-width:180px" />
      <button class="primary" disabled=${!path.trim()} onClick=${() => act(api('POST', '/sites', { path: path.trim(), name: name.trim() || null, php: null }), () => { setPath(''); setName(''); reload(); })}>${t('sites.link_button')}</button>
    </div>`;
}

function Extensions({ version }) {
  const [{ data: exts, error }, reload] = useApi(`/php/${version}/extensions`);
  if (!exts) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  return html`<p class="muted">${t('php.ext_hint')}</p><div class="ext-grid">
    ${exts.map((x) => html`<label>
      <input type="checkbox" checked=${x.loaded} disabled=${!x.toggleable}
        onChange=${(e) => act(api('PUT', `/php/${version}/extensions/${x.name}`, { enabled: e.target.checked }), reload)} />
      ${' '}${x.name}${!x.toggleable && html` <span class="muted">(${t('php.ext_builtin')})</span>`}</label>`)}
  </div>`;
}

function Php({ startJob }) {
  const [{ data, error }, reload] = useApi('/php', 5000);
  const [available, setAvailable] = useState(null);
  const [extOpen, setExtOpen] = useState(null);
  if (!data) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const loadAvailable = () => api('GET', '/php/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true));
  const install = (version) => api('POST', '/php/install', { version }).then((r) => startJob(r.job, () => { reload(); loadAvailable(); }), (e) => showToast(e.message, true));
  return html`
    <h1>${t('php.title')}</h1>
    ${data.installed.length === 0 ? html`<p class="muted">${t('php.empty')}</p>` : html`<table>
      ${data.installed.map((v) => html`<tr>
        <td class="big">${v.version} ${v.default && html`<span class="badge ok">${t('php.default')}</span>`}</td>
        <td class="muted">${t('php.workers', { count: v.workers })}</td>
        <td class="row">
          ${!v.default && html`<button onClick=${() => act(api('PUT', '/php/default', { version: v.version.split('.').slice(0, 2).join('.') }), reload)}>${t('php.set_default')}</button>`}
          <button onClick=${() => act(api('POST', `/php/${v.version}/restart`), reload)}>${t('php.restart')}</button>
          <button onClick=${() => setExtOpen(extOpen === v.version ? null : v.version)}>${t('php.extensions')}</button>
        </td></tr>
        ${extOpen === v.version && html`<tr><td colspan="3"><${Extensions} version=${v.version} /></td></tr>`}`)}
    </table>`}
    <h2>${t('php.available')}</h2>
    ${available === null ? html`<button onClick=${loadAvailable}>${t('php.load_available')}</button>` : html`<table>
      ${available.map((a) => html`<tr><td class="big">${a.version}</td><td>${a.installed ? html`<span class="badge ok">${t('common.installed')}</span>`
        : html`<button class="primary" onClick=${() => install(a.version)}>${t('common.install')}</button>`}</td></tr>`)}
    </table>`}`;
}

function Services({ startJob }) {
  const [{ data, error }, reload] = useApi('/mariadb', 3000);
  const [available, setAvailable] = useState(null);
  if (!data) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const loadAvailable = () => api('GET', '/mariadb/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true));
  const install = (version) => api('POST', '/mariadb/install', { version }).then((r) => startJob(r.job, () => { reload(); loadAvailable(); }), (e) => showToast(e.message, true));
  const major = (v) => v.split('.').slice(0, 2).join('.');
  return html`
    <h1>${t('nav.services')}</h1>
    <div class="card"><h3>${t('services.mariadb')}</h3>
      ${data.installed.length === 0 ? html`<p class="muted">${t('services.not_installed')}</p>` : html`
        <div class="row">
          <span class="big">${data.running ? html`${data.running.version} <span class="badge ok">${data.running.state}</span>` : t('common.stopped')}</span>
          ${data.running ? html`<button onClick=${() => act(api('POST', '/mariadb/stop'), reload)}>${t('common.stop')}</button>`
            : html`<button class="primary" onClick=${() => act(api('POST', '/mariadb/start'), reload)}>${t('common.start')}</button>`}
        </div>
        <p class="muted">${t('services.port')}: 127.0.0.1:${data.port} · ${t('services.datadir')}: <span class="mono">${data.data_dir ?? '—'}</span></p>
        <h2>${t('services.installed')}</h2>
        <table>${data.installed.map((v) => html`<tr><td class="big">${v}</td><td>${data.active === major(v)
          ? html`<span class="badge ok">${t('services.active')}</span>`
          : html`<button onClick=${() => act(api('PUT', '/mariadb/active', { version: major(v) }), reload)}>${t('services.use')}</button>`}</td></tr>`)}</table>
        <p class="muted">${t('services.switch_note')}</p>`}
      <h2>${t('services.available')}</h2>
      ${available === null ? html`<button onClick=${loadAvailable}>${t('services.load_available')}</button>` : html`<table>
        ${available.map((a) => html`<tr><td class="big">${a.major}</td>
          <td>${a.lts && html`<span class="badge">${t('services.lts')}</span>`} ${a.eol && html`<span class="muted">${t('services.eol', { date: a.eol })}</span>`} ${a.eol_passed && html`<span class="badge warn">${t('services.eol_passed')}</span>`}</td>
          <td>${a.installed ? html`<span class="badge ok">${t('common.installed')}</span>` : html`<button class="primary" onClick=${() => install(a.major)}>${t('common.install')}</button>`}</td></tr>`)}
      </table>`}
    </div>`;
}


// ------------------------------------------------------------------ M9: performance

const HISTORY = 60; // seconds of process history kept in the browser
const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const PALETTE = ['--accent', '--ok', '--warn', '--fail', '--muted'];
const fmtMs = (v) => (v == null ? '—' : v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`);

// A uPlot line chart that follows `data` ([x, ...series]) and resizes with its box.
function Chart({ title, labels, data }) {
  const box = useRef(null);
  const plot = useRef(null);
  const key = labels.join('|');
  const [failure, setFailure] = useState(null);
  useEffect(() => {
    const opts = {
      width: box.current.clientWidth,
      height: 180,
      scales: { x: { time: false } },
      legend: { show: true },
      axes: [{ stroke: css('--muted'), grid: { stroke: css('--border') } }, { stroke: css('--muted'), grid: { stroke: css('--border') } }],
      series: [{ label: 's' }, ...labels.map((label, i) => ({ label, stroke: css(PALETTE[i % PALETTE.length]), width: 2, spanGaps: false }))],
    };
    try {
      plot.current = new uPlot(opts, data, box.current);
      setFailure(null);
    } catch (e) {
      setFailure(String(e));
      return;
    }
    const onResize = () => plot.current?.setSize({ width: box.current.clientWidth, height: 180 });
    addEventListener('resize', onResize);
    return () => {
      removeEventListener('resize', onResize);
      plot.current?.destroy();
      plot.current = null;
    };
  }, [key]);
  useEffect(() => {
    plot.current?.setData(data);
  }, [data]);
  return html`<div class="card"><h3>${title}</h3>${failure && html`<span class="badge fail">${failure}</span>`}<div ref=${box}></div></div>`;
}

function Performance() {
  const [host, setHost] = useState('');
  const [withStatic, setWithStatic] = useState(false);
  const [live, setLive] = useState(null);
  const [procHistory, setProcHistory] = useState([]); // [{t, groups: {name: {cpu, kb}}}]
  const [visible, setVisible] = useState(document.visibilityState === 'visible');
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const onVis = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onVis);
    const clock = setInterval(() => setNow(Date.now()), 500);
    return () => {
      document.removeEventListener('visibilitychange', onVis);
      clearInterval(clock);
    };
  }, []);

  // One stream while the tab is visible; closing it stops all sampling in the daemon.
  useEffect(() => {
    if (!visible) return;
    const source = new EventSource(`/api/v1/metrics/stream?host=${encodeURIComponent(host)}&static=${withStatic ? 1 : 0}`);
    source.onmessage = (e) => {
      const m = JSON.parse(e.data);
      setLive(m);
      const groups = {};
      for (const p of m.processes) {
        const g = (groups[p.name] ??= { cpu: 0, kb: 0 });
        g.cpu += p.cpu_percent;
        g.kb += p.memory_kb ?? 0;
      }
      setProcHistory((h) => [...h.slice(-(HISTORY - 1)), { t: m.requests.generated_ms, groups }]);
    };
    return () => source.close();
  }, [host, withStatic, visible]);

  if (!live) return html`<h1>${t('perf.title')}</h1><p>${t('common.loading')}</p>`;
  const r = live.requests;
  const x = r.per_second.map((_, i) => i - r.per_second.length + 1);
  const ago = Math.max(0, (now - r.generated_ms) / 1000).toFixed(1) + ' s';
  const names = [...new Set(procHistory.flatMap((h) => Object.keys(h.groups)))].sort();
  const px = procHistory.map((_, i) => i - procHistory.length + 1);
  const maxCount = Math.max(1, ...r.histogram.map(([, c]) => c));

  return html`
    <h1>${t('perf.title')}</h1>
    <div class="row">
      <select value=${host} onChange=${(e) => { setHost(e.target.value); setProcHistory([]); }}>
        <option value="">${t('perf.all_sites')}</option>
        ${r.hosts.map((h) => html`<option value=${h}>${h}</option>`)}
      </select>
      <label><input type="checkbox" checked=${withStatic} onChange=${(e) => setWithStatic(e.target.checked)} /> ${t('perf.include_static')}</label>
      <span class="muted">${visible ? t('perf.live', { ago }) : t('perf.paused')}</span>
    </div>
    <p>${t('perf.summary', { window: r.window_secs, total: r.total, p50: fmtMs(r.p50_ms), p95: fmtMs(r.p95_ms), p99: fmtMs(r.p99_ms) })}<br />
      <span class="muted">${t('perf.status', { s2: r.status.s2xx, s3: r.status.s3xx, s4: r.status.s4xx, s5: r.status.s5xx })}</span></p>
    <div class="cards charts">
      <${Chart} title=${t('perf.rps')} labels=${['req/s']} data=${[x, r.per_second]} />
      <${Chart} title=${t('perf.latency')} labels=${[t('perf.p50'), t('perf.p95')]} data=${[x, r.p50_per_second, r.p95_per_second]} />
      ${names.length > 0 && html`<${Chart} title=${t('perf.cpu')} labels=${names}
        data=${[px, ...names.map((n) => procHistory.map((h) => h.groups[n]?.cpu ?? null))]} />`}
    </div>
    <h2>${t('perf.histogram')}</h2>
    <div class="hist">${r.histogram.map(([label, count]) => html`<div class="hist-row">
      <span class="mono">${label}</span><span class="bar" style=${`width:${(count / maxCount) * 100}%`}></span><span>${count}</span></div>`)}</div>
    <h2>${t('perf.slowest')}</h2>
    ${r.slowest.length === 0 ? html`<p class="muted">${t('perf.none', { window: r.window_secs })}</p>` : html`<table>
      <tr><th>${t('perf.route')}</th><th>${t('perf.count')}</th><th>${t('perf.avg')}</th><th>p95</th><th>${t('perf.max')}</th></tr>
      ${r.slowest.map((s) => html`<tr><td class="mono">${s.method} ${s.host}${s.route}</td><td>${s.count}</td><td>${fmtMs(s.avg_ms)}</td><td>${fmtMs(s.p95_ms)}</td><td>${fmtMs(s.max_ms)}</td></tr>`)}
    </table>
    <p class="muted">${t('perf.slowest_note', { n: 2000 })}</p>`}
    <h2>${t('perf.processes')}</h2>
    <table><tr><th></th><th>pid</th><th>${t('perf.cpu')}</th><th>${t('perf.ram')}</th></tr>
      ${live.processes.map((p) => html`<tr><td>${p.name}</td><td class="mono">${p.pid}</td><td>${p.cpu_percent.toFixed(1)}%</td><td>${mb(p.memory_kb)}</td></tr>`)}
    </table>`;
}

// ------------------------------------------------------------------ M9: logs

const MAX_LOG_LINES = 5000;

function Logs() {
  const [{ data: sources, error }] = useApi('/logs');
  const [source, setSource] = useState('daemon');
  const [lines, setLines] = useState([]);
  const [follow, setFollow] = useState(true);
  const [filter, setFilter] = useState('');
  const box = useRef(null);

  useEffect(() => {
    setLines([]);
    const es = new EventSource(`/api/v1/logs/stream?source=${encodeURIComponent(source)}&lines=500`);
    es.onmessage = (e) => {
      const chunk = JSON.parse(e.data);
      setLines((old) => {
        const next = chunk.reset ? [...old, t('logs.rotated'), ...chunk.lines] : [...old, ...chunk.lines];
        return next.length > MAX_LOG_LINES ? next.slice(-MAX_LOG_LINES) : next;
      });
    };
    return () => es.close();
  }, [source]);

  // `sources` too: lines can arrive before the log box exists.
  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [lines, follow, filter, sources]);

  if (!sources) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const needle = filter.toLowerCase();
  const shown = needle ? lines.filter((l) => l.toLowerCase().includes(needle)) : lines;
  const current = sources.find((s) => s.id === source);
  return html`
    <h1>${t('logs.title')}</h1>
    <div class="row">
      <select value=${source} onChange=${(e) => setSource(e.target.value)}>
        ${sources.map((s) => html`<option value=${s.id}>${s.label}</option>`)}
      </select>
      <input type="text" placeholder=${t('logs.filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} style="min-width:220px" />
      <label><input type="checkbox" checked=${follow} onChange=${(e) => setFollow(e.target.checked)} /> ${t('logs.follow')}</label>
      <span class="muted">${t('logs.lines', { count: shown.length })}</span>
    </div>
    <p class="muted mono">${current?.path}</p>
    <div class="log logview mono" ref=${box}>${shown.length ? shown.join('\n') : t('logs.empty')}</div>`;
}

// Page registry: a new feature = one entry here + its API endpoints.
const PAGES = [
  { id: 'overview', label: 'nav.overview', view: Overview },
  { id: 'sites', label: 'nav.sites', view: Sites },
  { id: 'php', label: 'nav.php', view: Php },
  { id: 'services', label: 'nav.services', view: Services },
  { id: 'performance', label: 'nav.performance', view: Performance },
  { id: 'logs', label: 'nav.logs', view: Logs },
];

// The dashboard window lives outside the daemon's process job (so stopping
// PHPev never closes the user's browser), so it closes itself when PHPev stops.
// window.close() is only allowed while the window has a single history entry,
// hence navigation with location.replace() below.
function useDaemonWatch() {
  const [stopped, setStopped] = useState(false);
  useEffect(() => {
    const events = new EventSource('/api/v1/events');
    events.onerror = async () => {
      // EventSource also errors on a hiccup and retries; confirm first. A 401
      // means a new daemon run (new token): this page cannot continue either.
      const alive = await fetch('/i18n/en.json', { cache: 'no-store' }).then((r) => r.ok, () => false);
      if (alive) return;
      events.close();
      window.close();
      setStopped(true); // reached only if the browser refused to close the window
    };
    return () => events.close();
  }, []);
  return stopped;
}

function App() {
  const current = () => location.hash.replace(/^#\//, '') || 'overview';
  const [page, setPage] = useState(current());
  const [job, setJob] = useState(null);
  const stopped = useDaemonWatch();
  useEffect(() => {
    const onHash = () => setPage(current());
    addEventListener('hashchange', onHash);
    return () => removeEventListener('hashchange', onHash);
  }, []);
  const go = (e, id) => {
    e.preventDefault();
    location.replace(`#/${id}`);
  };
  if (stopped) {
    return html`<div class="overlay"><div class="dialog"><h1>${t('app.stopped')}</h1><p>${t('app.stopped_hint')}</p></div></div>`;
  }
  const active = PAGES.find((p) => p.id === page) ?? PAGES[0];
  const startJob = (id, after) => setJob({ id, after });
  return html`<div class="layout">
    <nav><div class="brand"><picture><source srcset="/logo-dark.svg" media="(prefers-color-scheme: dark)" /><img src="/logo-light.svg" alt=${t('app.title')} /></picture></div>
      ${PAGES.map((p) => html`<a href="#/${p.id}" onClick=${(e) => go(e, p.id)} class=${p.id === active.id ? 'active' : ''}>${t(p.label)}</a>`)}</nav>
    <main><${active.view} startJob=${startJob} /></main>
    ${job && html`<${JobDialog} id=${job.id} onClose=${() => { job.after?.(); setJob(null); }} />`}
    <${Toast} />
  </div>`;
}

const lang = (navigator.language || 'en').slice(0, 2);
fetch(`/i18n/${lang}.json`)
  .then((r) => (r.ok ? r.json() : fetch('/i18n/en.json').then((x) => x.json())))
  .then((s) => {
    STRINGS = s;
    render(html`<${App} />`, document.getElementById('app'));
  });
