import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { Dialog } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Empty, Loading } from '../ui/controls.js';
import { prefs } from '../ui/prefs.js';
import { createStore } from '../ui/store.js';
import { SiteDetail } from './site-detail.js';

// Shared by the toolbar and the page.
const ui = createStore({ listHidden: prefs.get('sitesListHidden') === '1', switcher: false, linking: false });

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

function SiteRow({ site, current, onClick, role }) {
  return html`<a href=${siteHref(site.name)} role=${role} class="site-item ${current ? 'on' : ''}" aria-current=${current ? 'page' : undefined}
    onClick=${(e) => { e.preventDefault(); onClick?.(); location.replace(siteHref(site.name)); }}>
    <${Mark} site=${site} />
    <span class="grow"><b>${site.name}</b><span class="sub">${siteSummary(site)}</span></span>
    ${site.secure && html`<${Icon} name="lock" size=${15} label=${t('sites.https')} />`}
  </a>`;
}

export function SitesToolbar({ route }) {
  const state = ui.use();
  const [sites, setSites] = useState(null);
  const name = route[1];
  useEffect(() => {
    if (state.switcher) api('GET', '/sites').then(setSites, () => setSites([]));
  }, [state.switcher]);
  const toggleList = () => {
    prefs.set('sitesListHidden', state.listHidden ? '0' : '1');
    ui.set({ listHidden: !state.listHidden });
  };
  const label = state.listHidden ? t('sites.show_list') : t('sites.hide_list');
  return html`<div class="row-flex" style="position: relative; flex-wrap: nowrap">
    <button class="icon-btn" aria-label=${label} title=${label} aria-pressed=${state.listHidden ? 'false' : 'true'} onClick=${toggleList}>
      <${Icon} name="panel-left" size=${16} /></button>
    <a href="#/sites" class="small muted" style="padding-left: 4px" onClick=${(e) => { e.preventDefault(); location.replace('#/sites'); }}>${t('nav.sites')}</a>
    ${name && html`<span class="muted" aria-hidden="true">/</span>
      <button class="crumb" aria-haspopup="menu" aria-expanded=${state.switcher ? 'true' : 'false'} onClick=${() => ui.set({ switcher: !state.switcher })}>
        ${name}<${Icon} name="chevron-down" size=${15} /></button>`}
    ${state.switcher && html`
      <div style="position: fixed; inset: 0; z-index: 14" onClick=${() => ui.set({ switcher: false })}></div>
      <div class="menu" role="menu" aria-label=${t('sites.switch')} style="left: 44px">
        ${sites === null ? html`<div class="palette-empty">${t('common.loading')}</div>`
          : sites.map((s) => html`<${SiteRow} site=${s} current=${s.name === name} role="menuitem" onClick=${() => ui.set({ switcher: false })} />`)}
        <div style="border-top: 1px solid var(--hair); margin-top: 6px; padding-top: 6px">
          <a href="#" role="menuitem" class="site-item" style="color: var(--accent); font-weight: 600"
            onClick=${(e) => { e.preventDefault(); ui.set({ switcher: false, linking: true }); }}><${Icon} name="plus" size=${16} />${t('sites.link_title')}</a>
        </div>
      </div>`}
  </div>`;
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
  return html`<div class="site-layout">
    ${!state.listHidden && html`<nav class="site-list" aria-label=${t('nav.sites')}>
      <div class="site-list-head">
        <h2 class="section-title">${t('nav.sites')}</h2><span class="small muted">${sites.length}</span><span class="grow"></span>
        <button class="icon-btn sm" aria-label=${t('sites.link_title')} title=${t('sites.link_title')} onClick=${() => ui.set({ linking: true })}><${Icon} name="plus" size=${15} /></button>
      </div>
      ${sites.map((s) => html`<${SiteRow} site=${s} current=${s.name === name} key=${s.name} />`)}
    </nav>`}
    <${SiteDetail} key=${name} name=${name} tab=${route[2] || 'overview'} sub=${route[3]} onChanged=${reload} startJob=${startJob} />
  </div>${linking}`;
}
