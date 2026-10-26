import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';

function Extensions({ version }) {
  const [{ data: exts, error }, reload] = useApi(`/php/${version}/extensions`);
  if (!exts) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  return html`<p class="muted">${t('php.ext_hint')}</p><div class="ext-grid">
    ${exts.map((x) => html`<label>
      <input type="checkbox" checked=${x.loaded} disabled=${!x.toggleable}
        onChange=${(e) => act(api('PUT', `/php/${version}/extensions/${x.name}`, { enabled: e.target.checked }), reload)} />
      ${' '}${x.name}${!x.toggleable && html` <span class="muted">(${t('php.ext_builtin')})</span>`}</label>`)}
  </div>`;
}

export function Php({ startJob }) {
  const [{ data, error }, reload] = useApi('/php', 5000);
  const [available, setAvailable] = useState(null);
  const [extOpen, setExtOpen] = useState(null);
  const [addPath, setAddPath] = useState('');
  if (!data) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const loadAvailable = () => api('GET', '/php/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true));
  const install = (version) => api('POST', '/php/install', { version }).then((r) => startJob(r.job, () => { reload(); loadAvailable(); }), (e) => showToast(e.message, true));
  const browse = () => api('POST', '/php/pick-folder').then((r) => r.path && setAddPath(r.path), (e) => showToast(e.message, true));
  const add = () => api('POST', '/php/add', { path: addPath.trim() }).then((r) => startJob(r.job, () => { setAddPath(''); reload(); }), (e) => showToast(e.message, true));
  const remove = (v) => {
    const question = v.pinned_by.length
      ? t('php.confirm_remove_pinned', { version: v.version, sites: v.pinned_by.join(', ') })
      : t('php.confirm_remove', { version: v.version });
    if (confirm(question)) act(api('DELETE', `/php/${v.version}`), reload);
  };
  return html`
    <h1>${t('php.title')}</h1>
    ${data.installed.length === 0 ? html`<p class="muted">${t('php.empty')}</p>` : html`<table>
      ${data.installed.map((v) => html`<tr>
        <td class="big">${v.version} ${v.default && html`<span class="badge ok">${t('php.default')}</span>`}</td>
        <td class="muted">${t('php.workers', { count: v.workers })}</td>
        <td class="row">
          ${!v.default && html`<button onClick=${() => act(api('PUT', '/php/default', { version: v.version.split('.').slice(0, 2).join('.') }), reload)}>${t('php.set_default')}</button>`}
          <button onClick=${() => act(api('POST', `/php/${v.version}/restart`), reload)}>${t('php.restart')}</button>
          <button onClick=${() => setExtOpen(extOpen === v.version ? null : v.version)}>${t('php.extensions')}</button>
          ${(!v.default || data.installed.length === 1) && html`<button onClick=${() => remove(v)}>${t('common.remove')}</button>`}
        </td></tr>
        ${extOpen === v.version && html`<tr><td colspan="3"><${Extensions} version=${v.version} /></td></tr>`}`)}
    </table>`}
    <h2>${t('php.add_title')}</h2>
    <p class="muted">${t('php.add_hint')}</p>
    <div class="row">
      <input type="text" placeholder=${t('php.add_path')} value=${addPath} onInput=${(e) => setAddPath(e.target.value)} />
      <button onClick=${browse}>${t('php.browse')}</button>
      <button class="primary" disabled=${!addPath.trim()} onClick=${add}>${t('php.add_button')}</button>
    </div>
    <h2>${t('php.available')}</h2>
    ${available === null ? html`<button onClick=${loadAvailable}>${t('php.load_available')}</button>` : html`<table>
      ${available.map((a) => html`<tr><td class="big">${a.version}${a.archived && html` <span class="badge warn">${t('php.eol')}</span>`}</td><td>${a.installed ? html`<span class="badge ok">${t('common.installed')}</span>`
        : html`<button class="primary" onClick=${() => install(a.version)}>${t('common.install')}</button>`}</td></tr>`)}
    </table>`}`;
}
