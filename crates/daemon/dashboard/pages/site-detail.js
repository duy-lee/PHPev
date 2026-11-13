// The page of one site: header, then Overview / Requests / Logs / Database.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Switch, Select, Tabs, SetRow, Empty, Loading } from '../ui/controls.js';
import { Chart } from '../ui/chart.js';
import { LogEntries, parse as parseEntries, TONE, levelLabel } from '../ui/entries.js';
import { siteHref } from './sites.js';
import { prefs } from '../ui/prefs.js';
import { DatabaseCard, Database } from './site-database.js';
import { EditSite } from './site-settings.js';

const host = (url) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
const line = (v) => v.split('.').slice(0, 2).join('.');
// Metrics key requests by host without the port.
const metricsHost = (url) => host(url).split(':')[0];

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
  return html`<section class="card" style="overflow: hidden">
    <div class="card-head" style="padding-bottom: 12px"><h2>${t('site.latest_logs')}</h2><span class="grow"></span>
      <a href="#" class="small" style="font-weight: 600" onClick=${(e) => { e.preventDefault(); onOpen(); }}>${t('site.view_all')}</a></div>
    ${entries === null && html`<div class="logline muted">${t('common.loading')}</div>`}
    ${entries?.length === 0 && html`<div class="logline muted">${t('site.no_entries')}</div>`}
    ${entries?.map((x) => html`<div class="logline">
      <span class="pill ${TONE[x.level] ?? ''}">${levelLabel(x.level)}</span>
      <span class="m" title=${x.message}>${x.message}</span><span class="small muted mono">${x.time}</span></div>`)}
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

// The last entry of the Composer list leads to Services › Composer, where versions are installed.
const MANAGE_COMPOSER = '#manage';

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
    { value: MANAGE_COMPOSER, label: t('site.composer_manage'), divider: true },
  ];
  const pick = (version) => {
    if (version === MANAGE_COMPOSER) return location.replace('#/services/composer');
    act(api('PUT', `/sites/${encodeURIComponent(s.name)}/composer`, { version: version || null }), reload);
  };
  return html`<${SetRow} icon="package" title="Composer" desc=${composerDesc(d)} tone=${d.composer_error ? 'warn' : ''}>
    <${Select} label=${t('site.composer_version')} value=${pinned} options=${options} onChange=${pick} />
  </${SetRow}>`;
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
        : html`<${Select} label=${t('site.php_version')} value=${s.php_pinned ?? ''} options=${options}
            onChange=${(v) => act(api('PUT', `/sites/${s.name}/php`, { version: v || null }), reload)} />`}
    </${SetRow}>
    <${SetRow} icon="lock" title="HTTPS" desc=${s.secure ? (d.ca_trusted ? t('site.https_trusted') : t('site.https_untrusted')) : t('site.https_off')}>
      <${Switch} checked=${s.secure} label="HTTPS" onChange=${(on) => act(api('PUT', `/sites/${s.name}/secure`, { secure: on }), reload)} />
    </${SetRow}>
    <${ComposerRow} d=${d} reload=${reload} />
    ${php.ok && html`<${SetRow} icon="blocks" title=${t('php.extensions')} desc=${t('site.ext_desc', { version: line(php.value) })}>
      <a class="btn quiet sm" href="#/php" onClick=${(e) => { e.preventDefault(); location.replace('#/php'); }}>${t('site.manage')}</a>
    </${SetRow}>`}
    ${php.ok && html`<${SetRow} icon="restart" title=${t('site.workers')}
      desc=${d.workers ? t('site.workers_running', { count: d.workers, mb: mb(d.workers_memory_kb) }) : t('home.no_pools')}>
      <button class="btn quiet sm" onClick=${() => act(api('POST', `/php/${php.value}/restart`), reload)}>${t('site.restart')}</button>
    </${SetRow}>`}
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

export function SiteDetail({ name, tab, sub, onChanged, startJob }) {
  const [{ data: d, error }, reloadDetail] = useApi(`/sites/${encodeURIComponent(name)}`, 15000);
  useEffect(() => rememberRecent(name), [name]);
  if (!d) return html`<div class="site-detail"><${Loading} error=${error} /></div>`;
  const s = d.site;
  const reload = () => { reloadDetail(); onChanged?.(); };
  const go = (id) => location.replace(siteHref(name, id));
  const renamed = (to) => {
    rememberRecent(to, name);
    onChanged?.();
    location.replace(siteHref(to, tab));
  };
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
    ...(d.site_databases.length > 0 ? [{ id: 'database', label: t('site.tab_database') }] : []),
  ];
  const current = tabs.some((x) => x.id === tab) ? tab : 'overview';
  const eyebrow = [
    s.framework.version ? `${s.framework.name} ${s.framework.version}` : s.framework.name,
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
          <${EditSite} d=${d} reload=${reload} onRenamed=${renamed} />
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
        <${DatabaseCard} d=${d} onOpen=${(db) => go(`database/${encodeURIComponent(db)}`)} onAdded=${(db) => { reload(); go(`database/${encodeURIComponent(db)}`); }} />
        <${LatestLogs} logs=${d.logs} path=${d.site.path} onOpen=${() => go('logs')} />
      </div>
      <div class="col-side">
        <${Activity} siteHost=${metricsHost(s.url)} />
        <${Health} siteName=${s.name} checks=${d.checks} startJob=${startJob} reload=${reload} />
        <div class="row-flex" style="padding: 0 6px">
          <span class="grow small muted">${t('site.unlink_hint')}</span>
          <button class="btn link-danger sm" onClick=${unlink}>${t('sites.unlink')}</button></div>
      </div>
    </div>`}
    ${current === 'requests' && html`<${Requests} siteHost=${metricsHost(s.url)} />`}
    ${current === 'logs' && (d.logs.length
      ? html`<${LogEntries} sources=${d.logs.map((l) => ({ ...l, label: l.label.replace(`${s.name} · `, '') }))} initial=${defaultLog(d.logs)} />`
      : html`<section class="card"><${Empty} icon="logs" title=${t('site.no_logs')} /></section>`)}
    ${current === 'database' && html`<${Database} d=${d} current=${sub} startJob=${startJob} onChanged=${reload} onPick=${(db) => go(`database/${encodeURIComponent(db)}`)} />`}
  </div>`;
}
