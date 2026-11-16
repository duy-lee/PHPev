// Runtimes › MariaDB: the server, then Versions / Databases / my.ini (#/services/mariadb/<tab>).
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { mb } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { DefaultStar, RemoveButton, Tabs, Empty, Loading } from '../ui/controls.js';
import { IniEditor } from '../ui/ini-editor.js';
import { DatabasesPanel } from './mariadb-databases.js';

const major = (v) => v.split('.').slice(0, 2).join('.');
const TABS = ['versions', 'databases', 'ini'];

/** Releases to install, loaded on request (it asks mariadb.org). */
function Available({ onInstall }) {
  const [available, setAvailable] = useState(null);
  const [busy, setBusy] = useState(false);
  const load = () => {
    setBusy(true);
    api('GET', '/mariadb/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true)).finally(() => setBusy(false));
  };
  return html`<div class="sec"><h2>${t('php.available_short')}</h2><span class="small muted">${t('services.available_hint')}</span><span class="grow"></span>
      ${available === null && html`<button class="btn sm" disabled=${busy} onClick=${load}>${busy && html`<span class="spin"><${Icon} name="rotate-cw" size=${13} /></span>`}${t('services.load_available')}</button>`}</div>
    ${available !== null && html`<section class="card">
      ${available.map((a) => html`<div class="setrow" key=${a.major}>
        <span class="row-icon"><${Icon} name="database" /></span>
        <span class="row-text"><b><span class="ver">${a.major}</span>${a.lts && html` <span class="badge">${t('services.lts')}</span>`}${a.eol_passed && html` <span class="badge warn">${t('services.eol_passed')}</span>`}</b>
          <span>${a.eol ? t('services.supported_until', { date: a.eol }) : ''}</span></span>
        ${a.installed
          ? html`<span class="status"><${Icon} name="check" size=${13} />${t('common.installed')}</span>`
          : html`<button class="btn sm" onClick=${() => onInstall(a.major, load)}><${Icon} name="download" size=${13} />${t('common.install')}</button>`}
      </div>`)}
    </section>`}`;
}

export function MariaDb({ startJob, route = [] }) {
  const [{ data, error }, reload] = useApi('/mariadb', 3000);
  const [dbCount, setDbCount] = useState(null);
  const tab = TABS.includes(route[2]) ? route[2] : 'versions';
  const running = data?.running;
  // Databases and my.ini are those of the active major; another major's show once it is made active.
  const current = data?.installed.length ? data.active ?? major(data.installed[0]) : null;
  // For the count on the Databases tab (once per major and server); the panel updates it after each change.
  useEffect(() => {
    if (!current) return;
    api('GET', `/mariadb/${current}/databases`).then((r) => setDbCount(r.databases.length), () => setDbCount(null));
  }, [current, running?.version]);
  const head = html`<div class="page-head">
    <div class="grow"><h1 class="row-flex">MariaDB${data?.installed.length > 0 && (running
        ? html`<span class="badge ok"><span class="dot ok"></span>${t('common.running')}</span>`
        : html`<span class="badge">${t('common.stopped')}</span>`)}</h1>
      <p>${!data ? t('services.subtitle') : running
        ? html`<span class="mono t2">${running.version}</span> ${t('services.on')} <span class="mono t2">127.0.0.1:${data.port}</span> · ${mb(running.memory_kb)} · ${t('services.starts_with')}`
        : data.installed.length ? t('services.stopped_hint_long') : t('services.subtitle')}</p></div>
    ${data?.installed.length > 0 && html`<div class="acts">
      <button class="btn" disabled title=${t('site.mariadb_restart_soon_here')}><${Icon} name="rotate-cw" size=${15} />${t('site.restart')}</button>
      ${running
        ? html`<button class="btn" onClick=${() => act(api('POST', '/mariadb/stop'), reload)}><${Icon} name="square" size=${13} />${t('common.stop')}</button>`
        : html`<button class="btn primary" onClick=${() => act(api('POST', '/mariadb/start'), reload)}><${Icon} name="play" size=${13} />${t('common.start')}</button>`}
    </div>`}
  </div>`;
  if (!data) return html`${head}<${Loading} error=${error} />`;
  const install = (version, after) => api('POST', '/mariadb/install', { version }).then((r) => startJob(r.job, () => { reload(); after?.(); }), (e) => showToast(e.message, true));
  if (data.installed.length === 0) {
    return html`${head}<section class="card"><${Empty} icon="database" title=${t('services.not_installed')}><p>${t('services.not_installed_hint')}</p></${Empty}></section>
      <${Available} onInstall=${install} />`;
  }

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
  const go = (id) => location.replace(`#/services/mariadb${id === 'versions' ? '' : `/${id}`}`);

  return html`${head}
    <${Tabs} label=${t('services.sections')} value=${tab} onChange=${go}
      tabs=${[{ id: 'versions', label: t('services.versions') }, { id: 'databases', label: t('services.databases'), count: dbCount }, { id: 'ini', label: 'my.ini' }]} />

    ${tab === 'versions' && html`<section class="card clip">
        <div class="card-head"><h2>${t('services.installed')}</h2><span class="sub">${t('services.switch_note')}</span></div>
        <div class="scroll-x"><table class="table">
          <tbody>${data.installed.map((v) => {
            const active = data.active === major(v);
            return html`<tr key=${v}>
              <td class="fit"><${DefaultStar} on=${active} label=${t('services.use_version', { version: major(v) })} onLabel=${t('services.active')}
                onClick=${() => act(api('PUT', '/mariadb/active', { version: major(v) }), reload)} /></td>
              <td><span class="ver">${v}</span>${active && html` <span class="badge accent">${t('services.active_short')}</span>`}
                ${running && major(running.version) === major(v) && html` <span class="status" style="margin-left: 6px"><span class="dot ok"></span>${t('common.running')}</span>`}</td>
              <td class="mono muted small">${active && data.data_dir ? data.data_dir : `data\\mariadb\\${major(v)}`}</td>
              <td><div class="row-acts"><${RemoveButton} blocked=${active ? t('services.remove_active') : ''} onClick=${() => remove(v)} /></div></td>
            </tr>`;
          })}</tbody>
        </table></div>
      </section>
      <${Available} onInstall=${install} />`}

    ${tab === 'databases' && html`<${DatabasesPanel} major=${current} running=${!!running && major(running.version) === current}
      port=${data.port} startJob=${startJob} onCount=${setDbCount} />`}

    ${tab === 'ini' && html`<div class="callout" style="margin: 0 0 12px"><${Icon} name="info" size=${15} /><span>${t('services.my_ini_note', { major: current })}</span></div>
      <${IniEditor} key=${current} path=${`/mariadb/${current}/ini`} label=${t('services.my_ini_label', { major: current })}
        hint=${t('services.my_ini_hint', { major: current })} onSaved=${reload} />`}`;
}
