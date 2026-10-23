// PHPev dashboard (DESIGN_SPEC §14). Presentation only: every action is an
// API v1 call, so another UI (Slint, TUI) can replace this file without
// touching the daemon. Strings come from /i18n/<lang>.json.
import { html, render, useState, useEffect, useCallback } from '/vendor/htm-preact-standalone.mjs';

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

// Page registry: a new feature = one entry here + its API endpoints.
const PAGES = [
  { id: 'overview', label: 'nav.overview', view: Overview },
  { id: 'sites', label: 'nav.sites', view: Sites },
  { id: 'php', label: 'nav.php', view: Php },
  { id: 'services', label: 'nav.services', view: Services },
];

function App() {
  const current = () => location.hash.replace(/^#\//, '') || 'overview';
  const [page, setPage] = useState(current());
  const [job, setJob] = useState(null);
  useEffect(() => {
    const onHash = () => setPage(current());
    addEventListener('hashchange', onHash);
    return () => removeEventListener('hashchange', onHash);
  }, []);
  const active = PAGES.find((p) => p.id === page) ?? PAGES[0];
  const startJob = (id, after) => setJob({ id, after });
  return html`<div class="layout">
    <nav><div class="brand">${t('app.title')}</div>
      ${PAGES.map((p) => html`<a href="#/${p.id}" class=${p.id === active.id ? 'active' : ''}>${t(p.label)}</a>`)}</nav>
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
