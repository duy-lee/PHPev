// PHP: the installed versions (one of them the default), their extensions and php.ini, versions to install.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { mb } from '../format.js';
import { api, res, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction, Dialog } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { DefaultStar, RemoveButton, Tabs, Empty, Loading, SiteLinks } from '../ui/controls.js';
import { IniEditor } from '../ui/ini-editor.js';

const line = (v) => v.split('.').slice(0, 2).join('.');
/** PHP 7 and older no longer get security fixes. */
const eol = (v) => /^[57]\./.test(v);

function Extensions({ version }) {
  const [{ data: exts, error }, reload] = useApi(`/php/${version}/extensions`);
  if (!exts) return error ? html`<span class="badge danger">${error}</span>` : html`<${Loading} height=${90} />`;
  return html`<div class="ext">
    ${exts.map((x) => html`<label title=${x.toggleable ? '' : t('php.ext_builtin')}>
      <input type="checkbox" checked=${x.loaded} disabled=${!x.toggleable}
        onChange=${(e) => act(api('PUT', `/php/${version}/extensions/${x.name}`, { enabled: e.target.checked }), reload)} />
      ${x.name}${!x.toggleable && html`<span class="badge">${t('php.ext_builtin')}</span>`}</label>`)}
  </div>`;
}

/** Below a version's row: its extensions and its php.ini. */
function VersionPanel({ version, onSaved }) {
  const [tab, setTab] = useState('extensions');
  return html`<div class="subpanel">
    <div class="row-flex">
      <${Tabs} role="radiogroup" label=${t('php.details_of', { version })} value=${tab} onChange=${setTab}
        tabs=${[{ id: 'extensions', label: t('php.extensions') }, { id: 'ini', label: 'php.ini' }]} />
      <span class="grow"></span>
      <span class="note"><${Icon} name="info" size=${13} />${tab === 'extensions' ? t('php.ext_hint') : t('php.ini_note')}</span>
    </div>
    ${tab === 'extensions'
      ? html`<${Extensions} version=${version} />`
      : html`<${IniEditor} path=${`/php/${version}/ini`} label=${t('php.ini_label', { version })} hint=${t('php.ini_hint', { version })} onSaved=${onSaved} />`}
  </div>`;
}

/** Copies a PHP folder the user already has (php.exe and php-cgi.exe). */
function AddDialog({ onClose, startJob, onAdded }) {
  const [path, setPath] = useState('');
  const browse = () => api('POST', '/php/pick-folder').then((r) => r.path && setPath(r.path), (e) => showToast(e.message, true));
  const add = () => api('POST', '/php/add', { path: path.trim() }).then((r) => { onClose(); startJob(r.job, onAdded); }, (e) => showToast(e.message, true));
  return html`<${Dialog} title=${t('php.add_title')} onClose=${onClose}>
    <p>${t('php.add_hint')}</p>
    <div class="form-field">
      <label for="php-add-path">${t('php.add_folder')}</label>
      <div class="row-flex" style="flex-wrap: nowrap">
        <label class="field grow"><${Icon} name="folder" size=${14} />
          <input id="php-add-path" placeholder=${t('php.add_path')} value=${path} spellcheck="false" onInput=${(e) => setPath(e.target.value)} /></label>
        <button class="btn" onClick=${browse}>${t('php.browse')}</button>
      </div>
    </div>
    <div class="dialog-actions">
      <button class="btn" onClick=${onClose}>${t('common.cancel')}</button>
      <button class="btn primary" disabled=${!path.trim()} onClick=${add}>${t('php.add_button')}</button>
    </div>
  </${Dialog}>`;
}

/** Versions to install, loaded on request (it asks windows.php.net). */
function Available({ installed, onInstall }) {
  const [available, setAvailable] = useState(null);
  const [busy, setBusy] = useState(false);
  const load = () => {
    setBusy(true);
    api('GET', '/php/available').then(setAvailable, (e) => showToast(t('common.error', { message: e.message }), true)).finally(() => setBusy(false));
  };
  return html`<div class="sec"><h2>${t('php.available_short')}</h2><span class="small muted">${t('php.available_hint')}</span><span class="grow"></span>
      ${available === null && html`<button class="btn sm" disabled=${busy} onClick=${load}>${busy && html`<span class="spin"><${Icon} name="rotate-cw" size=${13} /></span>`}${t('php.load_available')}</button>`}</div>
    ${available !== null && html`<section class="card">
      ${available.map((a) => html`<div class="setrow">
        <span class="row-icon"><${Icon} name="code-xml" /></span>
        <span class="row-text"><b><span class="ver">${a.version}</span>${a.archived && html` <span class="badge warn">${t('php.eol')}</span>`}</b>
          <span>${a.archived ? t('php.eol_hint') : t('php.line', { line: a.line })}</span></span>
        ${installed(a.version)
          ? html`<span class="status"><${Icon} name="check" size=${13} />${t('common.installed')}</span>`
          : html`<button class="btn sm" onClick=${() => onInstall(a.version)}><${Icon} name="download" size=${13} />${t('common.install')}</button>`}
      </div>`)}
    </section>`}`;
}

export function Php({ startJob }) {
  const [{ data, error }, reload] = useApi('/php', 5000);
  const [{ data: sites }] = useApi('/sites', 10000);
  const [open, setOpen] = useState(null);
  const [adding, setAdding] = useState(false);
  const head = html`<div class="page-head"><div class="grow"><h1>PHP</h1>
      <p>${data?.default ? html`${t('php.default_outside')} <span class="mono t2">${data.default}</span> · ${t('php.each_site')}` : t('php.no_default')}</p></div>
    <div class="acts"><button class="btn" onClick=${() => setAdding(true)}><${Icon} name="folder" size=${15} />${t('php.add_existing')}</button></div></div>`;
  const dialog = adding && html`<${AddDialog} onClose=${() => setAdding(false)} startJob=${startJob} onAdded=${reload} />`;
  if (!data) return html`${head}<${Loading} error=${error} />`;
  // From the installed list (refreshed every 5 s), so removing or adding a version updates it at once.
  const isInstalled = (version) => data.installed.some((v) => v.version === version);
  const install = (version) => api('POST', '/php/install', { version }).then((r) => startJob(r.job, reload), (e) => showToast(e.message, true));
  const usedBy = (version) => (sites ?? []).filter((s) => res(s.php).value === version).map((s) => s.name);
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
  if (data.installed.length === 0) {
    return html`${head}<section class="card"><${Empty} icon="code-xml" title=${t('php.empty')}><p>${t('php.empty_hint')}</p></${Empty}></section>
      <${Available} installed=${isInstalled} onInstall=${install} />${dialog}`;
  }
  return html`${head}
    <section class="card clip">
      <div class="card-head"><h2>${t('services.installed')}</h2><span class="sub">${data.installed.length}</span></div>
      <div class="scroll-x"><table class="table">
        <thead><tr><th class="fit"><span class="sr-only">${t('php.default')}</span></th><th>${t('php.version')}</th><th>${t('php.used_by')}</th>
          <th>${t('php.workers_col')}</th><th class="fit"><span class="sr-only">${t('sites.actions')}</span></th></tr></thead>
        <tbody>${data.installed.map((v) => html`<tr key=${v.version}>
          <td class="fit"><${DefaultStar} on=${v.default} label=${t('php.set_default_of', { version: line(v.version) })} onLabel=${t('php.default_version')}
            onClick=${() => act(api('PUT', '/php/default', { version: line(v.version) }), reload)} /></td>
          <td><span class="ver">${v.version}</span>${v.default && html` <span class="badge accent">${t('php.default')}</span>`}${eol(v.version) && html` <span class="badge warn">${t('php.eol')}</span>`}</td>
          <td class="t2"><${SiteLinks} names=${usedBy(v.version)} /></td>
          <td class="t2 tnum">${v.workers ? html`<span class="status"><span class="dot ok"></span>${v.workers}${v.workers_memory_kb > 0 && ` · ${mb(v.workers_memory_kb)}`}</span>` : html`<span class="muted">—</span>`}</td>
          <td><div class="row-acts">
            <button class="btn ghost square sm" disabled=${!v.workers} aria-label=${t('php.restart')} title=${t('php.restart')}
              onClick=${() => act(api('POST', `/php/${v.version}/restart`), reload)}><${Icon} name="rotate-cw" size=${14} /></button>
            <${RemoveButton} blocked=${v.default && data.installed.length > 1 ? t('common.remove_default') : ''} onClick=${() => remove(v)} />
            <button class="btn sm ${open === v.version ? '' : 'ghost'}" aria-expanded=${open === v.version ? 'true' : 'false'}
              onClick=${() => setOpen(open === v.version ? null : v.version)}>${t('php.details')}
              <${Icon} name="chevron-down" size=${13} class=${open === v.version ? 'flip' : ''} /></button>
          </div></td>
        </tr>
        ${open === v.version && html`<tr class="panel-row"><td colspan="5"><${VersionPanel} version=${v.version} onSaved=${reload} /></td></tr>`}`)}
        </tbody>
      </table></div>
    </section>
    <${Available} installed=${isInstalled} onInstall=${install} />${dialog}`;
}
