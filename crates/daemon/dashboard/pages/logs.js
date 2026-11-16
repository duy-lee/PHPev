// Monitor › Logs: the logs of PHPev, PHP and MariaDB, in a viewer as tall as the window.
import { html } from '/vendor/htm-preact-standalone.mjs';
import { useApi } from '../api.js';
import { Loading } from '../ui/controls.js';
import { LogView } from '../ui/logview.js';

export function Logs() {
  const [{ data: sources, error }] = useApi('/logs');
  if (!sources) return html`<div class="logs-wait"><${Loading} error=${error} /></div>`;
  return html`<${LogView} sources=${sources} initial="daemon" />`;
}
