// Services: one card per service, each with its own page (#/services/<id>).
import { html } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { useApi } from '../api.js';
import { mb } from '../format.js';
import { Icon } from '../icons.js';
import { MariaDb } from './service-mariadb.js';
import { Composer } from './service-composer.js';

const go = (hash) => (e) => {
  e.preventDefault();
  location.replace(hash);
};

function ServiceCard({ id, icon, title, sub, running, children }) {
  return html`<a href=${`#/services/${id}`} class="scard" onClick=${go(`#/services/${id}`)}>
    <span class="top">
      <span class="row-icon"><${Icon} name=${icon} /></span>
      <span class="grow"><b>${title}</b><span class="mono small muted">${sub}</span></span>
      ${running && html`<span class="dot ok" role="img" aria-label=${t('common.running')}></span>`}
    </span>
    <span class="row-flex" style="gap: 6px">${children}<span class="grow"></span>
      <span class="small muted">${t('services.manage')}</span><${Icon} name="chevron-right" size=${15} /></span>
  </a>`;
}

function MariaDbCard() {
  const [{ data }] = useApi('/mariadb', 3000);
  const running = data?.running;
  const sub = !data ? ' ' : running ? `${running.version} · 127.0.0.1:${data.port}` : data.installed.length ? data.active ?? '' : t('services.not_installed_short');
  return html`<${ServiceCard} id="mariadb" icon="database" title=${t('services.mariadb')} sub=${sub} running=${!!running}>
    ${data && (running
      ? html`<span class="pill ok">${t('common.running')}</span><span class="small muted">${mb(running.memory_kb)}</span>`
      : html`<span class="pill">${data.installed.length ? t('common.stopped') : t('services.not_installed_short')}</span>`)}
  </${ServiceCard}>`;
}

function ComposerCard() {
  const [{ data }] = useApi('/composer');
  const sub = !data ? ' ' : data.default ? t('services.composer_default_short', { version: data.default })
    : data.installed.length ? data.installed[0].version : t('services.not_installed_short');
  return html`<${ServiceCard} id="composer" icon="package" title=${t('services.composer')} sub=${sub}>
    ${data && html`<span class="pill">${t('services.composer_count', { count: data.installed.length })}</span>`}
  </${ServiceCard}>`;
}

// A new service = one entry here (its card and its page) + its API endpoints.
export const SERVICES = [
  { id: 'mariadb', label: 'services.mariadb', icon: 'database', card: MariaDbCard, view: MariaDb },
  { id: 'composer', label: 'services.composer', icon: 'package', card: ComposerCard, view: Composer },
];

export function ServicesToolbar({ route }) {
  const service = SERVICES.find((s) => s.id === route[1]);
  return html`<div class="row-flex" style="flex-wrap: nowrap">
    <a href="#/services" class="small muted" onClick=${go('#/services')}>${t('nav.services')}</a>
    ${service && html`<span class="muted" aria-hidden="true">/</span><span class="small" style="font-weight: 600">${t(service.label)}</span>`}
  </div>`;
}

export function Services({ route, startJob }) {
  const service = SERVICES.find((s) => s.id === route[1]);
  if (service) return html`<${service.view} startJob=${startJob} />`;
  return html`
    <div class="page-head"><div class="grow"><h1>${t('nav.services')}</h1><p>${t('services.overview_subtitle')}</p></div></div>
    <div class="site-grid" style="margin-top: 0">${SERVICES.map((s) => html`<${s.card} key=${s.id} />`)}</div>`;
}
