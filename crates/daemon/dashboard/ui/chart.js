import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const PALETTE = ['--accent', '--ok', '--warn', '--danger', '--muted'];

// A uPlot line chart that follows `data` ([x, ...series]) and resizes with its box.
export function Chart({ title, labels, data }) {
  const box = useRef(null);
  const plot = useRef(null);
  const key = labels.join('|');
  const [failure, setFailure] = useState(null);
  useEffect(() => {
    const opts = {
      width: box.current.clientWidth,
      height: 180,
      scales: { x: { time: false } },
      legend: { show: true },
      axes: [{ stroke: css('--muted'), grid: { stroke: css('--hair') } }, { stroke: css('--muted'), grid: { stroke: css('--hair') } }],
      series: [{ label: 's' }, ...labels.map((label, i) => ({ label, stroke: css(PALETTE[i % PALETTE.length]), width: 2, spanGaps: false }))],
    };
    try {
      plot.current = new uPlot(opts, data, box.current);
      setFailure(null);
    } catch (e) {
      setFailure(String(e));
      return;
    }
    const onResize = () => plot.current?.setSize({ width: box.current.clientWidth, height: 180 });
    addEventListener('resize', onResize);
    return () => {
      removeEventListener('resize', onResize);
      plot.current?.destroy();
      plot.current = null;
    };
  }, [key]);
  useEffect(() => {
    plot.current?.setData(data);
  }, [data]);
  return html`<div class="card chart-card"><h3>${title}</h3>${failure && html`<span class="pill danger">${failure}</span>`}<div ref=${box}></div></div>`;
}
