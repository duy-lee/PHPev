import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, res, useApi } from '../api.js';
import { mb, uptime, fmtMs } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { Chart } from '../ui/chart.js';
import { Empty, Loading } from '../ui/controls.js';

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

  if (!live) return html`<div class="page-head"><h1>${t('perf.title')}</h1></div><${Loading} />`;
  const r = live.requests;
  const x = r.per_second.map((_, i) => i - r.per_second.length + 1);
  const ago = Math.max(0, (now - r.generated_ms) / 1000).toFixed(1) + ' s';
  const names = [...new Set(procHistory.flatMap((h) => Object.keys(h.groups)))].sort();
  const px = procHistory.map((_, i) => i - procHistory.length + 1);
  const maxCount = Math.max(1, ...r.histogram.map(([, c]) => c));

  return html`
    <div class="page-head">
      <div class="grow"><h1>${t('perf.title')}</h1><p>${visible ? t('perf.live', { ago }) : t('perf.paused')}</p></div>
      <div class="row-flex">
        <label class="sr-only" for="perf-host">${t('perf.all_sites')}</label>
        <select class="select" id="perf-host" value=${host} onChange=${(e) => { setHost(e.target.value); setProcHistory([]); }}>
          <option value="">${t('perf.all_sites')}</option>
          ${r.hosts.map((h) => html`<option value=${h}>${h}</option>`)}
        </select>
        <label class="check"><input type="checkbox" checked=${withStatic} onChange=${(e) => setWithStatic(e.target.checked)} />${t('perf.include_static')}</label>
      </div>
    </div>

    <section class="card pad">
      <div class="row-flex" style="gap: 36px; align-items: flex-end">
        <div><div class="small muted">${t('perf.requests', { window: r.window_secs })}</div><div class="display" style="font-size: 32px; font-weight: 600; letter-spacing: -.02em">${r.total}</div></div>
        <div><div class="small muted">p50</div><b style="font-size: 18px">${fmtMs(r.p50_ms)}</b></div>
        <div><div class="small muted">p95</div><b style="font-size: 18px">${fmtMs(r.p95_ms)}</b></div>
        <div><div class="small muted">p99</div><b style="font-size: 18px">${fmtMs(r.p99_ms)}</b></div>
        <span class="grow"></span>
        <div class="row-flex">
          <span class="pill ok">2xx ${r.status.s2xx}</span><span class="pill">3xx ${r.status.s3xx}</span>
          <span class="pill warn">4xx ${r.status.s4xx}</span><span class="pill danger">5xx ${r.status.s5xx}</span>
        </div>
      </div>
    </section>

    <div class="charts">
      <${Chart} title=${t('perf.rps')} labels=${['req/s']} data=${[x, r.per_second]} />
      <${Chart} title=${t('perf.latency')} labels=${[t('perf.p50'), t('perf.p95')]} data=${[x, r.p50_per_second, r.p95_per_second]} />
      ${names.length > 0 && html`<${Chart} title=${t('perf.cpu')} labels=${names}
        data=${[px, ...names.map((n) => procHistory.map((h) => h.groups[n]?.cpu ?? null))]} />`}
    </div>

    <section class="card">
      <div class="card-head"><h2>${t('perf.histogram')}</h2></div>
      <div class="hist">${r.histogram.map(([label, count]) => html`<div class="hist-row">
        <span class="mono">${label}</span><span class="bar" style=${`width:${(count / maxCount) * 100}%`}></span><span class="small muted">${count}</span></div>`)}</div>
    </section>

    <section class="card">
      <div class="card-head"><h2>${t('perf.slowest')}</h2></div>
      ${r.slowest.length === 0 ? html`<${Empty} icon="chart" title=${t('perf.none', { window: r.window_secs })} />` : html`<div class="scroll-x"><table class="table">
        <thead><tr><th>${t('perf.route')}</th><th class="num">${t('perf.count')}</th><th class="num">${t('perf.avg')}</th><th class="num">p95</th><th class="num">${t('perf.max')}</th></tr></thead>
        <tbody>${r.slowest.map((s) => html`<tr><td class="mono">${s.method} ${s.host}${s.route}</td><td class="num">${s.count}</td><td class="num">${fmtMs(s.avg_ms)}</td><td class="num">${fmtMs(s.p95_ms)}</td><td class="num">${fmtMs(s.max_ms)}</td></tr>`)}</tbody>
      </table></div>
      <div class="card-foot"><span class="small muted">${t('perf.slowest_note', { n: 2000 })}</span></div>`}
    </section>

    <section class="card">
      <div class="card-head"><h2>${t('perf.processes')}</h2></div>
      <div class="scroll-x"><table class="table">
        <thead><tr><th>${t('perf.process')}</th><th class="num">pid</th><th class="num">${t('perf.cpu')}</th><th class="num">${t('perf.ram')}</th></tr></thead>
        <tbody>${live.processes.map((p) => html`<tr><td>${p.name}</td><td class="num mono">${p.pid}</td><td class="num">${p.cpu_percent.toFixed(1)}%</td><td class="num">${mb(p.memory_kb)}</td></tr>`)}</tbody>
      </table></div>
    </section>`;
}
