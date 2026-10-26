// A log read as entries (Laravel and PHP error logs): one row per entry with
// its level, message and time, newest first; a row opens its stack trace.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';
import { Switch, Tabs, Empty } from './controls.js';

const MAX_ENTRIES = 1000;
// [2026-10-26 10:42:13] local.ERROR: message
const LARAVEL = /^\[(\d{4}-\d\d-\d\d)[ T](\d\d:\d\d:\d\d)[^\]]*\] [\w-]+\.(\w+): (.*)$/;
// [26-Oct-2026 10:42:13 UTC] PHP Warning:  message
const PHP_ERROR = /^\[(\d\d-\w{3}-\d{4}) (\d\d:\d\d:\d\d)[^\]]*\] PHP ([A-Za-z ]+?):\s+(.*)$/;
const PHP_LEVEL = { 'Fatal error': 'ERROR', 'Parse error': 'ERROR', Warning: 'WARNING', Notice: 'NOTICE', Deprecated: 'NOTICE' };
const ERRORS = ['ERROR', 'CRITICAL', 'ALERT', 'EMERGENCY'];
const TONE = { ERROR: 'danger', CRITICAL: 'danger', ALERT: 'danger', EMERGENCY: 'danger', WARNING: 'warn', NOTICE: 'accent', INFO: 'accent' };

let nextId = 1;

/** Adds raw `lines` to `entries` (oldest first): a header line starts an entry, others continue it. */
function parse(entries, lines) {
  const out = entries.slice();
  for (const line of lines) {
    const laravel = line.match(LARAVEL);
    const php = !laravel && line.match(PHP_ERROR);
    if (laravel) {
      out.push({ id: nextId++, date: laravel[1], time: laravel[2], level: laravel[3].toUpperCase(), message: laravel[4], detail: [] });
    } else if (php) {
      out.push({ id: nextId++, date: php[1], time: php[2], level: PHP_LEVEL[php[3]] ?? 'INFO', message: php[4], detail: [] });
    } else if (out.length && out[out.length - 1].level) {
      out[out.length - 1].detail.push(line);
    } else {
      out.push({ id: nextId++, level: null, message: line, detail: [] });
    }
  }
  return out.length > MAX_ENTRIES ? out.slice(-MAX_ENTRIES) : out;
}

const levelLabel = (level) => level.charAt(0) + level.slice(1).toLowerCase();

function Entry({ entry, open, onToggle }) {
  const more = entry.detail.length > 0;
  return html`<div class="entry ${open ? 'open' : ''}">
    <button class="entry-row" aria-expanded=${more ? (open ? 'true' : 'false') : undefined} onClick=${more ? onToggle : undefined}>
      <span class="pill ${TONE[entry.level] ?? ''}">${levelLabel(entry.level)}</span>
      <span class="entry-msg">${entry.message}</span>
      <span class="entry-time mono">${entry.time}</span>
      <span class="entry-chevron">${more && html`<${Icon} name=${open ? 'chevron-down' : 'chevron-right'} size=${15} />`}</span>
    </button>
    ${open && html`<pre class="entry-detail">${entry.detail.join('\n')}</pre>`}
  </div>`;
}

/** `sources` = [{ id, label, path }]. */
export function LogEntries({ sources, initial }) {
  const [source, setSource] = useState(initial ?? sources[0]?.id);
  const [entries, setEntries] = useState([]);
  const [paused, setPaused] = useState(null); // a frozen copy while Follow is off
  const [level, setLevel] = useState('all');
  const [filter, setFilter] = useState('');
  const [open, setOpen] = useState(() => new Set());

  useEffect(() => {
    setEntries([]);
    setOpen(new Set());
    if (!source) return;
    const es = new EventSource(`/api/v1/logs/stream?source=${encodeURIComponent(source)}&lines=2000`);
    es.onmessage = (e) => {
      const chunk = JSON.parse(e.data);
      setEntries((old) => parse(chunk.reset ? [] : old, chunk.lines));
    };
    return () => es.close();
  }, [source]);

  const all = paused ?? entries;
  const structured = all.some((x) => x.level);
  const errors = all.filter((x) => ERRORS.includes(x.level)).length;
  const warnings = all.filter((x) => x.level === 'WARNING').length;
  const needle = filter.toLowerCase();
  const shown = all
    .filter((x) => level === 'all' || (level === 'errors' ? ERRORS.includes(x.level) : x.level === 'WARNING'))
    .filter((x) => !needle || x.message.toLowerCase().includes(needle) || x.detail.some((d) => d.toLowerCase().includes(needle)))
    .reverse();
  const toggle = (id) => setOpen((s) => {
    const next = new Set(s);
    next.has(id) ? next.delete(id) : next.add(id);
    return next;
  });
  const current = sources.find((s) => s.id === source);

  return html`
    <div class="row-flex">
      <label class="sr-only" for="entries-source">${t('logs.source')}</label>
      <select class="select" id="entries-source" value=${source} onChange=${(e) => setSource(e.target.value)}>
        ${sources.map((s) => html`<option value=${s.id}>${s.label}</option>`)}
      </select>
      ${structured && html`<${Tabs} role="radiogroup" label=${t('logs.level')} value=${level} onChange=${setLevel} tabs=${[
        { id: 'all', label: t('logs.all') },
        { id: 'errors', label: t('logs.errors', { count: errors }) },
        { id: 'warnings', label: t('logs.warnings', { count: warnings }) },
      ]} />`}
      <label class="field grow" style="flex: 1 1 220px; border-radius: 999px"><${Icon} name="search" size=${16} />
        <input placeholder=${t('logs.search')} aria-label=${t('logs.search')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>
      <span class="row-flex small" style="font-weight: 600"><${Switch} checked=${paused === null} label=${t('logs.follow')}
        onChange=${(on) => setPaused(on ? null : entries)} />${t('logs.follow')}</span>
    </div>
    ${current && html`<p class="mono small muted" style="margin: 0">${current.path}</p>`}
    <section class="card entries">
      ${shown.length === 0 && html`<${Empty} icon="logs" title=${all.length ? t('logs.no_match') : t('logs.empty')} />`}
      ${structured
        ? shown.slice(0, 300).map((x) => (x.level
          ? html`<${Entry} key=${x.id} entry=${x} open=${open.has(x.id)} onToggle=${() => toggle(x.id)} />`
          : html`<div class="entry"><pre class="entry-detail">${x.message}</pre></div>`))
        : shown.length > 0 && html`<pre class="logview">${shown.slice().reverse().map((x) => x.message).join('\n')}</pre>`}
      ${structured && shown.length > 300 && html`<div class="card-foot"><span class="small muted">${t('logs.showing', { count: 300, total: shown.length })}</span></div>`}
    </section>`;
}
