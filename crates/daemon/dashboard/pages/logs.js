import { html } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { useApi } from '../api.js';
import { Loading } from '../ui/controls.js';
import { LogView } from '../ui/logview.js';

export function Logs() {
  const [{ data: sources, error }] = useApi('/logs');
  if (!sources) return html`<div class="page-head"><h1>${t('logs.title')}</h1></div><${Loading} error=${error} />`;
  return html`
    <div class="page-head"><div class="grow"><h1>${t('logs.title')}</h1></div></div>
    <${LogView} sources=${sources} initial="daemon" showPath />`;
}
