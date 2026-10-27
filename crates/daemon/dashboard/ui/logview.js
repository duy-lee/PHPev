// A live log: source picker, filter, follow, and the lines streamed from the daemon.
import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';
import { Switch } from './controls.js';

const MAX_LOG_LINES = 5000;

/** `sources` = [{ id, label, path }]; `initial` = the id shown first. */
export function LogView({ sources, initial, showPath = false }) {
  const [source, setSource] = useState(initial ?? sources[0]?.id);
  const [lines, setLines] = useState([]);
  const [follow, setFollow] = useState(true);
  const [filter, setFilter] = useState('');
  const box = useRef(null);

  useEffect(() => {
    setLines([]);
    if (!source) return;
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

  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [lines, follow, filter]);

  const needle = filter.toLowerCase();
  const shown = needle ? lines.filter((l) => l.toLowerCase().includes(needle)) : lines;
  const current = sources.find((s) => s.id === source);
  return html`
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
    ${showPath && current && html`<p class="mono small muted" style="margin: 0">${current.path}</p>`}
    <section class="card"><pre class="logview" ref=${box}>${shown.length ? shown.join('\n') : t('logs.empty')}</pre></section>`;
}
