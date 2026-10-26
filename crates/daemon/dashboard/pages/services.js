import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';

export function Services({ startJob }) {
  const [{ data, error }, reload] = useApi('/mariadb', 3000);
  const [available, setAvailable] = useState(null);
  if (!data) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const loadAvailable = () => api('GET', '/mariadb/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true));
  const install = (version) => api('POST', '/mariadb/install', { version }).then((r) => startJob(r.job, () => { reload(); loadAvailable(); }), (e) => showToast(e.message, true));
  const major = (v) => v.split('.').slice(0, 2).join('.');
  const remove = (v) => {
    if (!confirm(t('services.confirm_remove', { version: v }))) return;
    let deleteData = false;
    // The databases of a major are only unused once its last release is gone.
    if (!data.installed.some((o) => o !== v && major(o) === major(v))) {
      const typed = prompt(t('services.delete_data_prompt', { major: major(v) }), '');
      if (typed === null) return;
      deleteData = typed.trim().toLowerCase() === 'delete';
    }
    act(api('DELETE', `/mariadb/${v}`, { delete_data: deleteData }), reload);
  };
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
          : html`<button onClick=${() => act(api('PUT', '/mariadb/active', { version: major(v) }), reload)}>${t('services.use')}</button>
            <button onClick=${() => remove(v)}>${t('common.remove')}</button>`}</td></tr>`)}</table>
        <p class="muted">${t('services.switch_note')}</p>`}
      <h2>${t('services.available')}</h2>
      ${available === null ? html`<button onClick=${loadAvailable}>${t('services.load_available')}</button>` : html`<table>
        ${available.map((a) => html`<tr><td class="big">${a.major}</td>
          <td>${a.lts && html`<span class="badge">${t('services.lts')}</span>`} ${a.eol && html`<span class="muted">${t('services.eol', { date: a.eol })}</span>`} ${a.eol_passed && html`<span class="badge warn">${t('services.eol_passed')}</span>`}</td>
          <td>${a.installed ? html`<span class="badge ok">${t('common.installed')}</span>` : html`<button class="primary" onClick=${() => install(a.major)}>${t('common.install')}</button>`}</td></tr>`)}
      </table>`}
    </div>`;
}
