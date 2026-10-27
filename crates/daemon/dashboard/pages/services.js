import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { mb } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { SetRow, Empty, Loading } from '../ui/controls.js';

const major = (v) => v.split('.').slice(0, 2).join('.');

export function Services({ startJob }) {
  const [{ data, error }, reload] = useApi('/mariadb', 3000);
  const [available, setAvailable] = useState(null);
  if (!data) return html`<div class="page-head"><h1>${t('nav.services')}</h1></div><${Loading} error=${error} />`;
  const loadAvailable = () => api('GET', '/mariadb/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true));
  const install = (version) => api('POST', '/mariadb/install', { version }).then((r) => startJob(r.job, () => { reload(); loadAvailable(); }), (e) => showToast(e.message, true));
  const remove = async (v) => {
    // The databases of a major are only unused once its last release is gone.
    const last = !data.installed.some((o) => o !== v && major(o) === major(v));
    const answer = await confirmAction({
      title: t('services.confirm_remove', { version: v }),
      body: last ? t('services.remove_keeps_data', { major: major(v) }) : t('services.remove_other_release'),
      confirmLabel: t('common.remove'),
      danger: true,
      choice: last ? { label: t('services.delete_data_choice', { major: major(v) }), confirmWord: 'delete' } : null,
    });
    if (answer) act(api('DELETE', `/mariadb/${v}`, { delete_data: answer.choice }), reload);
  };
  const running = data.running;
  return html`
    <div class="page-head"><div class="grow"><h1>${t('nav.services')}</h1><p>${t('services.subtitle')}</p></div></div>

    <section class="card">
      <div class="card-head"><h2>${t('services.mariadb')}</h2><span class="grow"></span>
        ${data.installed.length > 0 && (running
          ? html`<span class="pill ok"><span class="dot ok"></span>${t('common.running')}</span>`
          : html`<span class="pill">${t('common.stopped')}</span>`)}</div>
      ${data.installed.length === 0
        ? html`<${Empty} icon="database" title=${t('services.not_installed')}>${t('services.not_installed_hint')}</${Empty}>`
        : html`
          <${SetRow} icon="database" title=${running ? `MariaDB ${running.version}` : t('services.mariadb')}
            desc=${running ? `127.0.0.1:${data.port} · ${mb(running.memory_kb)}` : t('services.stopped_hint')}>
            ${running
              ? html`<button class="btn quiet sm" onClick=${() => act(api('POST', '/mariadb/stop'), reload)}>${t('common.stop')}</button>`
              : html`<button class="btn primary sm" onClick=${() => act(api('POST', '/mariadb/start'), reload)}>${t('common.start')}</button>`}
          </${SetRow}>
          <${SetRow} icon="folder" title=${t('services.datadir')} desc=${data.data_dir ?? '—'} />`}
    </section>

    ${data.installed.length > 0 && html`<section class="card">
      <div class="card-head"><h2>${t('services.installed')}</h2></div>
      ${data.installed.map((v) => html`<${SetRow} icon="database" title=${`MariaDB ${v}`}
        desc=${data.active === major(v) ? t('services.active') : t('services.inactive')}>
        ${data.active === major(v)
          ? html`<span class="pill accent">${t('services.active')}</span>`
          : html`<button class="btn quiet sm" onClick=${() => act(api('PUT', '/mariadb/active', { version: major(v) }), reload)}>${t('services.use')}</button>
            <button class="btn danger-quiet sm" onClick=${() => remove(v)}>${t('common.remove')}</button>`}
      </${SetRow}>`)}
      <div class="card-foot"><span class="small muted">${t('services.switch_note')}</span></div>
    </section>`}

    <section class="card">
      <div class="card-head"><h2>${t('services.available')}</h2><span class="grow"></span>
        ${available === null && html`<button class="btn sm" onClick=${loadAvailable}>${t('services.load_available')}</button>`}</div>
      ${available === null
        ? html`<p class="small muted" style="margin: 0; padding: 0 22px 18px">${t('services.available_hint')}</p>`
        : available.map((a) => html`<${SetRow} icon="download" title=${`MariaDB ${a.major}`}
            desc=${a.eol ? t('services.eol', { date: a.eol }) : ''}>
            ${a.lts && html`<span class="pill">${t('services.lts')}</span>`}
            ${a.eol_passed && html`<span class="pill warn">${t('services.eol_passed')}</span>`}
            ${a.installed
              ? html`<span class="pill ok">${t('common.installed')}</span>`
              : html`<button class="btn sm" onClick=${() => install(a.major)}>${t('common.install')}</button>`}
          </${SetRow}>`)}
    </section>`;
}
