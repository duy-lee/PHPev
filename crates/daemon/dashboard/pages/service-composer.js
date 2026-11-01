import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { SetRow, DefaultStar, RemoveButton, Empty, Loading } from '../ui/controls.js';

function usage(c) {
  const parts = [t('services.composer_needs_php', { php: c.min_php })];
  if (c.default) parts.push(t('common.default_lower'));
  if (c.used_by.length) parts.push(t('services.composer_used_by', { sites: c.used_by.join(', ') }));
  return parts.join(' · ');
}

/** Services › Composer: the installed versions (one of them the default) and versions to install. */
export function Composer({ startJob }) {
  const [{ data, error }, reload] = useApi('/composer', 5000);
  const [available, setAvailable] = useState(null);
  if (!data) return html`<div class="page-head"><h1>${t('services.composer')}</h1></div><${Loading} error=${error} />`;
  const fail = (e) => showToast(t('common.error', { message: e.message }), true);
  const loadAvailable = () => api('GET', '/composer/available').then(setAvailable, fail);
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
  return html`
    <div class="page-head"><div class="grow"><h1>${t('services.composer')}</h1><p>${t('services.composer_subtitle')}</p></div></div>

    <section class="card">
      <div class="card-head"><h2>${t('services.installed')}</h2></div>
      ${data.installed.length === 0
        ? html`<${Empty} icon="package" title=${t('services.composer_none')}>${t('services.composer_none_hint')}</${Empty}>`
        : data.installed.map((c) => html`<${SetRow} icon="package" title=${`Composer ${c.version}`} desc=${usage(c)}>
            <${DefaultStar} on=${c.default} label=${t('php.set_default')} onLabel=${t('common.default')}
              onClick=${() => act(api('PUT', '/composer/default', { version: c.version }), refresh)} />
            <${RemoveButton} blocked=${blocked(c)} onClick=${() => remove(c)} />
          </${SetRow}>`)}
      <div class="card-foot"><span class="small muted">${t('services.composer_pin_note')}</span></div>
    </section>

    <section class="card">
      <div class="card-head"><h2>${t('services.composer_available')}</h2><span class="grow"></span>
        ${available === null && html`<button class="btn sm" onClick=${loadAvailable}>${t('services.load_available')}</button>`}</div>
      ${available === null
        ? html`<p class="small muted" style="margin: 0; padding: 0 22px 18px">${t('services.composer_available_hint')}</p>`
        : available.map((a) => html`<${SetRow} icon="download" title=${`Composer ${a.version}`}
            desc=${t('services.composer_needs_php', { php: a.min_php })}>
            ${a.installed
              ? html`<span class="pill ok">${t('common.installed')}</span>`
              : html`<button class="btn sm" onClick=${() => install(a.version)}>${t('common.install')}</button>`}
          </${SetRow}>`)}
    </section>`;
}
