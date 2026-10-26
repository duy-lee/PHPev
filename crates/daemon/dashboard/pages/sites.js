import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';

export function Sites() {
  const [{ data: sites, error }, reload] = useApi('/sites', 5000);
  const [{ data: phpInfo }] = useApi('/php');
  const [path, setPath] = useState('');
  const [name, setName] = useState('');
  if (!sites) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const versions = phpInfo?.installed.map((v) => v.version.split('.').slice(0, 2).join('.')) ?? [];
  const lines = [...new Set(versions)];
  const open = (s, target) => act(api('POST', `/sites/${s.name}/open`, { target }));
  return html`
    <h1>${t('sites.title')}</h1>
    ${sites.length === 0 ? html`<p class="muted">${t('sites.empty')}</p>` : html`<table>
      <tr><th>${t('sites.url')}</th><th>${t('sites.kind')}</th><th>${t('sites.php')}</th><th>${t('sites.https')}</th><th>${t('sites.folder')}</th><th>${t('sites.actions')}</th></tr>
      ${sites.map((s) => {
        const php = res(s.php);
        return html`<tr>
          <td><a href=${s.url} target="_blank" rel="noopener">${s.url}</a></td>
          <td>${s.kind}</td>
          <td>
            <select value=${s.php_pinned ?? ''} onChange=${(e) => act(api('PUT', `/sites/${s.name}/php`, { version: e.target.value || null }), reload)}>
              <option value="">${t('sites.php_default')}</option>
              ${lines.map((l) => html`<option value=${l}>${l}</option>`)}
            </select>
            <div class="muted">${php.ok ? php.value : html`<span class="badge warn">${t('sites.php_unavailable', { error: php.error })}</span>`}</div>
          </td>
          <td><input type="checkbox" checked=${s.secure} title=${t('sites.secure_hint')}
            onChange=${(e) => act(api('PUT', `/sites/${s.name}/secure`, { secure: e.target.checked }), reload)} /></td>
          <td class="mono">${s.path}</td>
          <td class="row">
            <button onClick=${() => open(s, 'browser')}>${t('sites.open')}</button>
            <button onClick=${() => open(s, 'folder')}>${t('sites.open_folder')}</button>
            <button onClick=${() => open(s, 'terminal')}>${t('sites.open_terminal')}</button>
            ${s.kind === 'linked' && html`<button onClick=${() => confirm(t('common.confirm_unlink', { name: s.name })) && act(api('DELETE', `/sites/${s.name}`), reload)}>${t('sites.unlink')}</button>`}
          </td></tr>`;
      })}</table>`}
    <p class="muted">${t('sites.secure_hint')}</p>
    <h2>${t('sites.link_title')}</h2>
    <div class="row">
      <input type="text" placeholder=${t('sites.link_path')} value=${path} onInput=${(e) => setPath(e.target.value)} />
      <input type="text" placeholder=${t('sites.link_name')} value=${name} onInput=${(e) => setName(e.target.value)} style="min-width:180px" />
      <button class="primary" disabled=${!path.trim()} onClick=${() => act(api('POST', '/sites', { path: path.trim(), name: name.trim() || null, php: null }), () => { setPath(''); setName(''); reload(); })}>${t('sites.link_button')}</button>
    </div>`;
}
