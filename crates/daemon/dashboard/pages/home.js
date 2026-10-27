// Home: is everything running, the sites, and what needs attention.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb } from '../format.js';
import { act } from '../ui/feedback.js';
import { Icon } from '../icons.js';
import { SetRow, Tabs, Loading } from '../ui/controls.js';
import { prefs } from '../ui/prefs.js';
import { Mark, LinkDialog, siteHref } from './sites.js';

const go = (hash) => (e) => {
  e?.preventDefault();
  location.replace(hash);
};
const line = (v) => v.split('.').slice(0, 2).join('.');
const host = (url) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');

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

function SiteCard({ site }) {
  const php = res(site.php);
  return html`<a href=${siteHref(site.name)} class="scard" onClick=${go(siteHref(site.name))}>
    <span class="top">
      <${Mark} site=${site} />
      <span class="grow"><b>${site.name}</b><span class="mono small muted">${host(site.url)}</span></span>
      ${site.requests_10m > 0 && html`<span class="dot ok" role="img" aria-label=${t('home.active')}></span>`}
    </span>
    <span class="row-flex" style="gap: 6px">
      <span class="pill">${site.framework.name}</span>
      ${php.ok
        ? html`<span class="pill ${php.value.startsWith('5') || php.value.startsWith('7') ? 'warn' : ''}">PHP ${line(php.value)}</span>`
        : html`<span class="pill danger">${t('home.no_php')}</span>`}
      ${site.secure && html`<${Icon} name="lock" size=${15} label="HTTPS" />`}
      <span class="grow"></span>
      <span class="small muted">${site.requests_10m ? t('home.requests', { count: site.requests_10m }) : t('home.idle')}</span>
    </span>
  </a>`;
}

function GetStarted({ hasPhp, hasDb, hasSites, onLink }) {
  const steps = [
    { done: hasPhp, title: t('home.step_php'), desc: t('home.step_php_hint'), action: html`<a class="btn primary sm" href="#/php" onClick=${go('#/php')}>${t('home.install_php')}</a>` },
    { done: hasDb, title: t('home.step_db'), desc: t('home.step_db_hint'), action: html`<a class="btn sm" href="#/services" onClick=${go('#/services')}>${t('home.install_mariadb')}</a>` },
    { done: hasSites, title: t('home.step_site'), desc: t('home.step_site_hint'), action: html`<button class="btn sm" onClick=${onLink}>${t('home.choose_folder')}</button>` },
  ];
  const next = steps.findIndex((s) => !s.done);
  return html`<section class="card getting-started">
    <div class="gs-intro">
      <div class="eyebrow" style="color: var(--accent)">${t('home.get_started')}</div>
      <h2 class="display">${t('home.three_steps')}</h2>
      <p class="muted">${t('home.no_admin')}</p>
    </div>
    <div class="gs-steps">
      ${steps.map((s, i) => html`<div class="gs-step">
        <span class="gs-n ${s.done ? 'done' : i === next ? 'next' : ''}">${s.done ? html`<${Icon} name="check" size=${15} stroke=${2.4} />` : i + 1}</span>
        <span class="grow"><b>${s.title}</b><span class="small muted" style="display: block">${s.desc}</span></span>
        ${!s.done && i === next && s.action}
      </div>`)}
    </div>
  </section>`;
}

function Services({ status, reload }) {
  const r = status.router;
  const db = status.mariadb;
  const workers = status.php_pools.reduce((n, p) => n + p.workers.length, 0);
  const poolKb = status.php_pools.reduce((a, p) => a + p.workers.reduce((b, w) => b + (w.memory_kb ?? 0), 0), 0);
  return html`<section class="card">
    <div class="card-head"><h2>${t('home.services')}</h2><span class="grow"></span>
      <a href="#/services" class="small" style="font-weight: 600" onClick=${go('#/services')}>${t('home.manage')}</a></div>
    <${SetRow} icon="globe" title=${t('home.web_server')} desc=${r.error ?? r.listening.join(' · ')} tone=${r.error ? 'warn' : ''}>
      <span class="dot ${r.error ? 'danger' : 'ok'}"></span>
    </${SetRow}>
    <${SetRow} icon="database" title=${db ? `MariaDB ${db.version}` : 'MariaDB'} desc=${db ? `127.0.0.1:${db.port} · ${mb(db.memory_kb)}` : t('common.stopped')}>
      ${db
        ? html`<button class="btn quiet sm" onClick=${() => act(api('POST', '/mariadb/stop'), reload)}>${t('common.stop')}</button>`
        : html`<button class="btn sm" onClick=${() => act(api('POST', '/mariadb/start'), reload)}>${t('common.start')}</button>`}
    </${SetRow}>
    <${SetRow} icon="php" title=${t('home.php_workers')}
      desc=${workers ? status.php_pools.map((p) => `${line(p.version)} × ${p.workers.length}`).join(' · ') : t('home.no_pools')}>
      ${workers > 0 && html`<span class="small muted">${mb(poolKb)}</span>`}
    </${SetRow}>
    <${SetRow} icon="lock" title="HTTPS" desc=${r.https_error ?? (r.https_listening.length ? t('home.https_on') : t('home.https_off'))} tone=${r.https_error ? 'warn' : ''}>
      <span class="dot ${r.https_error ? 'danger' : r.https_listening.length ? 'ok' : ''}"></span>
    </${SetRow}>
  </section>`;
}

function Memory({ status }) {
  const parts = [
    ['MariaDB', status.mariadb?.memory_kb ?? 0],
    [t('home.php_workers'), status.php_pools.reduce((a, p) => a + p.workers.reduce((b, w) => b + (w.memory_kb ?? 0), 0), 0)],
    ['PHPev', status.memory_kb ?? 0],
  ];
  const total = parts.reduce((a, [, kb]) => a + kb, 0);
  const max = Math.max(1, ...parts.map(([, kb]) => kb));
  return html`<section class="card pad">
    <div class="row-flex" style="justify-content: space-between; margin-bottom: 14px">
      <h2 style="font-size: 15px">${t('home.memory')}</h2><span class="small muted">${t('home.memory_total', { mb: mb(total) })}</span></div>
    <div class="stack" style="gap: 12px">
      ${parts.map(([name, kb]) => html`<div>
        <div class="row-flex small" style="justify-content: space-between; margin-bottom: 6px"><span>${name}</span><span class="muted">${mb(kb)}</span></div>
        <div class="meter"><i style=${`width: ${Math.max(2, (kb / max) * 100)}%`}></i></div>
      </div>`)}
    </div>
  </section>`;
}

const CHECK_ICON = { ok: 'check', warn: 'warning', fail: 'error' };

function Health({ checks }) {
  const problems = checks.filter((c) => c.status !== 'ok');
  return html`<section class="card checklist" style="overflow: hidden">
    <div class="card-head" style="padding-bottom: 10px"><h2>${t('site.health')}</h2><span class="grow"></span>
      <span class="pill ${problems.length ? 'warn' : 'ok'}">${t('home.checks_count', { fine: checks.length - problems.length, total: checks.length })}</span></div>
    ${problems.map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status]} title=${c.title} tone="warn"
      desc=${[...c.details, c.hint ? `→ ${c.hint}` : ''].filter(Boolean).join(' · ')} />`)}
    ${checks.length > problems.length && html`<${SetRow} icon="check" tone="ok" title=${t('home.checks_passed', { count: checks.length - problems.length })} desc=${t('home.checks_fine_hint')} />`}
  </section>`;
}

function UpdateBanner({ update, reload }) {
  return html`<section class="card update-banner">
    <${SetRow} icon="download" title=${t('home.update_title', { version: update.available })} desc=${t('home.update_hint', { current: update.current })}>
      <button class="btn quiet sm" onClick=${() => act(api('POST', '/update/skip'), reload)}>${t('home.update_skip')}</button>
      <a class="btn sm" href=${update.url} target="_blank" rel="noreferrer">${t('home.update_open')}<${Icon} name="open" size=${14} /></a>
    </${SetRow}>
  </section>`;
}

export function Home() {
  const [{ data: overview, error }, reloadOverview] = useApi('/overview', 5000);
  const [{ data: sites }, reloadSites] = useApi('/sites', 5000);
  const [{ data: php }] = useApi('/php', 15000);
  const [{ data: mariadb }] = useApi('/mariadb', 15000);
  const [{ data: update }, reloadUpdate] = useApi('/update', 600000);
  const [order, setOrder] = useState(prefs.get('homeSort') ?? 'recent');
  const [linking, setLinking] = useState(false);
  if (!overview || !sites) return html`<div class="page-head"><h1>${t('nav.home')}</h1></div><${Loading} error=${error} />`;
  const s = overview.status;
  const problems = overview.checks.filter((c) => c.status !== 'ok').length;
  const hasPhp = (php?.installed.length ?? 1) > 0;
  const hasDb = (mariadb?.installed.length ?? 1) > 0;
  const fresh = !hasPhp || !hasDb || sites.length === 0;
  const sort = (o) => {
    prefs.set('homeSort', o);
    setOrder(o);
  };
  const today = new Date().toLocaleDateString(LANG, { weekday: 'long', day: 'numeric', month: 'long' });
  return html`
    <div class="page-head">
      <div class="grow">
        <div class="eyebrow">${today}</div>
        <h1>${problems ? t('home.attention', { count: problems }) : t('home.all_running')}</h1>
        <p>${t('home.subtitle', {
          count: sites.length,
          tld: s.router.tld,
          db: s.mariadb ? `MariaDB ${line(s.mariadb.version)}` : t('home.no_db'),
          php: php?.default ? `PHP ${php.default}` : t('home.no_default_php'),
        })}</p>
      </div>
      <div class="row-flex">
        <a class="btn" href="#/php" onClick=${go('#/php')}>${t('home.install_php')}</a>
        <button class="btn primary" onClick=${() => setLinking(true)}><${Icon} name="plus" size=${16} />${t('sites.link_title')}</button>
      </div>
    </div>

    ${update?.available && html`<${UpdateBanner} update=${update} reload=${reloadUpdate} />`}
    ${fresh && html`<${GetStarted} hasPhp=${hasPhp} hasDb=${hasDb} hasSites=${sites.length > 0} onLink=${() => setLinking(true)} />`}

    <div class="cols">
      <section class="col-main">
        <div class="row-flex">
          <h2 class="section-title">${t('nav.sites')}</h2><span class="small muted">${sites.length}</span><span class="grow"></span>
          ${sites.length > 1 && html`<${Tabs} role="radiogroup" label=${t('home.sort')} value=${order} onChange=${sort}
            tabs=${[{ id: 'recent', label: t('home.recent') }, { id: 'name', label: t('home.name') }, { id: 'active', label: t('home.active') }]} />`}
        </div>
        <div class="site-grid">
          ${sortSites(sites, order).map((site) => html`<${SiteCard} key=${site.name} site=${site} />`)}
          <button class="add-card" onClick=${() => setLinking(true)}><${Icon} name="plus" />${t('sites.link_title')}</button>
        </div>
      </section>
      <div class="col-side">
        <${Services} status=${s} reload=${reloadOverview} />
        <${Memory} status=${s} />
        <${Health} checks=${overview.checks} />
      </div>
    </div>
    ${linking && html`<${LinkDialog} onClose=${() => setLinking(false)} onLinked=${reloadSites} />`}`;
}
