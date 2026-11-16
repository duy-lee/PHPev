// Services: each has its own page (#/services/<id>) and its own entry in the menu's Runtimes group.
import { html } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { MariaDb } from './service-mariadb.js';
import { Composer } from './service-composer.js';

// A new service = one entry here (its page) + its API endpoints.
export const SERVICES = [
  { id: 'mariadb', label: 'services.mariadb', icon: 'database', view: MariaDb },
  { id: 'composer', label: 'services.composer', icon: 'package', view: Composer },
];

/** Breadcrumb after "Runtimes /". */
export function ServicesCrumbs({ route }) {
  const service = SERVICES.find((s) => s.id === route[1]) ?? SERVICES[0];
  return html`<span class="cur">${t(service.label)}</span>`;
}

export function Services({ route, startJob }) {
  const service = SERVICES.find((s) => s.id === route[1]);
  // `#/services` had an overview of the services; MariaDB is the first of them.
  if (!service) {
    queueMicrotask(() => location.replace(`#/services/${SERVICES[0].id}`));
    return null;
  }
  return html`<${service.view} startJob=${startJob} route=${route} />`;
}
