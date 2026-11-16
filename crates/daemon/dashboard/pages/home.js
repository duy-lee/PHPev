// Home: is everything running, the sites, and what needs attention.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb } from '../format.js';
import { act } from '../ui/feedback.js';
import { Icon } from '../icons.js';
import { SetRow, Tabs, Empty, Loading, Spark } from '../ui/controls.js';
import { prefs } from '../ui/prefs.js';
import { SiteCell, siteHref, startLinking } from './sites.js';

const go = (hash) => (e) => {
  e?.preventDefault();
  location.replace(hash);
};
const line = (v) => v.split('.').slice(0, 2).join('.');
const num = (n) => n.toLocaleString(LANG);
/** PHP 7 and older no longer get security fixes. */
export const oldPhp = (v) => /^[57]\./.test(v);

function recentNames() {
  try {
    return JSON.parse(prefs.get('recentSites') ?? '[]');
  } catch {
    return [];
  }
}

function sortSites(sites, order) {
  const list = sites.slice();
  if (order === 'name') return list.sort((a, b) => a.name.localeCompare(b.name));
  if (order === 'active') return list.sort((a, b) => b.requests_10m - a.requests_10m || a.name.localeCompare(b.name));
  const recent = recentNames();
  const rank = (s) => (recent.includes(s.name) ? recent.indexOf(s.name) : recent.length);
  return list.sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
}

/** First run: what is left to do before the first site opens. */
function GetStarted({ hasPhp, hasDb, hasSites }) {
  const steps = [
    { done: hasPhp, title: t('home.step_php'), desc: t('home.step_php_hint'),
      action: html`<a class="btn primary sm" href="#/php" onClick=${go('#/php')}>${t('home.install_php')}</a>` },
    { done: hasDb, title: t('home.step_db'), desc: t('home.step_db_hint'), optional: true,
      action: html`<a class="btn sm" href="#/services/mariadb" onClick=${go('#/services/mariadb')}>${t('home.install_mariadb')}</a>` },
    { done: hasSites, title: t('home.step_site'), desc: t('home.step_site_hint'),
      action: html`<button class="btn sm" onClick=${startLinking}>${t('home.choose_folder')}</button>` },
  ];
  const next = steps.findIndex((s) => !s.done);
  return html`<section class="card steps" aria-label=${t('home.three_steps')}>
    ${steps.map((s, i) => html`<div class="setrow">
      <span class="stepn ${s.done ? 'done' : i === next ? 'next' : ''}">${s.done ? html`<${Icon} name="check" size=${13} stroke=${2.4} />` : i + 1}</span>
      <span class="row-text"><b>${s.title}</b><span>${s.desc}</span></span>
      ${s.done ? html`<span class="small muted">${t('home.step_done')}</span>`
        : i === next ? s.action : s.optional && html`<span class="small muted">${t('home.optional')}</span>`}
    </div>`)}
  </section>`;
}

function Stats({ sites, status, traffic }) {
  const active = sites.filter((s) => s.requests_10m > 0).length;
  const workers = status.php_pools.reduce((n, p) => n + p.workers.length, 0);
  const poolKb = status.php_pools.reduce((a, p) => a + p.workers.reduce((b, w) => b + (w.memory_kb ?? 0), 0), 0);
  const db = status.mariadb;
  return html`<div class="stats">
    <div class="stat"><div class="k"><${Icon} name="globe" size=${14} />${t('nav.sites')}</div>
      <div class="v">${sites.length}<small>${t('home.active_count', { count: active })}</small></div></div>
    <div class="stat"><div class="k"><${Icon} name="activity" size=${14} />${t('home.requests_10m')}</div>
      <div class="v">${traffic ? num(traffic.total) : '—'}${traffic?.total > 0 && html`<${Spark} values=${traffic.per_second} />`}</div></div>
    <div class="stat"><div class="k"><${Icon} name="cpu" size=${14} />${t('home.php_workers')}</div>
      <div class="v">${workers}${workers > 0 && html`<small>${mb(poolKb)}</small>`}</div></div>
    <div class="stat"><div class="k"><${Icon} name="database" size=${14} />${db ? `MariaDB ${line(db.version)}` : 'MariaDB'}</div>
      <div class="v text"><span class="status"><span class="dot ${db ? 'ok' : ''}"></span>${db ? t('common.running') : t('common.stopped')}</span>
        ${db && html`<small>:${db.port} · ${mb(db.memory_kb)}</small>`}</div></div>
  </div>`;
}

function SitesTable({ sites }) {
  const [order, setOrder] = useState(prefs.get('homeSort') ?? 'recent');
  const sort = (o) => {
    prefs.set('homeSort', o);
    setOrder(o);
  };
  return html`<section class="card clip">
    <div class="card-head"><h2>${t('nav.sites')}</h2><span class="sub">${sites.length}</span><span class="grow"></span>
      ${sites.length > 1 && html`<${Tabs} role="radiogroup" label=${t('home.sort')} value=${order} onChange=${sort}
        tabs=${[{ id: 'recent', label: t('home.recent') }, { id: 'name', label: t('home.name') }, { id: 'active', label: t('home.active') }]} />`}
    </div>
    ${sites.length === 0
      ? html`<${Empty} icon="folder-open" title=${t('sites.empty_title')}>
          <p>${t('sites.empty')}</p>
          <button class="btn" onClick=${startLinking}><${Icon} name="plus" size=${15} />${t('sites.link_title')}</button>
        </${Empty}>`
      : html`<div class="scroll-x"><table class="table">
        <thead><tr><th>${t('home.col_site')}</th><th>PHP</th><th>${t('home.col_trend')}</th><th class="num">${t('home.col_requests')}</th><th class="fit"></th></tr></thead>
        <tbody>${sortSites(sites, order).map((s) => {
          const php = res(s.php);
          return html`<tr class="click" key=${s.name} onClick=${go(siteHref(s.name))}>
            <td><a href=${siteHref(s.name)} class="cell-link" onClick=${go(siteHref(s.name))}><${SiteCell} site=${s} /></a></td>
            <td>${php.ok ? html`<span class="badge mono ${oldPhp(php.value) ? 'warn' : ''}">${line(php.value)}</span>` : html`<span class="badge danger">${t('home.no_php')}</span>`}</td>
            <td><${Spark} values=${s.requests_trend?.length ? s.requests_trend : [0, 0]} width=${88} idle=${!s.requests_10m} /></td>
            <td class="num ${s.requests_10m ? '' : 'muted'}">${s.requests_10m ? num(s.requests_10m) : t('home.idle')}</td>
            <td><span class="chev"><${Icon} name="chevron-right" size=${15} /></span></td>
          </tr>`;
        })}</tbody>
      </table></div>`}
  </section>`;
}

function Services({ status, https, reload }) {
  const r = status.router;
  const db = status.mariadb;
  const workers = status.php_pools.reduce((n, p) => n + p.workers.length, 0);
  const poolKb = status.php_pools.reduce((a, p) => a + p.workers.reduce((b, w) => b + (w.memory_kb ?? 0), 0), 0);
  const ports = [r.port, ...(r.https_listening.length ? [r.https_port] : [])].map((p) => `:${p}`).join(' · ');
  return html`<section class="card">
    <div class="card-head"><h2>${t('home.services')}</h2><span class="grow"></span>
      <a class="btn ghost sm" href="#/services/mariadb" onClick=${go('#/services/mariadb')}>${t('home.manage')}</a></div>
    <${SetRow} icon="globe" title=${t('home.web_server')} desc=${r.error ?? ports} tone=${r.error ? 'warn' : ''}>
      <span class="status"><span class="dot ${r.error ? 'danger' : 'ok'}"></span>${r.error ? t('common.error_short') : t('common.running')}</span>
    </${SetRow}>
    <${SetRow} icon="database" title=${db ? `MariaDB ${db.version}` : 'MariaDB'} desc=${db ? `127.0.0.1:${db.port} · ${mb(db.memory_kb)}` : t('common.stopped')}>
      ${db
        ? html`<button class="btn sm" onClick=${() => act(api('POST', '/mariadb/stop'), reload)}><${Icon} name="square" size=${12} />${t('common.stop')}</button>`
        : html`<button class="btn sm" onClick=${() => act(api('POST', '/mariadb/start'), reload)}><${Icon} name="play" size=${12} />${t('common.start')}</button>`}
    </${SetRow}>
    <${SetRow} icon="cpu" title=${t('home.php_workers')}
      desc=${workers ? status.php_pools.map((p) => `${line(p.version)} × ${p.workers.length}`).join(' · ') : t('home.no_pools')}>
      ${workers > 0 && html`<span class="meta">${mb(poolKb)}</span>`}
    </${SetRow}>
    <${SetRow} icon="lock" title="HTTPS" tone=${r.https_error ? 'warn' : ''}
      desc=${r.https_error ?? (r.https_listening.length ? t('home.https_sites', { count: https }) : t('home.https_off'))}>
      <span class="status"><span class="dot ${r.https_error ? 'danger' : r.https_listening.length ? 'ok' : ''}"></span>
        ${r.https_error ? t('common.error_short') : r.https_listening.length ? t('common.on') : t('common.off')}</span>
    </${SetRow}>
  </section>`;
}

function Memory({ status }) {
  const parts = [
    [t('home.php_workers'), status.php_pools.reduce((a, p) => a + p.workers.reduce((b, w) => b + (w.memory_kb ?? 0), 0), 0), 'var(--accent)'],
    ['MariaDB', status.mariadb?.memory_kb ?? 0, 'color-mix(in srgb, var(--accent) 45%, var(--panel))'],
    ['PHPev', status.memory_kb ?? 0, 'var(--line-2)'],
  ];
  const total = parts.reduce((a, [, kb]) => a + kb, 0);
  return html`<section class="card">
    <div class="card-head"><h2>${t('home.memory')}</h2><span class="grow"></span><span class="sub tnum">${t('home.memory_total', { mb: mb(total) })}</span></div>
    <div class="card-body">
      <div class="bars" aria-hidden="true">${parts.map(([, kb, color]) => kb > 0 && html`<i style=${`width: ${(kb / Math.max(1, total)) * 100}%; background: ${color}`}></i>`)}</div>
      <div class="legend">${parts.map(([name, kb, color]) => html`<div><span class="sq" style=${`background: ${color}`}></span>${name}<span class="meta">${mb(kb)}</span></div>`)}</div>
    </div>
  </section>`;
}

const CHECK_ICON = { ok: 'circle-check', warn: 'triangle-alert', fail: 'circle-x' };

function Health({ checks }) {
  const problems = checks.filter((c) => c.status !== 'ok');
  const fine = checks.length - problems.length;
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.health')}</h2><span class="grow"></span>
      <span class="badge ${problems.length ? 'warn' : 'ok'}">${problems.length ? t('site.to_fix', { count: problems.length }) : t('site.all_good')}</span></div>
    ${problems.map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status]} title=${c.title} tone=${c.status === 'fail' ? 'err' : 'warn'}
      desc=${[...c.details, c.hint ? `→ ${c.hint}` : ''].filter(Boolean).join(' · ')} />`)}
    ${fine > 0 && html`<${SetRow} icon="circle-check" tone="ok" title=${t('home.checks_passed', { count: fine })} desc=${t('home.checks_fine_hint')} />`}
  </section>`;
}

function UpdateBanner({ update, reload }) {
  return html`<div class="banner" role="status"><${Icon} name="arrow-down-to-line" />
    <div class="grow"><b>${t('home.update_title', { version: update.available })}</b> <span>${t('home.update_hint', { current: update.current })}</span></div>
    <button class="btn ghost sm" onClick=${() => act(api('POST', '/update/skip'), reload)}>${t('home.update_skip')}</button>
    <a class="btn sm" href=${update.url} target="_blank" rel="noreferrer">${t('home.update_open')}<${Icon} name="arrow-up-right" size=${14} /></a>
  </div>`;
}

export function Home() {
  const [{ data: overview, error }, reloadOverview] = useApi('/overview', 5000);
  const [{ data: sites }] = useApi('/sites', 5000);
  const [{ data: traffic }] = useApi('/metrics/requests?window=600', 5000);
  const [{ data: php }] = useApi('/php', 15000);
  const [{ data: mariadb }] = useApi('/mariadb', 15000);
  const [{ data: update }, reloadUpdate] = useApi('/update', 600000);
  const head = (title, sub, actions) => html`<div class="page-head">
    <div class="grow"><h1>${title}</h1>${sub && html`<p>${sub}</p>`}</div>${actions && html`<div class="acts">${actions}</div>`}</div>`;
  if (!overview || !sites) return html`${head(t('nav.home'))}<${Loading} error=${error} />`;
  const s = overview.status;
  const problems = overview.checks.filter((c) => c.status !== 'ok').length;
  const hasPhp = (php?.installed.length ?? 1) > 0;
  const hasDb = (mariadb?.installed.length ?? 1) > 0;
  const fresh = !hasPhp || sites.length === 0;
  const actions = html`<a class="btn" href="#/php" onClick=${go('#/php')}><${Icon} name="download" size=${15} />${t('home.install_php')}</a>
    <button class="btn primary" onClick=${startLinking}><${Icon} name="plus" size=${15} />${t('sites.link_title')}</button>`;
  if (fresh) {
    return html`${head(t('home.welcome'), t('home.welcome_hint'))}
      ${update?.available && html`<${UpdateBanner} update=${update} reload=${reloadUpdate} />`}
      <${GetStarted} hasPhp=${hasPhp} hasDb=${hasDb} hasSites=${sites.length > 0} />
      ${sites.length > 0 && html`<${SitesTable} sites=${sites} />`}`;
  }
  return html`
    ${head(problems ? t('home.attention', { count: problems }) : t('home.all_running'), t('home.subtitle', {
      count: sites.length,
      tld: s.router.tld,
      db: s.mariadb ? `MariaDB ${line(s.mariadb.version)}` : t('home.no_db'),
      php: php?.default ? `PHP ${line(php.default)}` : t('home.no_default_php'),
    }), actions)}
    ${update?.available && html`<${UpdateBanner} update=${update} reload=${reloadUpdate} />`}
    <${Stats} sites=${sites} status=${s} traffic=${traffic} />
    <div class="cols">
      <${SitesTable} sites=${sites} />
      <div class="stack">
        <${Services} status=${s} https=${sites.filter((x) => x.secure).length} reload=${reloadOverview} />
        <${Health} checks=${overview.checks} />
        <${Memory} status=${s} />
      </div>
    </div>`;
}
