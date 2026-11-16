// The page of one site: header, then Overview / Requests / Logs / Database.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, fmtMs, size } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Switch, Select, Tabs, SetRow, Empty, Loading, MenuButton, AreaChart, CopyButton } from '../ui/controls.js';
import { LogEntries, parse as parseEntries, TONE, levelLabel } from '../ui/entries.js';
import { Mark, siteHref, siteMenu, openSite } from './sites.js';
import { prefs } from '../ui/prefs.js';
import { Database } from './site-database.js';
import { EditDialog, shown } from './site-settings.js';

const host = (url) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
const line = (v) => v.split('.').slice(0, 2).join('.');
const oldPhp = (v) => /^[57]\./.test(v);
// Metrics key requests by host without the port.
const metricsHost = (url) => host(url).split(':')[0];
const go = (hash) => (e) => {
  e?.preventDefault();
  location.replace(hash);
};

/** Remembers the sites opened here, newest first, for Home's "Recent" order; `old` is a name it replaces. */
function rememberRecent(name, old) {
  let list = [];
  try {
    list = JSON.parse(prefs.get('recentSites') ?? '[]');
  } catch {
    list = [];
  }
  prefs.set('recentSites', JSON.stringify([name, ...list.filter((n) => n !== name && n !== old)].slice(0, 20)));
}

/** The log shown first: the newest Laravel log, else the site's newest log, else PHP errors. */
function defaultLog(logs) {
  const site = logs.filter((l) => l.id.startsWith('site:'));
  const laravel = site.filter((l) => /:laravel[^:]*$/.test(l.id));
  return (laravel.at(-1) ?? site.at(-1) ?? logs[0])?.id;
}

/** Windows paths compare without case or slash direction; the trailing slash keeps `shop` apart from `shop2`. */
const slashes = (s) => s.toLowerCase().replaceAll('\\', '/');
const under = (text, dir) => slashes(text).includes(`${slashes(dir).replace(/\/+$/, '')}/`);

function LatestLogs({ logs, path, onOpen }) {
  const [entries, setEntries] = useState(null);
  const source = defaultLog(logs);
  const file = logs.find((l) => l.id === source)?.path;
  // A site's own log is all its own; the PHP error log is shared by every site on that PHP version.
  const shared = source?.startsWith('php:');
  useEffect(() => {
    if (!source) return;
    setEntries(null);
    const es = new EventSource(`/api/v1/logs/stream?source=${encodeURIComponent(source)}&lines=${shared ? 1000 : 200}`);
    es.onmessage = (e) => {
      const lines = JSON.parse(e.data).lines;
      // Laravel and PHP error entries alike; the lines of a stack trace belong to the entry above.
      const found = parseEntries([], lines).filter((x) => x.level && (!shared || under([x.message, ...x.detail].join('\n'), path)));
      setEntries(found.slice(-4).reverse());
      es.close();
    };
    return () => es.close();
  }, [source, path]);
  if (!source) return null;
  return html`<section class="card clip">
    <div class="card-head"><h2>${t('site.latest_logs')}</h2>${file && html`<span class="sub mono ellipsis" title=${file}>${file.split(/[\\/]/).pop()}</span>`}
      <span class="grow"></span><button class="btn ghost sm" onClick=${onOpen}>${t('site.view_all')}</button></div>
    ${entries === null && html`<div class="logline muted">${t('common.loading')}</div>`}
    ${entries?.length === 0 && html`<div class="logline muted">${t('site.no_entries')}</div>`}
    ${entries?.map((x) => html`<div class="logline">
      <span class="badge level ${TONE[x.level] ?? ''}">${levelLabel(x.level)}</span>
      <span class="m" title=${x.message}>${x.message}</span><span class="entry-time">${x.time}</span></div>`)}
  </section>`;
}

function phpSourceText(d, php) {
  const version = php.ok ? php.value : '?';
  if (d.php_source === 'pinned') return t('site.php_pinned', { version });
  if (d.php_source === 'version-file') return t('site.php_version_file', { version });
  return t('site.php_default', { version });
}

function composerDesc(d) {
  if (d.composer_error) return d.composer_error;
  if (!d.composer) return t('site.composer_missing');
  if (d.composer_source === 'site') return t('site.composer_pinned', { version: d.composer });
  if (d.composer_source === 'newest') return t('site.composer_newest', { version: d.composer });
  return t('site.composer_default', { version: d.composer });
}

// The last entry of the version lists leads to the page where versions are installed.
const MANAGE = '#manage';

function ComposerRow({ d, reload }) {
  const s = d.site;
  const pinned = s.composer_pinned ?? '';
  const versions = d.composer_versions;
  // A pin made with `phpev composer pin 2.8` names a line, not one of the installed versions.
  const pinOnly = pinned && !versions.some((c) => c.version === pinned);
  const options = [
    { value: '', label: t('site.composer_automatic') },
    ...(pinOnly ? [{ value: pinned, label: pinned }] : []),
    ...versions.map((c) => ({
      value: c.version,
      hint: c.runs ? '' : t('site.composer_option_needs', { php: c.min_php }),
      disabled: !c.runs && c.version !== pinned,
    })),
    { value: MANAGE, label: t('site.composer_manage'), divider: true },
  ];
  const pick = (version) => {
    if (version === MANAGE) return location.replace('#/services/composer');
    act(api('PUT', `/sites/${encodeURIComponent(s.name)}/composer`, { version: version || null }), reload);
  };
  return html`<${SetRow} icon="package" title="Composer" desc=${composerDesc(d)} tone=${d.composer_error ? 'warn' : ''}>
    <${Select} label=${t('site.composer_version')} value=${pinned} options=${options} onChange=${pick} />
  </${SetRow}>`;
}

/** MariaDB is shared by every site: its version and state, and the databases of this site. */
function MariaDbRow({ d, onOpenDb }) {
  const [{ data: db }] = useApi('/mariadb', 5000);
  const running = db?.running;
  const names = d.site_databases.map((x) => x.database);
  const version = running?.version ?? d.mariadb_version ?? db?.active;
  return html`<div class="setrow">
    <span class="row-icon"><${Icon} name="database" /></span>
    <span class="row-text"><b>MariaDB${version && html` <span class="mono t2 light">${version}</span>`}</b>
      <span>${running ? t('site.mariadb_on', { port: db.port }) : t('site.db_stopped')}
        ${names.length > 0 && html` · ${names.map((n, i) => html`${i > 0 && ', '}<a class="mono" href=${siteHref(d.site.name, `database/${encodeURIComponent(n)}`)}
          onClick=${(e) => { e.preventDefault(); onOpenDb(n); }}>${n}</a>`)}`} · ${t('site.mariadb_shared')}</span></span>
    <span class="status"><span class="dot ${running ? 'ok' : ''}"></span>${running ? t('common.running') : t('common.stopped')}</span>
    ${running
      ? html`<button class="btn sm" disabled title=${t('site.mariadb_restart_soon')}><${Icon} name="rotate-cw" size=${13} />${t('site.restart')}</button>`
      : html`<a class="btn sm" href="#/services/mariadb" onClick=${go('#/services/mariadb')}>${t('site.manage')}</a>`}
  </div>`;
}

function Environment({ d, reload, onEdit, onOpenDb }) {
  const s = d.site;
  const php = res(s.php);
  const options = [
    { value: '', label: t('site.default') },
    ...d.php_lines.map((l) => ({ value: l, label: l })),
    { value: MANAGE, label: t('site.php_manage'), divider: true },
  ];
  const pickPhp = (v) => (v === MANAGE ? location.replace('#/php') : act(api('PUT', `/sites/${s.name}/php`, { version: v || null }), reload));
  const old = php.ok && oldPhp(php.value);
  // Sites whose .env names PHPev's MariaDB, or with a database added by hand.
  const usesDb = d.site_databases.length > 0 || d.databases.some((x) => x.local);
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.environment')}</h2><span class="grow"></span>
      <button class="btn ghost sm" onClick=${onEdit}><${Icon} name="pencil" size=${13} />${t('site.edit')}</button></div>
    <${SetRow} icon=${old ? 'triangle-alert' : 'code-xml'} title="PHP" tone=${!php.ok || old ? 'warn' : ''}
      desc=${php.ok ? `${phpSourceText(d, php)}${old ? ` · ${t('site.php_eol')}` : ''}` : t('sites.php_unavailable', { error: php.error })}>
      ${d.php_source === 'version-file'
        ? html`<span class="badge mono">.php-version</span>`
        : html`<${Select} label=${t('site.php_version')} value=${s.php_pinned ?? ''} options=${options} onChange=${pickPhp} />`}
    </${SetRow}>
    <${SetRow} icon="lock" title="HTTPS" desc=${s.secure ? (d.ca_trusted ? t('site.https_trusted') : t('site.https_untrusted')) : t('site.https_off')}>
      <${Switch} checked=${s.secure} label="HTTPS" onChange=${(on) => act(api('PUT', `/sites/${s.name}/secure`, { secure: on }), reload)} />
    </${SetRow}>
    <${ComposerRow} d=${d} reload=${reload} />
    ${usesDb && html`<${MariaDbRow} d=${d} onOpenDb=${onOpenDb} />`}
    <div class="setrow">
      <span class="row-icon"><${Icon} name="folder" /></span>
      <span class="row-text"><b>${t('site.docroot')}</b><span class="mono">${shown(d.docroot)}${!d.docroot_picked && html` <span class="muted">· ${t('site.docroot_auto')}</span>`}</span></span>
      <button class="btn sm" onClick=${onEdit}>${t('site.change')}</button>
    </div>
    ${php.ok && html`<${SetRow} icon="puzzle" title=${t('php.extensions')} desc=${t('site.ext_desc', { version: line(php.value) })}>
      <a class="btn sm" href="#/php" onClick=${go('#/php')}>${t('site.manage')}</a>
    </${SetRow}>`}
    ${php.ok && html`<${SetRow} icon="cpu" title=${t('site.workers')}
      desc=${d.workers ? t('site.workers_running', { count: d.workers, mb: mb(d.workers_memory_kb) }) : t('home.no_pools')}>
      <button class="btn sm" disabled=${!d.workers} onClick=${() => act(api('POST', `/php/${php.value}/restart`), reload)}>
        <${Icon} name="rotate-cw" size=${13} />${t('site.restart')}</button>
    </${SetRow}>`}
  </section>`;
}

function useTraffic(siteHost, everyMs) {
  return useApi(`/metrics/requests?window=600&host=${encodeURIComponent(siteHost)}`, everyMs);
}

function Activity({ siteHost }) {
  const [{ data: r }] = useTraffic(siteHost, 5000);
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.activity')}</h2><span class="grow"></span><span class="sub">${t('site.last_10_min')}</span></div>
    ${!r ? html`<div class="card-body"><${Loading} height=${80} /></div>` : html`
      <div class="card-body" style="padding-bottom: 12px">
        <div class="row-flex" style="align-items: baseline"><span class="bignum">${r.total.toLocaleString(LANG)}</span><span class="muted">${t('site.requests')}</span></div>
        ${r.total > 0
          ? html`<div style="margin-top: 10px"><${AreaChart} values=${r.per_second} /></div>`
          : html`<p class="muted" style="margin: 8px 0 0">${t('site.no_requests')}</p>`}
      </div>
      ${r.total > 0 && html`<div class="mini-stats">
        <div><b>${fmtMs(r.p50_ms)}</b><span>${t('site.median')}</span></div>
        <div><b>${fmtMs(r.p95_ms)}</b><span>p95</span></div>
        <div><b class=${r.status.s5xx ? 'danger-t' : ''}>${r.status.s5xx}</b><span>${t('site.server_errors')}</span></div>
      </div>`}`}
  </section>`;
}

const CHECK_ICON = { ok: 'circle-check', warn: 'triangle-alert', fail: 'circle-x' };

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
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.health')}</h2><span class="grow"></span>
      <span class="badge ${problems.length ? 'warn' : 'ok'}">${problems.length ? t('site.to_fix', { count: problems.length }) : t('site.all_good')}</span></div>
    ${checks.map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status] ?? 'info'} title=${c.title} desc=${c.hint ?? ''}
      tone=${c.status === 'ok' ? 'ok' : c.status === 'fail' ? 'err' : 'warn'}>
      ${c.status !== 'ok' && c.fix && html`<button class="btn sm" onClick=${() => fix(siteName, c, startJob, reload)}>${t('site.fix')}</button>`}
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
    <button class="btn square" title=${chosenName ? t('site.open_in_editor', { editor: chosenName }) : t('site.choose_editor')}
      aria-label=${chosenName ? t('site.open_in_editor', { editor: chosenName }) : t('site.choose_editor')} onClick=${main}><${Icon} name="code" size=${15} /></button>
    <button class="btn" aria-label=${t('site.choose_editor')} title=${t('site.choose_editor')} aria-haspopup="menu" aria-expanded=${menu ? 'true' : 'false'}
      onClick=${() => setMenu(!menu)}><${Icon} name="chevron-down" size=${14} /></button>
    ${menu && html`
      <div class="menu-catch" onClick=${() => setMenu(false)}></div>
      <div class="menu" role="menu" aria-label=${t('site.choose_editor')} style="position: absolute; top: 34px; right: 0; width: 240px">
        ${info.editors.length === 0 && html`<div class="mh">${t('site.no_editor', { list: info.supported.join(', ') })}</div>`}
        ${info.editors.map((e) => html`<button role="menuitemradio" aria-checked=${e.id === info.chosen ? 'true' : 'false'} class="mi"
          onClick=${() => openWith(e.id)}>
          <span class="tick">${e.id === info.chosen && html`<${Icon} name="check" size=${14} />`}</span><span class="lab grow">${e.name}</span></button>`)}
      </div>`}
  </div>`;
}

/** How many of the newest requests the list asks for (the daemon keeps 2000 for all sites). */
const REQUEST_LIMIT = 200;
const statusTone = (st) => (st >= 500 ? 'danger-t' : st >= 400 ? 'warn-t' : st >= 300 ? 't2' : 'ok-t');
const statusClass = { all: () => true, '4xx': (st) => st >= 400 && st < 500, '5xx': (st) => st >= 500 };

/** The site's newest requests, from the daemon's memory (lost on restart). */
function Requests({ siteHost }) {
  const [klass, setKlass] = useState('all');
  const [path, setPath] = useState('');
  const [withStatic, setWithStatic] = useState(true);
  const [{ data, error }] = useApi(`/metrics/recent?host=${encodeURIComponent(siteHost)}&static=${withStatic ? 1 : 0}&limit=${REQUEST_LIMIT}`, 2000);
  if (!data) return html`<${Loading} error=${error} />`;
  const count = (k) => data.filter((r) => statusClass[k](r.status)).length;
  const needle = path.trim().toLowerCase();
  const rows = data.filter((r) => statusClass[klass](r.status) && (!needle || r.path.toLowerCase().includes(needle)));
  const time = (ms) => new Date(ms).toLocaleTimeString(LANG, { hour12: false });
  return html`<section class="card clip">
    <div class="tools">
      <${Tabs} role="radiogroup" label=${t('site.req_status')} value=${klass} onChange=${setKlass}
        tabs=${[{ id: 'all', label: t('logs.all'), count: data.length }, { id: '4xx', label: '4xx', count: count('4xx') }, { id: '5xx', label: '5xx', count: count('5xx') }]} />
      <label class="field sm" style="width: 240px"><${Icon} name="search" size=${13} />
        <input placeholder=${t('site.req_filter')} aria-label=${t('site.req_filter')} value=${path} spellcheck="false" onInput=${(e) => setPath(e.target.value)} /></label>
      <span class="grow"></span><span class="live"><span class="dot"></span>${t('site.req_live')}</span>
      <label class="row-flex small t2" style="gap: 6px; flex-wrap: nowrap">
        <${Switch} checked=${withStatic} label=${t('perf.include_static')} onChange=${setWithStatic} />${t('perf.static_short')}</label>
    </div>
    ${rows.length === 0 ? html`<${Empty} icon="activity" title=${data.length ? t('logs.no_match') : t('site.req_none')} />` : html`<div class="scroll-x"><table class="table dense">
      <thead><tr><th>${t('site.req_time')}</th><th>${t('site.req_method')}</th><th>${t('site.req_path')}</th><th>${t('site.req_status')}</th>
        <th class="num">${t('site.req_duration')}</th><th class="num">${t('site.size')}</th></tr></thead>
      <tbody>${rows.map((r) => html`<tr>
        <td class="mono muted">${time(r.at_ms)}</td><td class="mono t2">${r.method}</td>
        <td class="mono" style="max-width: 420px; overflow: hidden; text-overflow: ellipsis" title=${r.path}>${r.path}</td>
        <td class="mono ${statusTone(r.status)}">${r.status}</td>
        <td class="num mono ${r.ms > 300 ? 'warn-t' : ''}">${fmtMs(r.ms)}</td>
        <td class="num mono muted">${r.bytes ? size(r.bytes) : '—'}</td></tr>`)}</tbody>
    </table></div>`}
    <div class="foot"><span>${t('site.req_in_memory', { count: data.length })}</span><span class="grow"></span>
      <a class="link" href="#/performance" onClick=${go('#/performance')}>${t('site.open_performance')}</a></div>
  </section>`;
}

export function SiteDetail({ name, tab, sub, onChanged, startJob }) {
  const [{ data: d, error }, reloadDetail] = useApi(`/sites/${encodeURIComponent(name)}`, 15000);
  const [editing, setEditing] = useState(false);
  useEffect(() => rememberRecent(name), [name]);
  if (!d) return html`<${Loading} error=${error} />`;
  const s = d.site;
  const reload = () => { reloadDetail(); onChanged?.(); };
  const goTab = (id) => location.replace(siteHref(name, id));
  const renamed = (to) => {
    setEditing(false);
    rememberRecent(to, name);
    onChanged?.();
    location.replace(siteHref(to, tab));
  };
  const problems = d.checks.filter((c) => c.status !== 'ok').length;
  const tabs = [
    { id: 'overview', label: t('site.tab_overview'), count: problems || null },
    { id: 'requests', label: t('site.tab_requests') },
    { id: 'logs', label: t('site.tab_logs') },
    { id: 'database', label: t('site.tab_database'), count: d.site_databases.length > 1 ? d.site_databases.length : null },
  ];
  const current = tabs.some((x) => x.id === tab) ? tab : 'overview';
  const framework = s.framework.version ? `${s.framework.name} ${s.framework.version}` : s.framework.name;
  const openDb = (db) => goTab(`database/${encodeURIComponent(db)}`);

  return html`<div class="site-detail">
    <section class="site-head">
      <${Mark} site=${s} large />
      <div class="grow">
        <h1>${s.name}${s.requests_10m > 0
          ? html`<span class="badge ok" title=${t('nav.site_active')}><span class="dot ok"></span>${t('site.active')}</span>`
          : html`<span class="badge">${t('site.idle')}</span>`}</h1>
        <div class="site-meta">
          <a class="mono" href=${s.url} target="_blank" rel="noopener">${s.secure && html`<${Icon} name="lock" size=${12} />`}${host(s.url)}</a>
          <span class="mono path"><${Icon} name="folder" size=${13} /><span class="ellipsis" title=${s.path}>${s.path}</span><${CopyButton} text=${s.path} label=${t('site.path')} /></span>
          <span>${framework}${d.workers > 0 && ` · ${t('site.workers_short', { count: d.workers })}`}</span>
        </div>
      </div>
      <div class="acts">
        <button class="btn square" aria-label=${t('sites.open_folder_long')} title=${t('sites.open_folder_long')} onClick=${() => openSite(s.name, 'folder')}>
          <${Icon} name="folder-open" /></button>
        <button class="btn square" aria-label=${t('sites.open_terminal_long')} title=${t('sites.open_terminal_long')} onClick=${() => openSite(s.name, 'terminal')}>
          <${Icon} name="square-terminal" /></button>
        <${EditorButton} siteName=${s.name} onFallback=${() => openSite(s.name, 'folder')} />
        <a class="btn primary" href=${s.url} target="_blank" rel="noopener">${t('site.open_site')}<${Icon} name="arrow-up-right" size=${15} /></a>
        <${MenuButton} label=${t('site.more')} small=${false} items=${siteMenu(s, { onEdit: () => setEditing(true), onUnlinked: () => location.replace('#/sites') })} />
      </div>
    </section>

    <${Tabs} label=${t('site.sections')} value=${current} tabs=${tabs} onChange=${goTab} />

    ${current === 'overview' && html`<div class="cols">
      <div class="stack">
        <${Environment} d=${d} reload=${reload} onEdit=${() => setEditing(true)} onOpenDb=${openDb} />
        <${LatestLogs} logs=${d.logs} path=${d.site.path} onOpen=${() => goTab('logs')} />
      </div>
      <div class="stack">
        <${Activity} siteHost=${metricsHost(s.url)} />
        <${Health} siteName=${s.name} checks=${d.checks} startJob=${startJob} reload=${reload} />
      </div>
    </div>`}
    ${current === 'requests' && html`<${Requests} siteHost=${metricsHost(s.url)} />`}
    ${current === 'logs' && (d.logs.length
      ? html`<${LogEntries} sources=${d.logs.map((l) => ({ ...l, label: l.label.replace(`${s.name} · `, '') }))} initial=${defaultLog(d.logs)} />`
      : html`<section class="card"><${Empty} icon="scroll-text" title=${t('site.no_logs')} /></section>`)}
    ${current === 'database' && html`<${Database} d=${d} current=${sub} startJob=${startJob} onChanged=${reload} onPick=${openDb} />`}
    ${editing && html`<${EditDialog} d=${d} reload=${reload} onClose=${() => setEditing(false)} onRenamed=${renamed} />`}
  </div>`;
}
