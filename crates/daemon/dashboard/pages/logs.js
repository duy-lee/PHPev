import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { Icon } from '../icons.js';
import { Switch, Loading } from '../ui/controls.js';

const MAX_LOG_LINES = 5000;

export function Logs() {
  const [{ data: sources, error }] = useApi('/logs');
  const [source, setSource] = useState('daemon');
  const [lines, setLines] = useState([]);
  const [follow, setFollow] = useState(true);
  const [filter, setFilter] = useState('');
  const box = useRef(null);

  useEffect(() => {
    setLines([]);
    const es = new EventSource(`/api/v1/logs/stream?source=${encodeURIComponent(source)}&lines=500`);
    es.onmessage = (e) => {
      const chunk = JSON.parse(e.data);
      setLines((old) => {
        const next = chunk.reset ? [...old, t('logs.rotated'), ...chunk.lines] : [...old, ...chunk.lines];
        return next.length > MAX_LOG_LINES ? next.slice(-MAX_LOG_LINES) : next;
      });
    };
    return () => es.close();
  }, [source]);

  // `sources` too: lines can arrive before the log box exists.
  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [lines, follow, filter, sources]);

  if (!sources) return html`<div class="page-head"><h1>${t('logs.title')}</h1></div><${Loading} error=${error} />`;
  const needle = filter.toLowerCase();
  const shown = needle ? lines.filter((l) => l.toLowerCase().includes(needle)) : lines;
  const current = sources.find((s) => s.id === source);
  return html`
    <div class="page-head">
      <div class="grow"><h1>${t('logs.title')}</h1><p class="mono small">${current?.path}</p></div>
    </div>
    <div class="row-flex">
      <label class="sr-only" for="log-source">${t('logs.source')}</label>
      <select class="select" id="log-source" value=${source} onChange=${(e) => setSource(e.target.value)}>
        ${sources.map((s) => html`<option value=${s.id}>${s.label}</option>`)}
      </select>
      <label class="field grow" style="flex: 1 1 240px; border-radius: 999px"><${Icon} name="search" size=${16} />
        <input placeholder=${t('logs.filter')} aria-label=${t('logs.filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>
      <span class="row-flex small" style="font-weight: 600"><${Switch} checked=${follow} label=${t('logs.follow')} onChange=${setFollow} />${t('logs.follow')}</span>
      <span class="small muted">${t('logs.lines', { count: shown.length })}</span>
    </div>
    <section class="card"><pre class="logview" ref=${box}>${shown.length ? shown.join('\n') : t('logs.empty')}</pre></section>`;
}
