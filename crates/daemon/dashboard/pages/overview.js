import { html } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime } from '../format.js';
import { act } from '../ui/feedback.js';
import { Icon } from '../icons.js';
import { SetRow, Loading } from '../ui/controls.js';

const CHECK_TONE = { ok: 'ok', warn: 'warn', fail: 'warn' };
const CHECK_ICON = { ok: 'check', warn: 'warning', fail: 'error' };

export function Overview() {
  const [{ data, error }, reload] = useApi('/overview', 2000);
  if (!data) return html`<div class="page-head"><h1>${t('nav.overview')}</h1></div><${Loading} error=${error} />`;
  const s = data.status;
  const r = s.router;
  const dash = res(s.dashboard);
  const problems = data.checks.filter((c) => c.status !== 'ok');
  const fine = data.checks.length - problems.length;
  const workers = s.php_pools.reduce((n, p) => n + p.workers.length, 0);
  const poolKb = s.php_pools.reduce((a, p) => a + p.workers.reduce((b, w) => b + (w.memory_kb ?? 0), 0), 0);
  return html`
    <div class="page-head">
      <div class="grow">
        <h1>${problems.length ? t('overview.attention', { count: problems.length }) : t('overview.all_running')}</h1>
        <p>${t('overview.subtitle', { version: data.version, uptime: uptime(s.uptime_secs) })}</p>
      </div>
    </div>

    <div class="cards">
      <section class="card">
        <div class="card-head"><h2>${t('overview.services')}</h2></div>
        <${SetRow} icon="globe" title=${t('overview.router')}
          desc=${r.error ? r.error : `*.${r.tld} · ${r.listening.join(', ')}`} tone=${r.error ? 'warn' : ''}>
          <span class="dot ${r.error ? 'danger' : 'ok'}" role="img" aria-label=${r.error ? t('common.stopped') : t('common.running')}></span>
        </${SetRow}>
        <${SetRow} icon="lock" title=${t('overview.https')}
          desc=${r.https_error ? r.https_error : r.https_listening.length ? r.https_listening.join(', ') : t('overview.https_off')} tone=${r.https_error ? 'warn' : ''}>
          <span class="dot ${r.https_error ? 'danger' : r.https_listening.length ? 'ok' : ''}"></span>
        </${SetRow}>
        <${SetRow} icon="database" title=${s.mariadb ? `${t('overview.mariadb')} ${s.mariadb.version}` : t('overview.mariadb')}
          desc=${s.mariadb ? `127.0.0.1:${s.mariadb.port} · ${mb(s.mariadb.memory_kb)}` : t('common.stopped')}>
          ${s.mariadb
            ? html`<button class="btn quiet sm" onClick=${() => act(api('POST', '/mariadb/stop'), reload)}>${t('common.stop')}</button>`
            : html`<button class="btn sm" onClick=${() => act(api('POST', '/mariadb/start'), reload)}>${t('common.start')}</button>`}
        </${SetRow}>
        <${SetRow} icon="php" title=${t('overview.php_pools')}
          desc=${workers ? s.php_pools.map((p) => `${p.version} × ${p.workers.length}`).join(' · ') : t('overview.no_pools')}>
          ${workers > 0 && html`<span class="small muted">${mb(poolKb)}</span>`}
        </${SetRow}>
      </section>

      <section class="card">
        <div class="card-head"><h2>${t('overview.phpev')}</h2></div>
        <${SetRow} icon="info" title=${t('overview.daemon')} desc=${`v${data.version} · pid ${s.pid}`}>
          <span class="small muted">${mb(s.memory_kb)}</span>
        </${SetRow}>
        <${SetRow} icon="chart" title=${t('overview.dashboard')} desc=${dash.ok ? dash.value : dash.error} tone=${dash.ok ? '' : 'warn'} />
        <${SetRow} icon="restart" title=${t('overview.uptime_title')} desc=${uptime(s.uptime_secs)} />
      </section>
    </div>

    <section class="card checklist">
      <div class="card-head">
        <h2>${t('overview.checks')}</h2><span class="grow"></span>
        <span class="pill ${problems.length ? 'warn' : 'ok'}">${t('overview.checks_fine', { fine, total: data.checks.length })}</span>
      </div>
      ${problems.map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status]} title=${c.title} tone=${CHECK_TONE[c.status]}
        desc=${[...c.details, c.hint ? `→ ${c.hint}` : ''].filter(Boolean).join(' · ')} />`)}
      ${fine > 0 && html`<details>
        <summary class="setrow" style="cursor: pointer; list-style: none"><span class="row-icon"><${Icon} name="check-circle" /></span>
          <span class="row-text"><b>${t('overview.checks_ok', { count: fine })}</b><span>${t('overview.checks_show')}</span></span></summary>
        ${data.checks.filter((c) => c.status === 'ok').map((c) => html`<${SetRow} icon="check" title=${c.title} tone="ok"
          desc=${c.details.join(' · ')} />`)}
      </details>`}
    </section>`;
}
