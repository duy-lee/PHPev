// The page of one site: header, then Overview / Requests / Logs / Database.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Switch, Picker, Tabs, SetRow, Empty, Loading } from '../ui/controls.js';
import { Chart } from '../ui/chart.js';
import { LogEntries } from '../ui/entries.js';
import { siteHref } from './sites.js';
import { prefs } from '../ui/prefs.js';

const host = (url) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
const line = (v) => v.split('.').slice(0, 2).join('.');
// Metrics key requests by host without the port.
const metricsHost = (url) => host(url).split(':')[0];

/** Remembers the sites opened here, newest first, for Home's "Recent" order. */
function rememberRecent(name) {
  let list = [];
  try {
    list = JSON.parse(prefs.get('recentSites') ?? '[]');
  } catch {
    list = [];
  }
  prefs.set('recentSites', JSON.stringify([name, ...list.filter((n) => n !== name)].slice(0, 20)));
}
const size = (bytes) => (bytes >= 1073741824 ? `${(bytes / 1073741824).toFixed(1)} GB` : bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`);
const copy = (text) => navigator.clipboard.writeText(text).then(() => showToast(t('common.copied')), () => showToast(text));

/** The log shown first: the newest Laravel log, else the site's newest log, else PHP errors. */
function defaultLog(logs) {
  const site = logs.filter((l) => l.id.startsWith('site:'));
  const laravel = site.filter((l) => /:laravel[^:]*$/.test(l.id));
  return (laravel.at(-1) ?? site.at(-1) ?? logs[0])?.id;
}

// "[2026-10-26 10:42:13] local.ERROR: message"
const LARAVEL_LINE = /^\[(\d{4}-\d\d-\d\d) (\d\d:\d\d:\d\d)\] \w+\.(\w+): (.*)$/;
const LEVEL_TONE = { ERROR: 'danger', CRITICAL: 'danger', ALERT: 'danger', EMERGENCY: 'danger', WARNING: 'warn', NOTICE: 'accent', INFO: 'accent' };

function LatestLogs({ logs, onOpen }) {
  const [entries, setEntries] = useState(null);
  const source = defaultLog(logs);
  useEffect(() => {
    if (!source) return;
    const es = new EventSource(`/api/v1/logs/stream?source=${encodeURIComponent(source)}&lines=200`);
    es.onmessage = (e) => {
      const lines = JSON.parse(e.data).lines;
      setEntries(lines.map((l) => l.match(LARAVEL_LINE)).filter(Boolean).slice(-4).reverse());
      es.close();
    };
    return () => es.close();
  }, [source]);
  if (!source) return null;
  return html`<section class="card" style="overflow: hidden">
    <div class="card-head" style="padding-bottom: 12px"><h2>${t('site.latest_logs')}</h2><span class="grow"></span>
      <a href="#" class="small" style="font-weight: 600" onClick=${(e) => { e.preventDefault(); onOpen(); }}>${t('site.view_all')}</a></div>
    ${entries === null && html`<div class="logline muted">${t('common.loading')}</div>`}
    ${entries?.length === 0 && html`<div class="logline muted">${t('site.no_entries')}</div>`}
    ${entries?.map(([, , time, level, msg]) => html`<div class="logline">
      <span class="pill ${LEVEL_TONE[level] ?? ''}">${level.charAt(0) + level.slice(1).toLowerCase()}</span>
      <span class="m" title=${msg}>${msg}</span><span class="small muted mono">${time}</span></div>`)}
  </section>`;
}

function phpSourceText(d, php) {
  const version = php.ok ? php.value : '?';
  if (d.php_source === 'pinned') return t('site.php_pinned', { version });
  if (d.php_source === 'version-file') return t('site.php_version_file', { version });
  return t('site.php_default', { version });
}

function Environment({ d, reload }) {
  const s = d.site;
  const php = res(s.php);
  const options = [{ value: '', label: t('site.default') }, ...d.php_lines.map((l) => ({ value: l, label: l }))];
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.environment')}</h2></div>
    <${SetRow} icon="php" title="PHP" desc=${php.ok ? phpSourceText(d, php) : t('sites.php_unavailable', { error: php.error })} tone=${php.ok ? '' : 'warn'}>
      ${d.php_source === 'version-file'
        ? html`<span class="pill mono">.php-version</span>`
        : html`<${Picker} label=${t('site.php_version')} value=${s.php_pinned ?? ''} options=${options}
            onChange=${(v) => act(api('PUT', `/sites/${s.name}/php`, { version: v || null }), reload)} />`}
    </${SetRow}>
    <${SetRow} icon="lock" title="HTTPS" desc=${s.secure ? (d.ca_trusted ? t('site.https_trusted') : t('site.https_untrusted')) : t('site.https_off')}>
      <${Switch} checked=${s.secure} label="HTTPS" onChange=${(on) => act(api('PUT', `/sites/${s.name}/secure`, { secure: on }), reload)} />
    </${SetRow}>
    <${SetRow} icon="package" title="Composer" desc=${t('site.composer_desc')}>
      ${d.composer ? html`<span class="mono muted">${d.composer}</span>` : html`<span class="small muted">${t('site.composer_missing')}</span>`}
    </${SetRow}>
    ${php.ok && html`<${SetRow} icon="blocks" title=${t('php.extensions')} desc=${t('site.ext_desc', { version: line(php.value) })}>
      <a class="btn quiet sm" href="#/php" onClick=${(e) => { e.preventDefault(); location.replace('#/php'); }}>${t('site.manage')}</a>
    </${SetRow}>`}
    ${php.ok && html`<${SetRow} icon="restart" title=${t('site.workers')}
      desc=${d.workers ? t('site.workers_running', { count: d.workers, mb: mb(d.workers_memory_kb) }) : t('home.no_pools')}>
      <button class="btn quiet sm" onClick=${() => act(api('POST', `/php/${php.value}/restart`), reload)}>${t('site.restart')}</button>
    </${SetRow}>`}
  </section>`;
}

function useSiteDatabase(siteName, conn) {
  const [state, setState] = useState(null);
  useEffect(() => {
    setState(null);
    if (!conn) return;
    api('GET', `/sites/${encodeURIComponent(siteName)}/database?name=${encodeURIComponent(conn.name)}`).then(setState, (e) => setState({ error: e.message }));
  }, [siteName, conn?.name, conn?.version]);
  return state;
}

/** "MAIN", or nothing for the plain DB_* connection. */
const connLabel = (db) => (db.name && db.name !== 'wordpress' ? db.name : '');

function databaseDesc(db, info) {
  if (!info) return t('common.loading');
  if (info.error) return info.error;
  if (!info.mariadb_running) return t('site.db_stopped');
  if (!info.exists) return t('site.db_missing');
  return t('site.db_size', { count: info.tables.length, size: size(info.bytes) });
}

/** Picks a .sql file, asks how to import it, then follows the import. */
async function importInto(siteName, db, startJob, done) {
  let picked;
  try {
    picked = await api('POST', `/sites/${encodeURIComponent(siteName)}/database/import/pick`);
  } catch (e) {
    return showToast(e.message, true);
  }
  if (!picked.path) return;
  const file = picked.path.split('\\').pop();
  const answer = await confirmAction({
    title: t('site.import_title', { db: db.database }),
    body: t('site.import_body', { file }),
    confirmLabel: t('site.import'),
    choice: { label: t('site.import_empty_first'), confirmWord: 'delete' },
  });
  if (!answer) return;
  api('POST', `/sites/${encodeURIComponent(siteName)}/database/import`, { name: db.name, path: picked.path, empty_first: answer.choice })
    .then((r) => startJob(r.job, done), (e) => showToast(e.message, true));
}

function exportFrom(siteName, db, startJob) {
  api('POST', `/sites/${encodeURIComponent(siteName)}/database/export`, { name: db.name })
    .then((r) => r.job && startJob(r.job), (e) => showToast(e.message, true));
}

function DatabaseCard({ d, startJob, onOpen }) {
  const db = d.databases.find((x) => x.local);
  const [version, setVersion] = useState(0);
  const info = useSiteDatabase(d.site.name, db ? { ...db, version } : null);
  const other = !db && d.databases.find((x) => x.database);
  const url = db && `mysql://${db.username || 'root'}@127.0.0.1:${info?.port ?? db.port ?? 3306}/${db.database}`;
  const status = info?.mariadb_running
    ? html`<span class="pill ok"><span class="dot ok"></span>MariaDB ${info.mariadb_version.split('.').slice(0, 2).join('.')}</span>`
    : info && html`<span class="pill">${t('site.db_stopped')}</span>`;
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.database')}</h2><span class="grow"></span>${db && status}</div>
    ${!db && html`<${SetRow} icon="database" title=${other ? other.database : t('site.db_none')}
      desc=${other ? t('site.db_elsewhere', { connection: other.connection, host: other.host }) : t('site.db_none_hint')} />`}
    ${db && html`
      <div class="setrow">
        <span class="row-icon" style="background: var(--accent-soft); color: var(--accent)"><${Icon} name="database" /></span>
        <span class="row-text"><b class="mono">${db.database}</b><span>${databaseDesc(db, info)}</span></span>
        <button class="btn quiet sm" disabled=${!info?.mariadb_running} onClick=${() => importInto(d.site.name, db, startJob, () => setVersion(version + 1))}>
          <${Icon} name="upload" size=${15} />${t('site.import')}</button>
        <button class="btn quiet sm" disabled=${!info?.exists} onClick=${() => exportFrom(d.site.name, db, startJob)}>
          <${Icon} name="download" size=${15} />${t('site.export')}</button>
      </div>
      <div class="copyline"><span class="key">${t('site.connection')}</span><span class="val mono">${url}</span>
        <button class="icon-btn ghost sm" aria-label=${t('common.copy')} title=${t('common.copy')} onClick=${() => copy(url)}><${Icon} name="copy" size=${15} /></button></div>
      <div class="copyline"><span class="key">${t('site.terminal')}</span><span class="val mono">mysql ${db.database}</span>
        <button class="icon-btn ghost sm" aria-label=${t('common.copy')} title=${t('common.copy')} onClick=${() => copy(`mysql ${db.database}`)}><${Icon} name="copy" size=${15} /></button></div>`}
  </section>`;
}

/** Requests per second over 10 minutes, as 60 bars of 10 s. */
function Sparkline({ perSecond }) {
  const buckets = [];
  for (let i = 0; i < perSecond.length; i += 10) buckets.push(perSecond.slice(i, i + 10).reduce((a, b) => a + b, 0));
  const max = Math.max(1, ...buckets);
  const w = 320;
  const h = 70;
  const step = w / Math.max(1, buckets.length - 1);
  const points = buckets.map((v, i) => `${(i * step).toFixed(1)},${(h - 4 - (v / max) * (h - 10)).toFixed(1)}`);
  return html`<svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="none" style="width: 100%; height: 70px; display: block; margin: 12px 0 14px" aria-hidden="true">
    <polygon points=${`0,${h} ${points.join(' ')} ${w},${h}`} fill="var(--chart-fill)" />
    <polyline points=${points.join(' ')} fill="none" stroke="var(--accent)" stroke-width="2" stroke-linejoin="round" vector-effect="non-scaling-stroke" />
  </svg>`;
}

function useTraffic(siteHost, everyMs) {
  return useApi(`/metrics/requests?window=600&host=${encodeURIComponent(siteHost)}`, everyMs);
}

function Activity({ siteHost }) {
  const [{ data: r }] = useTraffic(siteHost, 5000);
  return html`<section class="card pad">
    <div class="row-flex"><h2 style="font-size: 15px">${t('site.activity')}</h2><span class="grow"></span><span class="small muted">${t('site.last_10_min')}</span></div>
    ${!r ? html`<p class="muted small">${t('common.loading')}</p>` : html`
      <div class="big-number" style="margin-top: 8px">${r.total.toLocaleString(LANG)}</div>
      <div class="small muted">${t('site.requests')}</div>
      ${r.total > 0 ? html`<${Sparkline} perSecond=${r.per_second} />` : html`<p class="small muted" style="margin: 12px 0">${t('site.no_requests')}</p>`}
      <div class="row-flex" style="justify-content: space-between">
        <div class="stat"><b>${fmtMs(r.p50_ms)}</b><span>${t('site.median')}</span></div>
        <div class="stat"><b>${fmtMs(r.p95_ms)}</b><span>p95</span></div>
        <div class="stat"><b style=${r.status.s5xx ? 'color: var(--danger)' : ''}>${r.status.s5xx}</b><span>${t('site.server_errors')}</span></div>
      </div>`}
  </section>`;
}

const CHECK_ICON = { ok: 'check', warn: 'warning', fail: 'error' };

async function fix(siteName, check, startJob, reload) {
  if (check.fix.confirm) {
    const ok = await confirmAction({ title: t('site.fix_title', { title: check.title }), body: check.fix.confirm, confirmLabel: t('site.fix_run') });
    if (!ok) return;
  }
  try {
    const r = await api('POST', `/sites/${encodeURIComponent(siteName)}/fix/${encodeURIComponent(check.id)}`);
    if (r.job) startJob(r.job, reload);
    else {
      showToast(r.message);
      reload();
    }
  } catch (e) {
    showToast(e.message, true);
  }
}

function Health({ siteName, checks, startJob, reload }) {
  if (!checks.length) return null;
  const problems = checks.filter((c) => c.status !== 'ok');
  return html`<section class="card checklist" style="overflow: hidden">
    <div class="card-head" style="padding-bottom: 10px"><h2>${t('site.health')}</h2><span class="grow"></span>
      <span class="pill ${problems.length ? 'warn' : 'ok'}">${problems.length ? t('site.to_fix', { count: problems.length }) : t('site.all_good')}</span></div>
    ${checks.map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status]} title=${c.title} desc=${c.hint ?? ''} tone=${c.status === 'ok' ? 'ok' : 'warn'}>
      ${c.status !== 'ok' && c.fix && html`<button class="btn sm" style="background: var(--surface)" onClick=${() => fix(siteName, c, startJob, reload)}>${t('site.fix')}</button>`}
    </${SetRow}>`)}
  </section>`;
}

/** "Editor" opens the remembered editor; the arrow picks (and remembers) another. */
function EditorButton({ siteName, onFallback }) {
  const [info, setInfo] = useState(null);
  const [menu, setMenu] = useState(false);
  const load = () => api('GET', '/editors').then(setInfo, () => setInfo({ editors: [], chosen: null, supported: [] }));
  useEffect(() => { load(); }, []);
  const openWith = (editor) => {
    setMenu(false);
    api('POST', `/sites/${encodeURIComponent(siteName)}/open`, { target: 'editor', editor })
      .then(load, (e) => showToast(e.message, true));
  };
  const main = () => {
    if (!info) return;
    if (info.chosen) openWith(null);
    else if (info.editors.length) setMenu(true);
    else {
      showToast(t('site.no_editor', { list: info.supported.join(', ') }));
      onFallback();
    }
  };
  const chosenName = info?.editors.find((e) => e.id === info.chosen)?.name;
  return html`<div class="split" style="position: relative">
    <button class="btn" title=${chosenName ?? t('site.choose_editor')} onClick=${main}><${Icon} name="editor" size=${15} />${t('site.editor')}</button>
    <button class="btn" aria-label=${t('site.choose_editor')} title=${t('site.choose_editor')} aria-haspopup="menu" aria-expanded=${menu ? 'true' : 'false'}
      onClick=${() => setMenu(!menu)}><${Icon} name="chevron-down" size=${15} /></button>
    ${menu && html`
      <div style="position: fixed; inset: 0; z-index: 14" onClick=${() => setMenu(false)}></div>
      <div class="menu" role="menu" aria-label=${t('site.choose_editor')} style="top: 42px; left: auto; right: 0; width: 260px">
        ${info.editors.length === 0 && html`<div class="palette-empty">${t('site.no_editor', { list: info.supported.join(', ') })}</div>`}
        ${info.editors.map((e) => html`<button role="menuitemradio" aria-checked=${e.id === info.chosen ? 'true' : 'false'} class="site-item"
          style="width: 100%; border: 0; background: none; font: inherit; cursor: pointer" onClick=${() => openWith(e.id)}>
          <${Icon} name="editor" size=${16} /><span class="grow" style="text-align: left">${e.name}</span>
          ${e.id === info.chosen && html`<${Icon} name="check" size=${16} />`}</button>`)}
      </div>`}
  </div>`;
}

function Requests({ siteHost }) {
  const [{ data: r }] = useTraffic(siteHost, 2000);
  if (!r) return html`<${Loading} />`;
  const x = r.per_second.map((_, i) => i - r.per_second.length + 1);
  return html`
    <section class="card pad">
      <div class="row-flex" style="gap: 36px; align-items: flex-end">
        <div><div class="small muted">${t('site.requests_10_min')}</div><div class="big-number">${r.total.toLocaleString(LANG)}</div></div>
        <div class="stat"><b>${fmtMs(r.p50_ms)}</b><span>${t('site.median')}</span></div>
        <div class="stat"><b>${fmtMs(r.p95_ms)}</b><span>p95</span></div>
        <div class="stat"><b>${fmtMs(r.p99_ms)}</b><span>p99</span></div>
        <span class="grow"></span>
        <div class="row-flex">
          <span class="pill ok">2xx ${r.status.s2xx}</span><span class="pill">3xx ${r.status.s3xx}</span>
          <span class="pill warn">4xx ${r.status.s4xx}</span><span class="pill danger">5xx ${r.status.s5xx}</span>
        </div>
      </div>
    </section>
    <div class="charts">
      <${Chart} title=${t('perf.rps')} labels=${['req/s']} data=${[x, r.per_second]} />
      <${Chart} title=${t('perf.latency')} labels=${[t('perf.p50'), t('perf.p95')]} data=${[x, r.p50_per_second, r.p95_per_second]} />
    </div>
    <section class="card">
      <div class="card-head"><h2>${t('perf.slowest')}</h2></div>
      ${r.slowest.length === 0 ? html`<${Empty} icon="chart" title=${t('site.no_requests')} />` : html`<div class="scroll-x"><table class="table">
        <thead><tr><th>${t('perf.route')}</th><th class="num">${t('perf.count')}</th><th class="num">${t('perf.avg')}</th><th class="num">p95</th><th class="num">${t('perf.max')}</th></tr></thead>
        <tbody>${r.slowest.map((s) => html`<tr><td><span class="pill mono" style="margin-right: 8px">${s.method}</span><span class="mono">${s.route}</span></td>
          <td class="num">${s.count}</td><td class="num">${fmtMs(s.avg_ms)}</td><td class="num">${fmtMs(s.p95_ms)}</td><td class="num">${fmtMs(s.max_ms)}</td></tr>`)}</tbody>
      </table></div>`}
    </section>`;
}

function Database({ d, conn, onPick, startJob }) {
  const locals = d.databases.filter((db) => db.local);
  const found = locals.find((x) => x.name === conn) ?? locals[0];
  const [version, setVersion] = useState(0);
  const db = { ...found, version };
  const info = useSiteDatabase(d.site.name, db);
  const rows = [
    ['Host', '127.0.0.1'],
    [t('site.port'), String(info?.port ?? db.port ?? 3306)],
    [t('site.user'), db.username || 'root'],
    ['Database', db.database],
  ];
  return html`
    ${locals.length > 1 && html`<${Tabs} role="radiogroup" label=${t('site.connection')} value=${db.name} onChange=${onPick}
      tabs=${locals.map((x) => ({ id: x.name, label: connLabel(x) || x.database }))} />`}
    <div class="cols">
    <div class="col-side"><section class="card" style="overflow: hidden">
      <div class="card-head" style="padding-bottom: 10px"><h2>${t('site.connect')}</h2></div>
      <p class="small muted" style="margin: 0; padding: 0 22px 10px">${t('site.connect_hint')}</p>
      ${rows.map(([k, v]) => html`<div class="copyline"><span class="key">${k}</span><span class="val mono">${v}</span>
        <button class="icon-btn ghost sm" aria-label=${`${t('common.copy')} ${k}`} title=${t('common.copy')} onClick=${() => copy(v)}><${Icon} name="copy" size=${15} /></button></div>`)}
      <div class="copyline"><span class="key">${t('site.password')}</span><span class="val muted">${db.has_password ? t('site.password_set') : t('site.password_empty')}</span></div>
      <div class="card-foot">
        <button class="btn primary sm" disabled=${!info?.mariadb_running} onClick=${() => importInto(d.site.name, db, startJob, () => setVersion(version + 1))}>
          <${Icon} name="upload" size=${15} />${t('site.import_sql')}</button>
        <button class="btn sm" disabled=${!info?.exists} onClick=${() => exportFrom(d.site.name, db, startJob)}><${Icon} name="download" size=${15} />${t('site.export')}</button>
      </div>
    </section></div>
    <div class="col-main"><section class="card" style="overflow: hidden">
      <div class="card-head" style="padding-bottom: 6px"><h2>${t('site.tables')}</h2><span class="small muted">${databaseDesc(db, info)}</span></div>
      ${!info ? html`<div style="padding: 0 22px 18px"><${Loading} height=${120} /></div>`
        : info.error ? html`<div style="padding: 0 22px 18px"><span class="pill danger">${info.error}</span></div>`
        : info.tables.length === 0 ? html`<${Empty} icon="database" title=${databaseDesc(db, info)} />`
        : html`<div class="scroll-x"><table class="table">
          <thead><tr><th>${t('site.table')}</th><th class="num">${t('site.rows')}</th><th class="num">${t('site.size')}</th></tr></thead>
          <tbody>${info.tables.map((x) => html`<tr><td class="mono">${x.name}</td><td class="num">${x.rows.toLocaleString(LANG)}</td><td class="num">${size(x.bytes)}</td></tr>`)}</tbody>
        </table></div>`}
    </section></div>
  </div>`;
}

export function SiteDetail({ name, tab, sub, onChanged, startJob }) {
  const [{ data: d, error }, reloadDetail] = useApi(`/sites/${encodeURIComponent(name)}`, 15000);
  useEffect(() => rememberRecent(name), [name]);
  if (!d) return html`<div class="site-detail"><${Loading} error=${error} /></div>`;
  const s = d.site;
  const reload = () => { reloadDetail(); onChanged?.(); };
  const go = (id) => location.replace(siteHref(name, id));
  const open = (target) => act(api('POST', `/sites/${s.name}/open`, { target }));
  const unlink = async () => {
    const ok = await confirmAction({
      title: t('sites.unlink_title', { name: s.name }),
      body: t('common.confirm_unlink', { name: s.name }),
      confirmLabel: t('sites.unlink'),
      danger: true,
    });
    if (ok) act(api('DELETE', `/sites/${s.name}`), () => { onChanged?.(); location.replace('#/sites'); });
  };
  const problems = d.checks.filter((c) => c.status !== 'ok').length;
  const tabs = [
    { id: 'overview', label: t('site.tab_overview') },
    { id: 'requests', label: t('site.tab_requests') },
    { id: 'logs', label: t('site.tab_logs') },
    ...(d.databases.some((db) => db.local) ? [{ id: 'database', label: t('site.tab_database') }] : []),
  ];
  const current = tabs.some((x) => x.id === tab) ? tab : 'overview';
  const eyebrow = [
    s.framework.version ? `${s.framework.name} ${s.framework.version}` : s.framework.name,
    s.kind === 'linked' ? t('sites.linked') : t('sites.parked'),
    d.workers ? t('site.workers_short', { count: d.workers }) : null,
  ].filter(Boolean);

  return html`<div class="site-detail">
    <section class="site-head">
      <div class="grow" style="flex: 1 1 320px">
        <div class="eyebrow">${eyebrow.join(' · ')}</div>
        <h1>${s.name}</h1>
        <div class="row-flex" style="gap: 16px">
          <a href=${s.url} target="_blank" rel="noopener" style="display: inline-flex; align-items: center; gap: 6px; font-weight: 600">
            <${Icon} name=${s.secure ? 'lock' : 'globe'} size=${15} />${host(s.url)}</a>
          <span class="mono small muted" style="display: inline-flex; align-items: center; gap: 6px; min-width: 0; overflow-wrap: anywhere"><${Icon} name="folder" size=${15} /><span style="min-width: 0">${s.path}</span></span>
        </div>
      </div>
      <div class="row-flex">
        <a class="btn primary" href=${s.url} target="_blank" rel="noopener">${t('site.open_site')}<${Icon} name="open" size=${15} /></a>
        <${EditorButton} siteName=${s.name} onFallback=${() => open('folder')} />
        <button class="btn" onClick=${() => open('folder')}><${Icon} name="folder" size=${15} />${t('sites.open_folder')}</button>
        <button class="btn" onClick=${() => open('terminal')}><${Icon} name="terminal" size=${15} />${t('sites.open_terminal')}</button>
      </div>
    </section>

    <${Tabs} label=${t('site.sections')} value=${current} tabs=${tabs.map((x) => (x.id === 'overview' && problems ? { ...x, count: problems } : x))} onChange=${go} />

    ${current === 'overview' && html`<div class="cols">
      <div class="col-main">
        <${Environment} d=${d} reload=${reload} />
        <${DatabaseCard} d=${d} startJob=${startJob} onOpen=${(conn) => go(`database/${encodeURIComponent(conn)}`)} />
        <${LatestLogs} logs=${d.logs} onOpen=${() => go('logs')} />
      </div>
      <div class="col-side">
        <${Activity} siteHost=${metricsHost(s.url)} />
        <${Health} siteName=${s.name} checks=${d.checks} startJob=${startJob} reload=${reload} />
        ${s.kind === 'linked' && html`<div class="row-flex" style="padding: 0 6px">
          <span class="grow small muted">${t('site.unlink_hint')}</span>
          <button class="btn link-danger sm" onClick=${unlink}>${t('sites.unlink')}</button></div>`}
      </div>
    </div>`}
    ${current === 'requests' && html`<${Requests} siteHost=${metricsHost(s.url)} />`}
    ${current === 'logs' && (d.logs.length
      ? html`<${LogEntries} sources=${d.logs.map((l) => ({ ...l, label: l.label.replace(`${s.name} · `, '') }))} initial=${defaultLog(d.logs)} />`
      : html`<section class="card"><${Empty} icon="logs" title=${t('site.no_logs')} /></section>`)}
    ${current === 'database' && html`<${Database} d=${d} conn=${sub} startJob=${startJob} onPick=${(conn) => go(`database/${encodeURIComponent(conn)}`)} />`}
  </div>`;
}
