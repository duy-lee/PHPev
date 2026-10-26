import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { act } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { Switch, Empty, Loading } from '../ui/controls.js';

const host = (url) => url.replace(/^https?:\/\//, '');

export function Sites() {
  const [{ data: sites, error }, reload] = useApi('/sites', 5000);
  const [{ data: phpInfo }] = useApi('/php');
  const [path, setPath] = useState('');
  const [name, setName] = useState('');
  if (!sites) return html`<div class="page-head"><h1>${t('sites.title')}</h1></div><${Loading} error=${error} />`;
  const versions = phpInfo?.installed.map((v) => v.version.split('.').slice(0, 2).join('.')) ?? [];
  const lines = [...new Set(versions)];
  const open = (s, target) => act(api('POST', `/sites/${s.name}/open`, { target }));
  const unlink = async (s) => {
    const ok = await confirmAction({
      title: t('sites.unlink_title', { name: s.name }),
      body: t('common.confirm_unlink', { name: s.name }),
      confirmLabel: t('sites.unlink'),
      danger: true,
    });
    if (ok) act(api('DELETE', `/sites/${s.name}`), reload);
  };
  const link = () => act(api('POST', '/sites', { path: path.trim(), name: name.trim() || null, php: null }), () => { setPath(''); setName(''); reload(); });
  return html`
    <div class="page-head"><div class="grow"><h1>${t('sites.title')}</h1><p>${t('sites.subtitle', { count: sites.length })}</p></div></div>

    <section class="card">
      ${sites.length === 0 && html`<${Empty} icon="globe" title=${t('sites.empty_title')}>${t('sites.empty')}</${Empty}>`}
      ${sites.map((s) => {
        const php = res(s.php);
        return html`<div class="setrow" key=${s.name}>
          <span class="row-icon" style=${s.secure ? 'color: var(--ok)' : ''}><${Icon} name=${s.secure ? 'lock' : 'globe'} /></span>
          <span class="row-text">
            <b><a href=${s.url} target="_blank" rel="noopener">${host(s.url)}</a></b>
            <span class="mono">${s.path}</span>
          </span>
          <span class="pill">${s.kind === 'linked' ? t('sites.linked') : t('sites.parked')}</span>
          <label class="sr-only" for=${`php-${s.name}`}>${t('sites.php')}</label>
          <select class="select" id=${`php-${s.name}`} value=${s.php_pinned ?? ''} title=${php.ok ? `PHP ${php.value}` : t('sites.php_unavailable', { error: php.error })}
            onChange=${(e) => act(api('PUT', `/sites/${s.name}/php`, { version: e.target.value || null }), reload)}>
            <option value="">${t('sites.php_default_short', { version: php.ok ? php.value : '?' })}</option>
            ${lines.map((l) => html`<option value=${l}>PHP ${l}</option>`)}
          </select>
          <${Switch} checked=${s.secure} label=${t('sites.https')} onChange=${(on) => act(api('PUT', `/sites/${s.name}/secure`, { secure: on }), reload)} />
          <button class="icon-btn" aria-label=${t('sites.open')} title=${t('sites.open')} onClick=${() => open(s, 'browser')}><${Icon} name="open" size=${16} /></button>
          <button class="icon-btn" aria-label=${t('sites.open_folder')} title=${t('sites.open_folder')} onClick=${() => open(s, 'folder')}><${Icon} name="folder" size=${16} /></button>
          <button class="icon-btn" aria-label=${t('sites.open_terminal')} title=${t('sites.open_terminal')} onClick=${() => open(s, 'terminal')}><${Icon} name="terminal" size=${16} /></button>
          ${s.kind === 'linked'
            ? html`<button class="icon-btn" aria-label=${t('sites.unlink')} title=${t('sites.unlink')} onClick=${() => unlink(s)}><${Icon} name="trash" size=${16} /></button>`
            : html`<span style="width: 36px"></span>`}
          ${!php.ok && html`<span class="pill warn">${t('sites.php_unavailable', { error: php.error })}</span>`}
        </div>`;
      })}
      ${sites.length > 0 && html`<div class="card-foot"><${Icon} name="info" size=${16} /><span class="small muted">${t('sites.secure_hint')}</span></div>`}
    </section>

    <section class="card">
      <div class="card-head"><h2>${t('sites.link_title')}</h2></div>
      <div class="row-flex" style="padding: 8px 22px 20px">
        <label class="field grow" style="flex: 2 1 320px"><${Icon} name="folder" size=${16} />
          <input placeholder=${t('sites.link_path')} aria-label=${t('sites.link_path')} value=${path} onInput=${(e) => setPath(e.target.value)} /></label>
        <label class="field" style="flex: 1 1 180px"><input placeholder=${t('sites.link_name')} aria-label=${t('sites.link_name')} value=${name} onInput=${(e) => setName(e.target.value)} /></label>
        <button class="btn primary" disabled=${!path.trim()} onClick=${link}><${Icon} name="plus" size=${16} />${t('sites.link_button')}</button>
      </div>
    </section>`;
}
