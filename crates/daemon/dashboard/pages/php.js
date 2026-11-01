import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { SetRow, DefaultStar, RemoveButton, Tabs, Empty, Loading } from '../ui/controls.js';

const line = (v) => v.split('.').slice(0, 2).join('.');

function Extensions({ version }) {
  const [{ data: exts, error }, reload] = useApi(`/php/${version}/extensions`);
  if (!exts) return error ? html`<span class="pill danger">${error}</span>` : html`<span class="small muted">${t('common.loading')}</span>`;
  return html`<p class="small muted" style="margin: 0">${t('php.ext_hint')}</p>
    <div class="ext-grid">
      ${exts.map((x) => html`<label class="check" title=${x.toggleable ? '' : t('php.ext_builtin')}>
        <input type="checkbox" checked=${x.loaded} disabled=${!x.toggleable}
          onChange=${(e) => act(api('PUT', `/php/${version}/extensions/${x.name}`, { enabled: e.target.checked }), reload)} />
        <span class="mono">${x.name}</span>${!x.toggleable && html`<span class="small muted">${t('php.ext_builtin')}</span>`}</label>`)}
    </div>`;
}

// A textarea reports `\n` line endings whatever the file uses.
const lf = (text) => text.replace(/\r\n/g, '\n');

/** The version's php.ini in a textarea; saving checks it with PHP and restarts the workers. */
function PhpIni({ version, active, onSaved }) {
  const [ini, setIni] = useState(null);
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const dirty = ini?.text !== undefined && text !== lf(ini.text);
  const load = () => api('GET', `/php/${version}/ini`).then((r) => {
    setIni(r);
    setText(lf(r.text));
  }, (e) => setIni({ error: e.message }));
  // Reloaded each time the tab is shown, unless edited: extensions also change the file.
  useEffect(() => {
    if (active && !dirty) load();
  }, [active]);
  if (!ini) return html`<span class="small muted">${t('common.loading')}</span>`;
  if (ini.error) return html`<span class="pill danger">${t('common.error', { message: ini.error })}</span>`;
  const save = async () => {
    if (!dirty || busy) return;
    setBusy(true);
    try {
      const r = await api('PUT', `/php/${version}/ini`, { text, stamp: ini.stamp });
      showToast(r.message);
      await load();
      onSaved();
    } catch (e) {
      showToast(t('common.error', { message: e.message }), true);
    }
    setBusy(false);
  };
  const onKey = (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      save();
    }
  };
  return html`<div class="ini-editor">
    <textarea class="mono" spellcheck="false" aria-label=${t('php.ini_label', { version })} value=${text}
      onInput=${(e) => setText(e.target.value)} onKeyDown=${onKey}></textarea>
    <div class="row-flex">
      <span class="small muted grow">${t('php.ini_hint', { version })}</span>
      <button class="btn quiet sm" disabled=${!dirty || busy} onClick=${() => setText(lf(ini.text))}>${t('php.ini_discard')}</button>
      <button class="btn primary sm" disabled=${!dirty || busy} onClick=${save}>${busy ? t('php.ini_saving') : t('php.ini_save')}</button>
    </div>
  </div>`;
}

/** Below a version's row: its extensions and its php.ini, as two tabs. */
function VersionPanel({ version, onSaved }) {
  const [tab, setTab] = useState('extensions');
  return html`<div class="version-panel">
    <${Tabs} label=${t('php.details_of', { version })} value=${tab} onChange=${setTab}
      tabs=${[{ id: 'extensions', label: t('php.extensions') }, { id: 'ini', label: 'php.ini' }]} />
    <div class="ext-panel" hidden=${tab !== 'extensions'}><${Extensions} version=${version} /></div>
    <div hidden=${tab !== 'ini'}><${PhpIni} version=${version} active=${tab === 'ini'} onSaved=${onSaved} /></div>
  </div>`;
}

export function Php({ startJob }) {
  const [{ data, error }, reload] = useApi('/php', 5000);
  const [available, setAvailable] = useState(null);
  const [open, setOpen] = useState(null);
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
          <button class="btn quiet sm" aria-expanded=${open === v.version ? 'true' : 'false'} onClick=${() => setOpen(open === v.version ? null : v.version)}>
            ${t('php.details')}<${Icon} name=${open === v.version ? 'chevron-down' : 'chevron-right'} size=${15} /></button>
          <${DefaultStar} on=${v.default} label=${t('php.set_default')} onLabel=${t('php.default')}
            onClick=${() => act(api('PUT', '/php/default', { version: line(v.version) }), reload)} />
          <button class="icon-btn" aria-label=${t('php.restart')} title=${t('php.restart')} onClick=${() => act(api('POST', `/php/${v.version}/restart`), reload)}><${Icon} name="restart" size=${16} /></button>
          <${RemoveButton} blocked=${v.default && data.installed.length > 1 ? t('common.remove_default') : ''} onClick=${() => remove(v)} />
        </${SetRow}>
        ${open === v.version && html`<${VersionPanel} version=${v.version} onSaved=${reload} />`}`)}
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
