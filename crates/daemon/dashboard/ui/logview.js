// A live log filling its page: source, level and text filters, follow, and the lines streamed from the daemon.
import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { Icon } from '../icons.js';
import { Switch, Select, Tabs } from './controls.js';

const MAX_LOG_LINES = 5000;
// The level of a line, looked for near its start: `ERROR`, `local.WARNING:`, `PHP Warning:`, `[warn]`.
const LEVEL = /\b(emergency|alert|critical|fatal error|error|err|warning|warn|deprecated|notice|info|debug|trace)\b/i;
const NORMAL = { 'fatal error': 'error', err: 'error', emergency: 'error', alert: 'error', critical: 'error', warn: 'warning', deprecated: 'warning', trace: 'debug' };
const levelOf = (line) => {
  const m = line.slice(0, 80).match(LEVEL);
  if (!m) return '';
  const l = m[1].toLowerCase();
  return NORMAL[l] ?? l;
};

let nextNo = 1;

/** `sources` = [{ id, label, path }]; `initial` = the id shown first. */
export function LogView({ sources, initial }) {
  const [source, setSource] = useState(initial ?? sources[0]?.id);
  const [lines, setLines] = useState([]); // [{ no, text, level }]
  const [follow, setFollow] = useState(true);
  const [level, setLevel] = useState('all');
  const [filter, setFilter] = useState('');
  const box = useRef(null);

  useEffect(() => {
    setLines([]);
    nextNo = 1;
    if (!source) return;
    const es = new EventSource(`/api/v1/logs/stream?source=${encodeURIComponent(source)}&lines=500`);
    es.onmessage = (e) => {
      const chunk = JSON.parse(e.data);
      const add = (chunk.reset ? [t('logs.rotated'), ...chunk.lines] : chunk.lines).map((text) => ({ no: nextNo++, text, level: levelOf(text) }));
      setLines((old) => {
        const next = [...old, ...add];
        return next.length > MAX_LOG_LINES ? next.slice(-MAX_LOG_LINES) : next;
      });
    };
    return () => es.close();
  }, [source]);

  const needle = filter.toLowerCase();
  const shown = lines
    .filter((l) => level === 'all' || (level === 'errors' ? l.level === 'error' : l.level === 'warning'))
    .filter((l) => !needle || l.text.toLowerCase().includes(needle));
  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [shown.length, follow]);
  // Scrolling up stops following; back at the bottom, it follows again.
  const onScroll = () => {
    const el = box.current;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    if (atBottom !== follow) setFollow(atBottom);
  };

  const errors = lines.filter((l) => l.level === 'error').length;
  const warnings = lines.filter((l) => l.level === 'warning').length;
  const current = sources.find((s) => s.id === source);
  return html`<div class="logs">
    <div class="tools">
      <label class="sr-only" for="log-source">${t('logs.source')}</label>
      <${Select} id="log-source" label=${t('logs.source')} value=${source} onChange=${setSource}
        options=${sources.map((s) => ({ value: s.id, label: s.label, hint: s.path.split(/[\\/]/).pop() }))} />
      <${Tabs} role="radiogroup" label=${t('logs.level')} value=${level} onChange=${setLevel} tabs=${[
        { id: 'all', label: t('logs.all') },
        { id: 'errors', label: t('logs.errors_short'), count: errors },
        { id: 'warnings', label: t('logs.warnings_short'), count: warnings },
      ]} />
      <label class="field sm" style="flex: 0 1 240px"><${Icon} name="search" size=${13} />
        <input placeholder=${t('logs.filter')} aria-label=${t('logs.filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>
      <span class="grow"></span>
      <label class="row-flex small t2" style="gap: 6px; flex-wrap: nowrap">
        <${Switch} checked=${follow} label=${t('logs.follow')} onChange=${setFollow} />${t('logs.follow')}</label>
    </div>
    <div class="logv" role="log" aria-live="off" ref=${box} onScroll=${onScroll}>
      ${shown.length === 0
        ? html`<div class="logv-empty">${lines.length ? t('logs.no_match') : t('logs.empty')}</div>`
        : shown.map((l) => html`<div class="ll" key=${l.no}><span class="ln">${l.no}</span>
            <span class="lm">${l.level ? html`<span class=${`lv-${l.level}`}>${l.text}</span>` : l.text}</span></div>`)}
    </div>
    <div class="foot">
      <span class="tnum">${t('logs.lines', { count: shown.length.toLocaleString(LANG) })}</span>
      ${follow && html`<span>·</span><span class="live"><span class="dot"></span>${t('logs.following')}</span>`}
      <span class="grow"></span>${current && html`<span class="mono ellipsis" title=${current.path}>${current.path}</span>`}
    </div>
  </div>`;
}
