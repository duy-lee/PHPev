// Runtimes › Composer: the installed versions (one of them the default) and versions to install.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { DefaultStar, RemoveButton, Empty, Loading, SiteLinks } from '../ui/controls.js';

export function Composer({ startJob }) {
  const [{ data, error }, reload] = useApi('/composer', 5000);
  const [available, setAvailable] = useState(null);
  const [busy, setBusy] = useState(false);
  const head = html`<div class="page-head"><div class="grow"><h1>${t('services.composer')}</h1><p>${t('services.composer_subtitle')}</p></div></div>`;
  if (!data) return html`${head}<${Loading} error=${error} />`;
  const fail = (e) => showToast(t('common.error', { message: e.message }), true);
  const loadAvailable = () => {
    setBusy(true);
    api('GET', '/composer/available').then(setAvailable, fail).finally(() => setBusy(false));
  };
  const refresh = () => {
    reload();
    if (available) loadAvailable();
  };
  const install = (version) => api('POST', '/composer/install', { version }).then((r) => startJob(r.job, refresh), fail);
  const remove = async (c) => {
    const ok = await confirmAction({
      title: t('services.composer_remove_title', { version: c.version }),
      body: c.used_by.length
        ? t('services.composer_remove_used', { sites: c.used_by.join(', ') })
        : t('services.composer_remove_body'),
      confirmLabel: t('common.remove'),
      danger: true,
    });
    if (ok) act(api('DELETE', `/composer/${c.version}`), refresh);
  };
  // As on the PHP page: the default goes last, once no other version is left.
  const blocked = (c) => {
    if (c.needed_by.length) return t('services.composer_needed_by', { sites: c.needed_by.join(', ') });
    return c.default && data.installed.length > 1 ? t('common.remove_default') : '';
  };
  return html`${head}
    ${data.installed.length === 0
      ? html`<section class="card"><${Empty} icon="package" title=${t('services.composer_none')}><p>${t('services.composer_none_hint')}</p></${Empty}></section>`
      : html`<section class="card clip">
        <div class="card-head"><h2>${t('services.installed')}</h2><span class="sub">${data.installed.length}</span></div>
        <div class="scroll-x"><table class="table">
          <thead><tr><th class="fit"><span class="sr-only">${t('common.default')}</span></th><th>${t('php.version')}</th><th>${t('services.composer_needs')}</th>
            <th>${t('php.used_by')}</th><th class="fit"><span class="sr-only">${t('sites.actions')}</span></th></tr></thead>
          <tbody>${data.installed.map((c) => html`<tr key=${c.version}>
            <td class="fit"><${DefaultStar} on=${c.default} label=${t('php.set_default_of', { version: c.version })} onLabel=${t('php.default_version')}
              onClick=${() => act(api('PUT', '/composer/default', { version: c.version }), refresh)} /></td>
            <td><span class="ver">${c.version}</span>${c.default && html` <span class="badge accent">${t('php.default')}</span>`}</td>
            <td class="t2">${t('services.composer_needs_php', { php: c.min_php })}</td>
            <td class="t2"><${SiteLinks} names=${c.used_by} /></td>
            <td><div class="row-acts"><${RemoveButton} blocked=${blocked(c)} onClick=${() => remove(c)} /></div></td>
          </tr>`)}</tbody>
        </table></div>
      </section>
      <p class="note"><${Icon} name="info" size=${13} />${t('services.composer_pin_note')}</p>`}

    <div class="sec"><h2>${t('php.available_short')}</h2><span class="small muted">${t('services.composer_available_hint')}</span><span class="grow"></span>
      ${available === null && html`<button class="btn sm" disabled=${busy} onClick=${loadAvailable}>${busy && html`<span class="spin"><${Icon} name="rotate-cw" size=${13} /></span>`}${t('services.load_available')}</button>`}</div>
    ${available !== null && html`<section class="card">
      ${available.map((a) => html`<div class="setrow" key=${a.version}>
        <span class="row-icon"><${Icon} name="package" /></span>
        <span class="row-text"><b><span class="ver">${a.version}</span></b><span>${t('services.composer_needs_php', { php: a.min_php })}</span></span>
        ${a.installed
          ? html`<span class="status"><${Icon} name="check" size=${13} />${t('common.installed')}</span>`
          : html`<button class="btn sm" onClick=${() => install(a.version)}><${Icon} name="download" size=${13} />${t('common.install')}</button>`}
      </div>`)}
    </section>`}`;
}
