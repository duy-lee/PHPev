import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { mb } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { SetRow, DefaultStar, RemoveButton, Tabs, Empty, Loading } from '../ui/controls.js';
import { IniEditor } from '../ui/ini-editor.js';

const major = (v) => v.split('.').slice(0, 2).join('.');

/** Below a version's row, as on the PHP page: the my.ini of its major, read by the server at start. */
function VersionPanel({ version, onSaved }) {
  const [tab, setTab] = useState('ini');
  const m = major(version);
  return html`<div class="version-panel">
    <${Tabs} label=${t('services.details_of', { version })} value=${tab} onChange=${setTab} tabs=${[{ id: 'ini', label: 'my.ini' }]} />
    <div class="ini-editor">
      <p class="small muted" style="margin: 0">${t('services.my_ini_note', { major: m })}</p>
      <${IniEditor} path=${`/mariadb/${m}/ini`} label=${t('services.my_ini_label', { major: m })}
        hint=${t('services.my_ini_hint', { major: m })} onSaved=${onSaved} />
    </div>
  </div>`;
}

/** Services › MariaDB: the server, installed versions (with their my.ini) and versions to install. */
export function MariaDb({ startJob }) {
  const [{ data, error }, reload] = useApi('/mariadb', 3000);
  const [available, setAvailable] = useState(null);
  const [open, setOpen] = useState(null);
  if (!data) return html`<div class="page-head"><h1>${t('services.mariadb')}</h1></div><${Loading} error=${error} />`;
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
    <div class="page-head"><div class="grow"><h1>${t('services.mariadb')}</h1><p>${t('services.subtitle')}</p></div></div>

    <section class="card">
      <div class="card-head"><h2>${t('services.server')}</h2><span class="grow"></span>
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
        <button class="btn quiet sm" aria-expanded=${open === v ? 'true' : 'false'} onClick=${() => setOpen(open === v ? null : v)}>
          ${t('php.details')}<${Icon} name=${open === v ? 'chevron-down' : 'chevron-right'} size=${15} /></button>
        <${DefaultStar} on=${data.active === major(v)} label=${t('services.use')} onLabel=${t('services.active')}
          onClick=${() => act(api('PUT', '/mariadb/active', { version: major(v) }), reload)} />
        <${RemoveButton} blocked=${data.active === major(v) ? t('services.remove_active') : ''} onClick=${() => remove(v)} />
      </${SetRow}>
      ${open === v && html`<${VersionPanel} version=${v} onSaved=${reload} />`}`)}
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
