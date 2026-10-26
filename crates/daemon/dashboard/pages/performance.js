import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { Chart } from '../ui/chart.js';

const HISTORY = 60; // seconds of process history kept in the browser

export function Performance() {
  const [host, setHost] = useState('');
  const [withStatic, setWithStatic] = useState(false);
  const [live, setLive] = useState(null);
  const [procHistory, setProcHistory] = useState([]); // [{t, groups: {name: {cpu, kb}}}]
  const [visible, setVisible] = useState(document.visibilityState === 'visible');
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    const onVis = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onVis);
    const clock = setInterval(() => setNow(Date.now()), 500);
    return () => {
      document.removeEventListener('visibilitychange', onVis);
      clearInterval(clock);
    };
  }, []);

  // One stream while the tab is visible; closing it stops all sampling in the daemon.
  useEffect(() => {
    if (!visible) return;
    const source = new EventSource(`/api/v1/metrics/stream?host=${encodeURIComponent(host)}&static=${withStatic ? 1 : 0}`);
    source.onmessage = (e) => {
      const m = JSON.parse(e.data);
      setLive(m);
      const groups = {};
      for (const p of m.processes) {
        const g = (groups[p.name] ??= { cpu: 0, kb: 0 });
        g.cpu += p.cpu_percent;
        g.kb += p.memory_kb ?? 0;
      }
      setProcHistory((h) => [...h.slice(-(HISTORY - 1)), { t: m.requests.generated_ms, groups }]);
    };
    return () => source.close();
  }, [host, withStatic, visible]);

  if (!live) return html`<h1>${t('perf.title')}</h1><p>${t('common.loading')}</p>`;
  const r = live.requests;
  const x = r.per_second.map((_, i) => i - r.per_second.length + 1);
  const ago = Math.max(0, (now - r.generated_ms) / 1000).toFixed(1) + ' s';
  const names = [...new Set(procHistory.flatMap((h) => Object.keys(h.groups)))].sort();
  const px = procHistory.map((_, i) => i - procHistory.length + 1);
  const maxCount = Math.max(1, ...r.histogram.map(([, c]) => c));

  return html`
    <h1>${t('perf.title')}</h1>
    <div class="row">
      <select value=${host} onChange=${(e) => { setHost(e.target.value); setProcHistory([]); }}>
        <option value="">${t('perf.all_sites')}</option>
        ${r.hosts.map((h) => html`<option value=${h}>${h}</option>`)}
      </select>
      <label><input type="checkbox" checked=${withStatic} onChange=${(e) => setWithStatic(e.target.checked)} /> ${t('perf.include_static')}</label>
      <span class="muted">${visible ? t('perf.live', { ago }) : t('perf.paused')}</span>
    </div>
    <p>${t('perf.summary', { window: r.window_secs, total: r.total, p50: fmtMs(r.p50_ms), p95: fmtMs(r.p95_ms), p99: fmtMs(r.p99_ms) })}<br />
      <span class="muted">${t('perf.status', { s2: r.status.s2xx, s3: r.status.s3xx, s4: r.status.s4xx, s5: r.status.s5xx })}</span></p>
    <div class="cards charts">
      <${Chart} title=${t('perf.rps')} labels=${['req/s']} data=${[x, r.per_second]} />
      <${Chart} title=${t('perf.latency')} labels=${[t('perf.p50'), t('perf.p95')]} data=${[x, r.p50_per_second, r.p95_per_second]} />
      ${names.length > 0 && html`<${Chart} title=${t('perf.cpu')} labels=${names}
        data=${[px, ...names.map((n) => procHistory.map((h) => h.groups[n]?.cpu ?? null))]} />`}
    </div>
    <h2>${t('perf.histogram')}</h2>
    <div class="hist">${r.histogram.map(([label, count]) => html`<div class="hist-row">
      <span class="mono">${label}</span><span class="bar" style=${`width:${(count / maxCount) * 100}%`}></span><span>${count}</span></div>`)}</div>
    <h2>${t('perf.slowest')}</h2>
    ${r.slowest.length === 0 ? html`<p class="muted">${t('perf.none', { window: r.window_secs })}</p>` : html`<table>
      <tr><th>${t('perf.route')}</th><th>${t('perf.count')}</th><th>${t('perf.avg')}</th><th>p95</th><th>${t('perf.max')}</th></tr>
      ${r.slowest.map((s) => html`<tr><td class="mono">${s.method} ${s.host}${s.route}</td><td>${s.count}</td><td>${fmtMs(s.avg_ms)}</td><td>${fmtMs(s.p95_ms)}</td><td>${fmtMs(s.max_ms)}</td></tr>`)}
    </table>
    <p class="muted">${t('perf.slowest_note', { n: 2000 })}</p>`}
    <h2>${t('perf.processes')}</h2>
    <table><tr><th></th><th>pid</th><th>${t('perf.cpu')}</th><th>${t('perf.ram')}</th></tr>
      ${live.processes.map((p) => html`<tr><td>${p.name}</td><td class="mono">${p.pid}</td><td>${p.cpu_percent.toFixed(1)}%</td><td>${mb(p.memory_kb)}</td></tr>`)}
    </table>`;
}
