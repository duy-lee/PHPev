import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { SetRow, Empty, Loading } from '../ui/controls.js';

const line = (v) => v.split('.').slice(0, 2).join('.');

function Extensions({ version }) {
  const [{ data: exts, error }, reload] = useApi(`/php/${version}/extensions`);
  if (!exts) return html`<div class="ext-grid">${error ? html`<span class="pill danger">${error}</span>` : t('common.loading')}</div>`;
  return html`<div class="ext-grid">
    <p class="small muted" style="grid-column: 1 / -1; margin: 10px 0 4px">${t('php.ext_hint')}</p>
    ${exts.map((x) => html`<label class="check" title=${x.toggleable ? '' : t('php.ext_builtin')}>
      <input type="checkbox" checked=${x.loaded} disabled=${!x.toggleable}
        onChange=${(e) => act(api('PUT', `/php/${version}/extensions/${x.name}`, { enabled: e.target.checked }), reload)} />
      <span class="mono">${x.name}</span>${!x.toggleable && html`<span class="small muted">${t('php.ext_builtin')}</span>`}</label>`)}
  </div>`;
}

export function Php({ startJob }) {
  const [{ data, error }, reload] = useApi('/php', 5000);
  const [available, setAvailable] = useState(null);
  const [extOpen, setExtOpen] = useState(null);
  const [addPath, setAddPath] = useState('');
  if (!data) return html`<div class="page-head"><h1>${t('php.title')}</h1></div><${Loading} error=${error} />`;
  // From the installed list (refreshed every 5 s), so removing or adding a version updates it at once.
  const isInstalled = (version) => data.installed.some((v) => v.version === version);
  const loadAvailable = () => api('GET', '/php/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true));
  const install = (version) => api('POST', '/php/install', { version }).then((r) => startJob(r.job, reload), (e) => showToast(e.message, true));
  const browse = () => api('POST', '/php/pick-folder').then((r) => r.path && setAddPath(r.path), (e) => showToast(e.message, true));
  const add = () => api('POST', '/php/add', { path: addPath.trim() }).then((r) => startJob(r.job, () => { setAddPath(''); reload(); }), (e) => showToast(e.message, true));
  const remove = async (v) => {
    const ok = await confirmAction({
      title: t('php.remove_title', { version: v.version }),
      body: v.pinned_by.length
        ? t('php.confirm_remove_pinned', { version: v.version, sites: v.pinned_by.join(', ') })
        : t('php.confirm_remove', { version: v.version }),
      confirmLabel: t('common.remove'),
      danger: true,
    });
    if (ok) act(api('DELETE', `/php/${v.version}`), reload);
  };
  return html`
    <div class="page-head"><div class="grow"><h1>${t('php.title')}</h1>
      <p>${data.default ? t('php.default_is', { version: data.default }) : t('php.no_default')}</p></div></div>

    <section class="card">
      ${data.installed.length === 0 && html`<${Empty} icon="php" title=${t('php.empty')}>${t('php.empty_hint')}</${Empty}>`}
      ${data.installed.map((v) => html`
        <${SetRow} icon="php" title=${`PHP ${v.version}`}
          desc=${[v.default ? t('php.default') : '', t('php.workers', { count: v.workers })].filter(Boolean).join(' · ')}>
          ${v.default
            ? html`<span class="pill accent">${t('php.default')}</span>`
            : html`<button class="btn quiet sm" onClick=${() => act(api('PUT', '/php/default', { version: line(v.version) }), reload)}>${t('php.set_default')}</button>`}
          <button class="btn quiet sm" aria-expanded=${extOpen === v.version ? 'true' : 'false'} onClick=${() => setExtOpen(extOpen === v.version ? null : v.version)}>
            ${t('php.extensions')}<${Icon} name=${extOpen === v.version ? 'chevron-down' : 'chevron-right'} size=${15} /></button>
          <button class="icon-btn" aria-label=${t('php.restart')} title=${t('php.restart')} onClick=${() => act(api('POST', `/php/${v.version}/restart`), reload)}><${Icon} name="restart" size=${16} /></button>
          ${(!v.default || data.installed.length === 1)
            ? html`<button class="icon-btn" aria-label=${t('common.remove')} title=${t('common.remove')} onClick=${() => remove(v)}><${Icon} name="trash" size=${16} /></button>`
            : html`<span style="width: 36px"></span>`}
        </${SetRow}>
        ${extOpen === v.version && html`<${Extensions} version=${v.version} />`}`)}
    </section>

    <section class="card">
      <div class="card-head"><h2>${t('php.add_title')}</h2></div>
      <p class="small muted" style="margin: 0; padding: 0 22px">${t('php.add_hint')}</p>
      <div class="row-flex" style="padding: 12px 22px 20px">
        <label class="field grow" style="flex: 1 1 320px"><${Icon} name="folder" size=${16} />
          <input placeholder=${t('php.add_path')} aria-label=${t('php.add_path')} value=${addPath} onInput=${(e) => setAddPath(e.target.value)} /></label>
        <button class="btn" onClick=${browse}>${t('php.browse')}</button>
        <button class="btn primary" disabled=${!addPath.trim()} onClick=${add}>${t('php.add_button')}</button>
      </div>
    </section>

    <section class="card">
      <div class="card-head"><h2>${t('php.available')}</h2><span class="grow"></span>
        ${available === null && html`<button class="btn sm" onClick=${loadAvailable}>${t('php.load_available')}</button>`}</div>
      ${available === null
        ? html`<p class="small muted" style="margin: 0; padding: 0 22px 18px">${t('php.available_hint')}</p>`
        : available.map((a) => html`<${SetRow} icon="download" title=${`PHP ${a.version}`} desc=${a.archived ? t('php.eol_hint') : t('php.line', { line: a.line })}>
            ${a.archived && html`<span class="pill warn">${t('php.eol')}</span>`}
            ${isInstalled(a.version)
              ? html`<span class="pill ok">${t('common.installed')}</span>`
              : html`<button class="btn sm" onClick=${() => install(a.version)}>${t('common.install')}</button>`}
          </${SetRow}>`)}
    </section>`;
}
