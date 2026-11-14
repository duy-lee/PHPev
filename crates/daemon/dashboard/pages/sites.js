// Sites: the list of linked folders (#/sites), the breadcrumb switcher, the Link a folder dialog,
// and what a site's row and page share (its mark, its ⋯ menu, unlink).
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi, refreshAll } from '../api.js';
import { act, copy, showToast } from '../ui/feedback.js';
import { Dialog, confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Empty, Loading, MenuButton } from '../ui/controls.js';
import { createStore } from '../ui/store.js';
import { SiteDetail } from './site-detail.js';

// Shared by the breadcrumb, the menu and the page.
const ui = createStore({ switcher: false, linking: false });

/** Opens the Link a folder dialog, from any page. */
export const startLinking = () => ui.set({ linking: true, switcher: false });

const FRAMEWORKS = ['laravel', 'symfony', 'wordpress', 'static'];
const fwClass = (site) => {
  const name = site.framework?.name.toLowerCase();
  return FRAMEWORKS.includes(name) ? name : 'php';
};
export const siteHref = (name, tab) => `#/sites/${encodeURIComponent(name)}${tab ? `/${tab}` : ''}`;
const host = (url) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');

export function Mark({ site, large = false }) {
  return html`<span class="mk ${fwClass(site)} ${large ? 'lg' : ''}" aria-hidden="true">${site.name.slice(0, 1).toUpperCase()}</span>`;
}

/** A site in a table: its mark, name and address. */
export function SiteCell({ site }) {
  return html`<div class="cell-site"><${Mark} site=${site} />
    <div><b>${site.name}</b><span>${site.secure && html`<${Icon} name="lock" size=${11} label=${t('sites.https')} />`}${host(site.url)}</span></div></div>`;
}

/** "Laravel · PHP 8.3" */
export function siteSummary(site) {
  const php = res(site.php);
  const line = php.ok ? php.value.split('.').slice(0, 2).join('.') : null;
  return [site.framework.name, line && `PHP ${line}`].filter(Boolean).join(' · ');
}

/** Asks, then unlinks `site`; `done` runs once it is gone. */
export async function unlinkSite(site, done) {
  const ok = await confirmAction({
    title: t('sites.unlink_title', { name: site.name }),
    body: t('common.confirm_unlink', { name: site.name }),
    confirmLabel: t('sites.unlink'),
    danger: true,
  });
  if (ok) act(api('DELETE', `/sites/${encodeURIComponent(site.name)}`), () => { refreshAll(); done?.(); });
}

/** Opens the site's folder (`folder`), a terminal in it (`terminal`) or an editor on it. */
export const openSite = (name, target) => act(api('POST', `/sites/${encodeURIComponent(name)}/open`, { target }));

/** The ⋯ of a site (its row in Sites, its page); `onEdit` adds Edit site when given. */
export function siteMenu(site, { onEdit, onUnlinked } = {}) {
  return [
    onEdit && { label: t('site.edit_menu'), icon: 'pencil', onClick: onEdit },
    { label: t('sites.copy_address'), icon: 'copy', onClick: () => copy(site.url) },
    { label: t('sites.open_folder_long'), icon: 'folder-open', onClick: () => openSite(site.name, 'folder') },
    { label: t('sites.open_terminal_long'), icon: 'square-terminal', onClick: () => openSite(site.name, 'terminal') },
    { label: t('sites.unlink_menu'), hint: t('site.unlink_hint'), icon: 'trash-2', danger: true, divider: true, onClick: () => unlinkSite(site, onUnlinked) },
  ];
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
          <a href="#" role="menuitem" class="mi" onClick=${(e) => { e.preventDefault(); startLinking(); }}>
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
    <div class="form-field">
      <label for="link-path">${t('sites.link_path')}</label>
      <div class="row-flex" style="flex-wrap: nowrap">
        <label class="field grow"><${Icon} name="folder" size=${14} />
          <input id="link-path" value=${path} spellcheck="false" onInput=${(e) => setPath(e.target.value)} /></label>
        <button class="btn" onClick=${browse}>${t('php.browse')}</button>
      </div>
    </div>
    <div class="form-field">
      <label for="link-name">${t('sites.link_name')}</label>
      <div class="field"><input id="link-name" value=${name} spellcheck="false" autocomplete="off" onInput=${(e) => setName(e.target.value)} /></div>
    </div>
    <div class="dialog-actions">
      <button class="btn" onClick=${onClose}>${t('common.cancel')}</button>
      <button class="btn primary" disabled=${!path.trim()} onClick=${link}>${t('sites.link_button')}</button>
    </div>
  </${Dialog}>`;
}

/** The Link a folder dialog while it is open; the app renders it once, for every page. */
export function LinkHost() {
  const state = ui.use();
  return state.linking && html`<${LinkDialog} onClose=${() => ui.set({ linking: false })} onLinked=${refreshAll} />`;
}

/** `#/sites`: every linked folder. */
function SiteList({ sites, tld }) {
  const [filter, setFilter] = useState('');
  const q = filter.trim().toLowerCase();
  const shown = sites.filter((s) => !q || s.name.includes(q) || s.path.toLowerCase().includes(q));
  const open = (name) => location.replace(siteHref(name));
  return html`<div class="page-head">
      <div class="grow"><h1>${t('sites.title')}</h1><p>${t('sites.subtitle', { count: sites.length, tld })}</p></div>
      <div class="acts">
        <label class="field" style="width: 220px"><${Icon} name="search" size=${14} />
          <input placeholder=${t('sites.filter')} aria-label=${t('sites.filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>
        <button class="btn primary" onClick=${startLinking}><${Icon} name="plus" size=${15} />${t('sites.link_title')}</button>
      </div>
    </div>
    <section class="card clip"><div class="scroll-x"><table class="table">
      <thead><tr><th>${t('home.col_site')}</th><th>${t('sites.framework')}</th><th>PHP</th><th>Composer</th>
        <th class="num">${t('home.requests_10m')}</th><th class="fit"><span class="sr-only">${t('sites.actions')}</span></th></tr></thead>
      <tbody>${shown.map((s) => {
        const php = res(s.php);
        return html`<tr class="click" key=${s.name} onClick=${(e) => !e.target.closest('button, a, .menu') && open(s.name)}>
          <td><a href=${siteHref(s.name)} class="cell-link" onClick=${(e) => { e.preventDefault(); open(s.name); }}><${SiteCell} site=${s} /></a></td>
          <td class="t2">${s.framework.version ? `${s.framework.name} ${s.framework.version}` : s.framework.name}</td>
          <td>${php.ok
            ? html`<span class="badge mono ${/^[57]\./.test(php.value) ? 'warn' : ''}">${php.value}</span>${s.php_pinned && html` <span class="small muted">${t('sites.pinned')}</span>`}`
            : html`<span class="badge danger" title=${php.error}>${t('home.no_php')}</span>`}</td>
          <td class="mono t2">${s.composer_pinned ?? html`<span class="muted">${t('site.default')}</span>`}</td>
          <td class="num ${s.requests_10m ? '' : 'muted'}">${s.requests_10m ? s.requests_10m.toLocaleString() : t('home.idle')}</td>
          <td><${MenuButton} label=${t('sites.more_for', { name: s.name })} items=${siteMenu(s)} /></td>
        </tr>`;
      })}
      ${shown.length === 0 && html`<tr><td colspan="6"><${Empty} icon="search" title=${t('sites.no_match')} /></td></tr>`}</tbody>
    </table></div></section>`;
}

export function Sites({ route, startJob }) {
  const [{ data: sites, error }, reload] = useApi('/sites', 5000);
  const [{ data: settings }] = useApi('/settings');
  if (!sites) return html`<div class="page-head"><div class="grow"><h1>${t('sites.title')}</h1></div></div><${Loading} error=${error} />`;
  if (sites.length === 0) {
    return html`<div class="page-head"><div class="grow"><h1>${t('sites.title')}</h1><p>${t('sites.subtitle_empty')}</p></div></div>
      <section class="card"><${Empty} icon="folder-open" title=${t('sites.empty_title')}>
        <p>${t('sites.empty')}</p>
        <button class="btn primary" onClick=${startLinking}><${Icon} name="plus" size=${15} />${t('sites.link_title')}</button>
      </${Empty}></section>`;
  }
  if (!route[1]) return html`<${SiteList} sites=${sites} tld=${settings?.tld ?? ''} />`;
  const site = sites.find((s) => s.name === route[1]);
  if (!site) {
    return html`<section class="card"><${Empty} icon="globe" title=${t('sites.not_found', { name: route[1] })}>
      <a class="btn" href="#/sites" onClick=${(e) => { e.preventDefault(); location.replace('#/sites'); }}>${t('sites.back')}</a>
    </${Empty}></section>`;
  }
  return html`<${SiteDetail} key=${site.name} name=${site.name} tab=${route[2] || 'overview'} sub=${route[3]} onChanged=${reload} startJob=${startJob} />`;
}
