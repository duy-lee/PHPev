import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';

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

  if (!sources) return error ? html`<p>${t('common.error', { message: error })}</p>` : html`<p>${t('common.loading')}</p>`;
  const needle = filter.toLowerCase();
  const shown = needle ? lines.filter((l) => l.toLowerCase().includes(needle)) : lines;
  const current = sources.find((s) => s.id === source);
  return html`
    <h1>${t('logs.title')}</h1>
    <div class="row">
      <select value=${source} onChange=${(e) => setSource(e.target.value)}>
        ${sources.map((s) => html`<option value=${s.id}>${s.label}</option>`)}
      </select>
      <input type="text" placeholder=${t('logs.filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} style="min-width:220px" />
      <label><input type="checkbox" checked=${follow} onChange=${(e) => setFollow(e.target.checked)} /> ${t('logs.follow')}</label>
      <span class="muted">${t('logs.lines', { count: shown.length })}</span>
    </div>
    <p class="muted mono">${current?.path}</p>
    <div class="log logview mono" ref=${box}>${shown.length ? shown.join('\n') : t('logs.empty')}</div>`;
}
