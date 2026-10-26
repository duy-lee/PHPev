import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';

export function Overview() {
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
