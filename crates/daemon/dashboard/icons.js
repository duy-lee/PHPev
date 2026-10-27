// The one icon set of the dashboard: outline glyphs on a 24 px grid, 1.6 px
// stroke, coloured by the text. Each icon is a list of [element, attributes].
import { h } from '/vendor/htm-preact-standalone.mjs';

const p = (d) => ['path', { d }];
const c = (cx, cy, r) => ['circle', { cx, cy, r }];
const r = (x, y, width, height, rx) => ['rect', { x, y, width, height, rx }];
const e = (cx, cy, rx, ry) => ['ellipse', { cx, cy, rx, ry }];

const ICONS = {
  home: [p('M3.5 10.5 12 3.5l8.5 7'), p('M5.5 9v10.5a1 1 0 0 0 1 1H10v-5.5h4v5.5h3.5a1 1 0 0 0 1-1V9')],
  globe: [c(12, 12, 8.5), p('M3.5 12h17'), p('M12 3.5c2.3 2.4 3.5 5.2 3.5 8.5s-1.2 6.1-3.5 8.5c-2.3-2.4-3.5-5.2-3.5-8.5S9.7 5.9 12 3.5z')],
  php: [e(12, 12, 9, 6), p('M8 14.5 9 9.5h2a1.3 1.3 0 0 1 0 2.6H9.6'), p('m13 14.5 1-5'), p('M13.6 12h2.2'), p('m16 14.5 1-5')],
  database: [e(12, 6, 7.5, 2.7), p('M4.5 6v12c0 1.5 3.4 2.7 7.5 2.7s7.5-1.2 7.5-2.7V6'), p('M4.5 12c0 1.5 3.4 2.7 7.5 2.7s7.5-1.2 7.5-2.7')],
  chart: [p('M4 19.5h16'), p('M6.5 16v-4'), p('M11 16V8'), p('M15.5 16v-6'), p('M20 16V5')],
  logs: [p('M8 6.5h11'), p('M8 12h11'), p('M8 17.5h7'), c(4.5, 6.5, 0.6), c(4.5, 12, 0.6), c(4.5, 17.5, 0.6)],
  settings: [c(12, 12, 3), p('M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7')],
  lock: [r(5, 11, 14, 9.5, 2), p('M8 11V7.5a4 4 0 0 1 8 0V11')],
  unlock: [r(5, 11, 14, 9.5, 2), p('M8 11V7.5a4 4 0 0 1 7.7-1.5')],
  folder: [p('M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z')],
  terminal: [r(3.5, 4.5, 17, 15, 2.5), p('m7.5 9.5 3 2.5-3 2.5'), p('M13 14.5h3.5')],
  editor: [r(3.5, 3.5, 17, 17, 3), p('m10 9-3 3 3 3'), p('m14 9 3 3-3 3')],
  open: [p('M7 17 17 7'), p('M8 7h9v9')],
  copy: [r(8.5, 8.5, 11.5, 11.5, 2.5), p('M15.5 8.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7.5a2 2 0 0 0 2 2h2.5')],
  package: [p('M12 3.5 4.5 7.3v9.4l7.5 3.8 7.5-3.8V7.3z'), p('m4.5 7.3 7.5 3.8 7.5-3.8'), p('M12 11.1v9.4')],
  blocks: [r(4, 4, 6.5, 6.5, 1.6), r(13.5, 4, 6.5, 6.5, 1.6), r(4, 13.5, 6.5, 6.5, 1.6), p('M16.75 13.5v6.5M13.5 16.75H20')],
  restart: [p('M20 12a8 8 0 1 1-2.34-5.66L20 8.5'), p('M20 3.5v5h-5')],
  upload: [p('M12 15V4.5'), p('m7.5 9 4.5-4.5L16.5 9'), p('M4.5 15v3.5a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V15')],
  download: [p('M12 4.5V15'), p('m7.5 10.5 4.5 4.5 4.5-4.5'), p('M4.5 15v3.5a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V15')],
  search: [c(11, 11, 6.5), p('m20 20-4-4')],
  plus: [p('M12 5v14M5 12h14')],
  close: [p('m7 7 10 10M17 7 7 17')],
  check: [p('m6 12.5 4 4 8-9')],
  'check-circle': [c(12, 12, 8.5), p('m8.5 12.2 2.4 2.4 4.6-5')],
  warning: [p('M12 4 3 19.5h18z'), p('M12 10v4.5'), p('M12 17.3v.01')],
  error: [c(12, 12, 8.5), p('m15 9-6 6M9 9l6 6')],
  info: [c(12, 12, 8.5), p('M12 11v5'), p('M12 8h.01')],
  trash: [p('M4.5 7h15'), p('M10 11v6M14 11v6'), p('M6.5 7l1 13h9l1-13'), p('M9.5 7V4.5h5V7')],
  shield: [p('M12 3.5 5 6.5v5.5c0 4 2.8 7 7 8.5 4.2-1.5 7-4.5 7-8.5V6.5z'), p('m9 12 2.2 2.2L15.5 10')],
  'chevron-down': [p('m7 10 5 5 5-5')],
  'chevron-right': [p('m10 7 5 5-5 5')],
  more: [c(6, 12, 1.2), c(12, 12, 1.2), c(18, 12, 1.2)],
  'panel-left': [r(3.5, 4.5, 17, 15, 3), p('M9.5 4.5v15')],
  'collapse-left': [r(3.5, 4.5, 17, 15, 3), p('M9.5 4.5v15'), p('m15.5 10-2 2 2 2')],
  'expand-right': [r(3.5, 4.5, 17, 15, 3), p('M9.5 4.5v15'), p('m13.5 10 2 2-2 2')],
  play: [p('M8 5.5v13l10-6.5z')],
  stop: [r(6.5, 6.5, 11, 11, 2)],
  power: [p('M12 3.5v8'), p('M7.2 6.6a7.5 7.5 0 1 0 9.6 0')],
  theme: [c(12, 12, 8.5), p('M12 3.5a8.5 8.5 0 0 1 0 17z')],
  plug: [p('M9 3.5v4M15 3.5v4'), p('M6.5 7.5h11v3a5.5 5.5 0 0 1-11 0z'), p('M12 16v4.5')],
  drive: [r(3.5, 13, 17, 6.5, 2), p('M5.5 13 8 5.5h8l2.5 7.5'), p('M16.5 16.3h.01')],
  tray: [r(3.5, 4.5, 17, 15, 2.5), p('M3.5 15h17'), p('M16.5 17.3h.01')],
};

/** `<Icon name="globe" size=${18} />`; decorative unless `label` is given. */
export function Icon({ name, size = 18, label, stroke = 1.6, class: cls = '' }) {
  const parts = ICONS[name] ?? ICONS.info;
  const a11y = label ? { role: 'img', 'aria-label': label } : { 'aria-hidden': 'true' };
  return h(
    'svg',
    {
      class: `icon ${cls}`.trim(),
      width: size,
      height: size,
      viewBox: '0 0 24 24',
      fill: 'none',
      stroke: 'currentColor',
      'stroke-width': stroke,
      'stroke-linecap': 'round',
      'stroke-linejoin': 'round',
      ...a11y,
    },
    parts.map(([tag, attrs]) => h(tag, attrs)),
  );
}
