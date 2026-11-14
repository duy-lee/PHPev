import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { Dialog } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Empty, Loading } from '../ui/controls.js';
import { createStore } from '../ui/store.js';
import { SiteDetail } from './site-detail.js';

// Shared by the breadcrumb, the menu and the page.
const ui = createStore({ switcher: false, linking: false });

/** Opens the Link a folder dialog (from the menu's site list, for instance). */
export const startLinking = () => {
  ui.set({ linking: true });
  if (!location.hash.startsWith('#/sites')) location.replace('#/sites');
};

const FRAMEWORKS = ['laravel', 'symfony', 'wordpress', 'static'];
const fwClass = (site) => {
  const name = site.framework?.name.toLowerCase();
  return FRAMEWORKS.includes(name) ? name : 'php';
};
export const siteHref = (name, tab) => `#/sites/${encodeURIComponent(name)}${tab ? `/${tab}` : ''}`;

export function Mark({ site }) {
  return html`<span class="mk ${fwClass(site)}" aria-hidden="true">${site.name.slice(0, 1).toUpperCase()}</span>`;
}

/** "Laravel · PHP 8.3" */
export function siteSummary(site) {
  const php = res(site.php);
  const line = php.ok ? php.value.split('.').slice(0, 2).join('.') : null;
  return [site.framework.name, line && `PHP ${line}`].filter(Boolean).join(' · ');
}

/** Breadcrumb after "Sites /": the current site, a button that switches to another one. */
export function SitesCrumbs({ route }) {
  const state = ui.use();
  const [sites, setSites] = useState(null);
  const name = route[1];
  useEffect(() => {
    if (state.switcher) api('GET', '/sites').then(setSites, () => setSites([]));
  }, [state.switcher]);
  useEffect(() => {
    if (!state.switcher) return;
    const onKey = (e) => e.key === 'Escape' && ui.set({ switcher: false });
    addEventListener('keydown', onKey);
    return () => removeEventListener('keydown', onKey);
  }, [state.switcher]);
  const close = () => ui.set({ switcher: false });
  if (!name) return html`<span class="cur">${t('nav.sites')}</span>`;
  return html`<a href="#/sites" onClick=${(e) => { e.preventDefault(); location.replace('#/sites'); }}>${t('nav.sites')}</a>
    <span class="sep" aria-hidden="true">/</span>
    <span style="position: relative">
      <button class="crumb-btn" aria-haspopup="menu" aria-expanded=${state.switcher ? 'true' : 'false'} title=${t('sites.switch')}
        onClick=${() => ui.set({ switcher: !state.switcher })}>${name}<${Icon} name="chevrons-up-down" size=${14} /></button>
      ${state.switcher && html`
        <div class="menu-catch" onClick=${close}></div>
        <div class="menu" role="menu" aria-label=${t('sites.switch')} style="position: absolute; top: 32px; left: 0">
          ${sites === null ? html`<div class="mh">${t('common.loading')}</div>`
            : sites.map((s) => html`<a href=${siteHref(s.name)} role="menuitem" class="mi" key=${s.name}
              onClick=${(e) => { e.preventDefault(); close(); location.replace(siteHref(s.name)); }}>
              <span class="tick">${s.name === name && html`<${Icon} name="check" size=${14} />`}</span>
              <span class="grow"><span class="lab">${s.name}</span><span class="hint">${siteSummary(s)}</span></span></a>`)}
          <hr />
          <a href="#" role="menuitem" class="mi" onClick=${(e) => { e.preventDefault(); ui.set({ switcher: false, linking: true }); }}>
            <${Icon} name="plus" size=${14} /><span class="lab">${t('sites.link_title')}</span></a>
        </div>`}
    </span>`;
}

export function LinkDialog({ onClose, onLinked }) {
  const [path, setPath] = useState('');
  const [name, setName] = useState('');
  const browse = () => api('POST', '/sites/pick-folder').then((r) => r.path && setPath(r.path), (e) => showToast(e.message, true));
  const link = () => act(api('POST', '/sites', { path: path.trim(), name: name.trim() || null, php: null }), () => { onLinked(); onClose(); });
  return html`<${Dialog} title=${t('sites.link_title')} onClose=${onClose}>
    <p>${t('sites.link_hint')}</p>
    <div class="row-flex" style="flex-wrap: nowrap">
      <label class="field grow"><${Icon} name="folder" size=${16} />
        <input placeholder=${t('sites.link_path')} aria-label=${t('sites.link_path')} value=${path} onInput=${(e) => setPath(e.target.value)} /></label>
      <button class="btn" onClick=${browse}>${t('php.browse')}</button>
    </div>
    <label class="field" style="margin-top: 10px"><input placeholder=${t('sites.link_name')} aria-label=${t('sites.link_name')} value=${name} onInput=${(e) => setName(e.target.value)} /></label>
    <div class="dialog-actions">
      <button class="btn quiet" onClick=${onClose}>${t('common.cancel')}</button>
      <button class="btn primary" disabled=${!path.trim()} onClick=${link}>${t('sites.link_button')}</button>
    </div>
  </${Dialog}>`;
}

export function Sites({ route, startJob }) {
  const state = ui.use();
  const [{ data: sites, error }, reload] = useApi('/sites', 5000);
  const linking = state.linking && html`<${LinkDialog} onClose=${() => ui.set({ linking: false })} onLinked=${reload} />`;
  if (!sites) return html`<${Loading} error=${error} />`;
  if (sites.length === 0) {
    return html`<div class="page-head"><h1>${t('sites.title')}</h1></div>
      <section class="card"><${Empty} icon="globe" title=${t('sites.empty_title')}>
        <p style="margin: 0 0 14px">${t('sites.empty')}</p>
        <button class="btn primary" onClick=${() => ui.set({ linking: true })}><${Icon} name="plus" size=${16} />${t('sites.link_title')}</button>
      </${Empty}></section>${linking}`;
  }
  const name = sites.some((s) => s.name === route[1]) ? route[1] : sites[0].name;
  // `#/sites` (from the menu) or a removed site: show the first one, with its name in the address
  // so the breadcrumb and the tabs know it.
  if (route[1] !== name) queueMicrotask(() => location.replace(siteHref(name, route[2])));
  return html`<${SiteDetail} key=${name} name=${name} tab=${route[2] || 'overview'} sub=${route[3]} onChanged=${reload} startJob=${startJob} />${linking}`;
}
