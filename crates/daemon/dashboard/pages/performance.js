// Monitor › Performance: live requests, latency, slowest routes and the processes of PHPev.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { mb, fmtMs } from '../format.js';
import { Chart } from '../ui/chart.js';
import { Select, Switch, Empty, Loading } from '../ui/controls.js';

const HISTORY = 60; // seconds of process history kept in the browser
/** Buckets above this take the second chart colour: slow requests. */
const SLOW_MS = 250;

/** The daemon's buckets read "≤100 ms" … "≤500 ms", then ">500 ms". */
const slowBucket = (label) => label.startsWith('>') || Number(label.match(/\d+/)?.[0] ?? 0) > SLOW_MS;

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

  const filters = (hosts) => html`<div class="acts">
    <label class="sr-only" for="perf-host">${t('perf.all_sites')}</label>
    <${Select} id="perf-host" label=${t('perf.all_sites')} value=${host} onChange=${(v) => { setHost(v); setProcHistory([]); }}
      options=${[{ value: '', label: t('perf.all_sites') }, ...hosts.map((h) => ({ value: h }))]} />
    <label class="row-flex small t2" style="gap: 6px; flex-wrap: nowrap">
      <${Switch} checked=${withStatic} label=${t('perf.include_static')} onChange=${setWithStatic} />${t('perf.static_short')}</label>
  </div>`;
  if (!live) return html`<div class="page-head"><div class="grow"><h1>${t('perf.title')}</h1></div></div><${Loading} />`;
  const r = live.requests;
  const x = r.per_second.map((_, i) => i - r.per_second.length + 1);
  const ago = `${Math.max(0, (now - r.generated_ms) / 1000).toFixed(0)} s`;
  const names = [...new Set(procHistory.flatMap((h) => Object.keys(h.groups)))].sort();
  const px = procHistory.map((_, i) => i - procHistory.length + 1);
  const maxCount = Math.max(1, ...r.histogram.map(([, c]) => c));
  const errors = r.status.s5xx;
  const errorShare = r.total ? ((errors / r.total) * 100).toFixed(1) : '0';
  const maxCpu = Math.max(100, ...live.processes.map((p) => p.cpu_percent));

  return html`
    <div class="page-head">
      <div class="grow"><h1>${t('perf.title')}</h1>
        <p>${visible ? html`<span class="live"><span class="dot"></span>${t('perf.live_short', { ago })}</span>` : t('perf.paused')}</p></div>
      ${filters(r.hosts)}
    </div>

    <div class="stats">
      <div class="stat"><div class="k">${t('perf.rps_short')}</div>
        <div class="v">${(r.total / Math.max(1, r.window_secs)).toLocaleString(LANG, { maximumFractionDigits: 1 })}<small>${t('perf.in_all', { count: r.total.toLocaleString(LANG), window: r.window_secs })}</small></div></div>
      <div class="stat"><div class="k">${t('perf.median')}</div><div class="v">${fmtMs(r.p50_ms)}</div></div>
      <div class="stat"><div class="k">p95 · p99</div><div class="v">${fmtMs(r.p95_ms)}<small>${fmtMs(r.p99_ms)}</small></div></div>
      <div class="stat"><div class="k">${t('perf.server_errors')}</div>
        <div class="v"><span class=${errors ? 'danger-t' : ''}>${errors}</span><small>5xx · ${errorShare} %</small></div></div>
    </div>

    <div class="charts">
      <${Chart} title=${t('perf.rps')} labels=${['req/s']} data=${[x, r.per_second]} />
      <${Chart} title=${t('perf.latency')} labels=${[t('perf.p50'), t('perf.p95')]} data=${[x, r.p50_per_second, r.p95_per_second]} />
    </div>

    <div class="charts">
      <section class="card">
        <div class="card-head"><h2>${t('perf.histogram')}</h2></div>
        <div class="card-body">${r.histogram.map(([label, count]) => html`<div class="hrow">
          <span>${label}</span><div><div class="bar ${slowBucket(label) ? 'slow' : ''}" style=${`width: ${(count / maxCount) * 100}%`}></div></div>
          <span class="num">${count.toLocaleString(LANG)}</span></div>`)}</div>
      </section>
      ${names.length > 0 && html`<${Chart} title=${t('perf.cpu')} labels=${names}
        data=${[px, ...names.map((n) => procHistory.map((h) => h.groups[n]?.cpu ?? null))]} />`}
    </div>

    <section class="card clip">
        <div class="card-head"><h2>${t('perf.slowest')}</h2></div>
        ${r.slowest.length === 0 ? html`<${Empty} icon="activity" title=${t('perf.none', { window: r.window_secs })} />` : html`<div class="scroll-x"><table class="table dense">
          <thead><tr><th>${t('perf.route')}</th><th class="num">${t('perf.count')}</th><th class="num">${t('perf.avg')}</th><th class="num">p95</th><th class="num">${t('perf.max')}</th></tr></thead>
          <tbody>${r.slowest.map((s) => html`<tr><td><span class="muted small">${s.host.split('.')[0]}</span> <span class="mono">${s.method} ${s.route}</span></td>
            <td class="num mono t2">${s.count}</td><td class="num mono t2">${fmtMs(s.avg_ms)}</td>
            <td class="num mono ${s.p95_ms > 300 ? 'warn-t' : ''}">${fmtMs(s.p95_ms)}</td><td class="num mono muted">${fmtMs(s.max_ms)}</td></tr>`)}</tbody>
        </table></div>
        <div class="foot">${t('perf.slowest_note', { n: 2000 })}</div>`}
    </section>

    <section class="card clip">
      <div class="card-head"><h2>${t('perf.processes')}</h2><span class="sub">${live.processes.length}</span></div>
      <div class="scroll-x"><table class="table dense">
        <thead><tr><th>${t('perf.process')}</th><th>PID</th><th style="width: 30%">${t('perf.cpu')}</th><th class="num">${t('perf.ram')}</th></tr></thead>
        <tbody>${live.processes.map((p) => html`<tr key=${p.pid}><td class="mono">${p.name}</td><td class="mono muted">${p.pid}</td>
          <td><div class="row-flex" style="flex-wrap: nowrap"><div class="meter grow" style="max-width: 160px"><i style=${`width: ${Math.min(100, (p.cpu_percent / maxCpu) * 100)}%`}></i></div>
            <span class="mono small t2 tnum">${p.cpu_percent.toFixed(1)}%</span></div></td>
          <td class="num mono t2">${mb(p.memory_kb)}</td></tr>`)}</tbody>
      </table></div>
    </section>`;
}
