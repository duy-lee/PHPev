import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
/** Axis in the muted text colour, dashed hairline grid, Cascadia Mono numbers. */
const axis = () => ({ stroke: css('--text-3'), font: '11px "Cascadia Mono", Consolas, monospace', grid: { stroke: css('--line'), dash: [2, 4], width: 1 }, ticks: { show: false } });
const PALETTE = ['--accent', '--chart-2', '--ok', '--danger', '--text-3'];

/** Changes when light/dark changes (the chosen theme or the Windows one): the colours are read again. */
function useThemeChanges() {
  const [n, setN] = useState(0);
  useEffect(() => {
    const bump = () => setN((x) => x + 1);
    const system = matchMedia('(prefers-color-scheme: dark)');
    system.addEventListener('change', bump);
    const watch = new MutationObserver(bump);
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => {
      system.removeEventListener('change', bump);
      watch.disconnect();
    };
  }, []);
  return n;
}

// A uPlot line chart that follows `data` ([x, ...series]) and resizes with its box.
export function Chart({ title, labels, data }) {
  const box = useRef(null);
  const plot = useRef(null);
  const theme = useThemeChanges();
  const key = `${labels.join('|')}#${theme}`;
  const [failure, setFailure] = useState(null);
  useEffect(() => {
    const opts = {
      width: box.current.clientWidth,
      height: 170,
      scales: { x: { time: false } },
      legend: { show: true },
      axes: [axis(), axis()],
      series: [{ label: 's' }, ...labels.map((label, i) => ({ label, stroke: css(PALETTE[i % PALETTE.length]), width: 1.5, spanGaps: false,
        ...(i === 0 ? { fill: css('--chart-fill') } : {}) }))],
    };
    try {
      plot.current = new uPlot(opts, data, box.current);
      setFailure(null);
    } catch (e) {
      setFailure(String(e));
      return;
    }
    // Follows its card, not only the window: a card can still be growing when the chart is created.
    const observer = new ResizeObserver(() => box.current && plot.current?.setSize({ width: box.current.clientWidth, height: 170 }));
    observer.observe(box.current);
    return () => {
      observer.disconnect();
      plot.current?.destroy();
      plot.current = null;
    };
  }, [key]);
  useEffect(() => {
    plot.current?.setData(data);
  }, [data]);
  return html`<section class="card chart-card"><div class="card-head"><h2>${title}</h2></div>
    <div class="card-body">${failure && html`<span class="badge danger">${failure}</span>`}<div ref=${box}></div></div></section>`;
}
