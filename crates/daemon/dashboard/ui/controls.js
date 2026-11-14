// Small building blocks shared by the pages.
import { html, useState, useRef, useEffect, useLayoutEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';
import { copy } from './feedback.js';

/** On/off switch. */
export function Switch({ checked, onChange, label, disabled = false }) {
  return html`<button class="switch" role="switch" aria-checked=${checked ? 'true' : 'false'} aria-label=${label}
    disabled=${disabled} onClick=${() => onChange(!checked)}><span></span></button>`;
}

// Left/right (and Home/End) move between the buttons of a group, as in a
// native radio group or tab list.
function arrowKeys(e) {
  const keys = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 };
  const buttons = [...e.currentTarget.querySelectorAll('button')];
  const at = buttons.indexOf(document.activeElement);
  if (at < 0) return;
  let next = null;
  if (e.key in keys) next = (at + keys[e.key] + buttons.length) % buttons.length;
  if (e.key === 'Home') next = 0;
  if (e.key === 'End') next = buttons.length - 1;
  if (next === null) return;
  e.preventDefault();
  buttons[next].focus();
  buttons[next].click();
}

let selectIds = 0;

const alignRight = (b, w) => (b.left + b.width / 2 > innerWidth / 2 ? b.right - w >= 8 : b.left + w > innerWidth - 8);

/**
 * Dropdown: `options` = [{ value, label, hint, disabled, divider }]; `hint` is a second, muted
 * line and `divider` draws a line above the option. It shows `value` until the parent changes it.
 * The list is fixed to the window, so a card or a scrolling area never clips it.
 */
export function Select({ value, options, onChange, label, id }) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const [place, setPlace] = useState(null);
  const button = useRef(null);
  const list = useRef(null);
  const uid = useRef(`dd${++selectIds}`);
  const current = options.find((o) => o.value === value);
  const usable = (i) => options[i] && !options[i].disabled;

  const show = () => {
    const at = options.findIndex((o) => o.value === value);
    setActive(usable(at) ? at : options.findIndex((o) => !o.disabled));
    setPlace(null);
    setOpen(true);
  };
  const close = (focus = true) => {
    setOpen(false);
    if (focus) button.current?.focus();
  };
  const pick = (o) => {
    if (o.disabled) return;
    close();
    if (o.value !== value) onChange(o.value);
  };
  const step = (from, dir) => {
    for (let i = from + dir; i >= 0 && i < options.length; i += dir) if (usable(i)) return i;
    return from;
  };

  // Below the button, aligned to its edge nearest the window's edge; above it or on the other
  // side when the window is too small.
  useLayoutEffect(() => {
    if (!open || place || !list.current) return;
    const b = button.current.getBoundingClientRect();
    const w = list.current.offsetWidth;
    const h = list.current.offsetHeight;
    const up = b.bottom + 6 + h > innerHeight - 8 && b.top - 6 - h > 8;
    setPlace({
      minWidth: `${b.width}px`,
      ...(up ? { bottom: `${innerHeight - b.top + 6}px` } : { top: `${b.bottom + 6}px` }),
      ...(alignRight(b, w) ? { right: `${innerWidth - b.right}px` } : { left: `${b.left}px` }),
    });
  }, [open, place]);
  // Focus once placed: a hidden list cannot take it.
  useEffect(() => {
    if (open && place) list.current?.focus({ preventScroll: true });
  }, [open, place]);
  useEffect(() => {
    list.current?.querySelector(`#${uid.current}-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [active, place]);
  // A click elsewhere, scrolling or resizing closes it.
  useEffect(() => {
    if (!open) return;
    const onDown = (e) => !button.current?.contains(e.target) && !list.current?.contains(e.target) && close(false);
    const onMove = (e) => !list.current?.contains(e.target) && close(false);
    document.addEventListener('mousedown', onDown);
    addEventListener('scroll', onMove, true);
    addEventListener('resize', onMove);
    return () => {
      document.removeEventListener('mousedown', onDown);
      removeEventListener('scroll', onMove, true);
      removeEventListener('resize', onMove);
    };
  }, [open]);

  const onButtonKey = (e) => {
    if (open) return onListKey(e);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      show();
    }
  };
  const onListKey = (e) => {
    const keys = {
      ArrowDown: () => setActive(step(active, 1)),
      ArrowUp: () => setActive(step(active, -1)),
      Home: () => setActive(step(-1, 1)),
      End: () => setActive(step(options.length, -1)),
      Enter: () => options[active] && pick(options[active]),
      ' ': () => options[active] && pick(options[active]),
      Escape: () => close(),
    };
    if (e.key === 'Tab') return close(false);
    if (!keys[e.key]) return;
    e.preventDefault();
    keys[e.key]();
  };

  return html`<button ref=${button} type="button" class="select" id=${id} aria-label=${label} aria-haspopup="listbox"
      aria-expanded=${open ? 'true' : 'false'} onClick=${() => (open ? close() : show())} onKeyDown=${onButtonKey}>
      <span class="select-value">${current ? current.label ?? current.value : ''}</span><${Icon} name="chevron-down" size=${14} /></button>
    ${open && html`<div ref=${list} class="dropdown" role="listbox" tabindex="-1" aria-label=${label}
      aria-activedescendant=${active >= 0 ? `${uid.current}-${active}` : undefined}
      style=${place ?? { visibility: 'hidden', top: '0px', left: '0px' }} onKeyDown=${onListKey}>
      ${options.map((o, i) => html`
        ${o.divider && html`<div class="dropdown-sep" role="separator"></div>`}
        <div id=${`${uid.current}-${i}`} role="option" class="dropdown-item ${i === active ? 'active' : ''}"
          aria-selected=${o.value === value ? 'true' : 'false'} aria-disabled=${o.disabled ? 'true' : undefined}
          onMouseMove=${() => !o.disabled && i !== active && setActive(i)} onClick=${() => pick(o)}>
          <span class="tick">${o.value === value && html`<${Icon} name="check" size=${14} />`}</span>
          <span class="grow"><span class="dropdown-label" title=${o.label ?? o.value}>${o.label ?? o.value}</span>
            ${o.hint && html`<span class="dropdown-hint">${o.hint}</span>`}</span>
        </div>`)}
    </div>`}`;
}

/** Tabs (underlined) or, with role="radiogroup", a segmented control: `tabs` = [{ id, label, count? }]. */
export function Tabs({ value, tabs, onChange, label, role = 'tablist' }) {
  const item = role === 'tablist' ? 'tab' : 'radio';
  const state = role === 'tablist' ? 'aria-selected' : 'aria-checked';
  return html`<div class=${role === 'tablist' ? 'tabs' : 'seg'} role=${role} aria-label=${label} onKeyDown=${arrowKeys}>
    ${tabs.map((tab) => html`<button role=${item} ...${{ [state]: tab.id === value ? 'true' : 'false' }}
      tabindex=${tab.id === value ? 0 : -1} onClick=${() => onChange(tab.id)}>${tab.label}${tab.count ? html`<span class="n">${tab.count}</span>` : ''}</button>`)}
  </div>`;
}

/** The star of a version list: filled on the default (`on`), else a button that makes this one the default. */
export function DefaultStar({ on, label, onLabel, onClick }) {
  return on
    ? html`<span class="star on" role="img" aria-label=${onLabel} title=${onLabel}><${Icon} name="star" size=${15} /></span>`
    : html`<button class="star" aria-label=${label} title=${label} onClick=${onClick}><${Icon} name="star" size=${15} /></button>`;
}

/** Trash button of a version list; disabled with the reason as its tooltip when `blocked` says why. */
export function RemoveButton({ blocked, onClick }) {
  const label = t('common.remove');
  return html`<button class="btn ghost square sm" disabled=${!!blocked} aria-label=${label} title=${blocked || label}
    onClick=${onClick}><${Icon} name="trash" size=${15} /></button>`;
}

/**
 * A button that opens a menu of actions (the ⋯ of a row): `items` = [{ label, icon, hint, onClick,
 * danger, disabled, title, divider }]; `divider` draws a line above the item. The menu is fixed to
 * the window, so a table or a scrolling area never clips it.
 */
export function MenuButton({ label, items, icon = 'ellipsis', small = true, text = null }) {
  const [open, setOpen] = useState(false);
  const [place, setPlace] = useState(null);
  const button = useRef(null);
  const menu = useRef(null);
  const close = (focus = true) => {
    setOpen(false);
    if (focus) button.current?.focus();
  };
  useLayoutEffect(() => {
    if (!open || place || !menu.current) return;
    const b = button.current.getBoundingClientRect();
    const w = menu.current.offsetWidth;
    const h = menu.current.offsetHeight;
    const up = b.bottom + 4 + h > innerHeight - 8 && b.top - 4 - h > 8;
    setPlace({
      ...(up ? { bottom: `${innerHeight - b.top + 4}px` } : { top: `${b.bottom + 4}px` }),
      ...(b.right - w >= 8 ? { right: `${innerWidth - b.right}px` } : { left: `${b.left}px` }),
    });
  }, [open, place]);
  useEffect(() => {
    if (!open || !place) return;
    menu.current?.querySelector('[role="menuitem"]:not([aria-disabled="true"])')?.focus();
    const onDown = (e) => !button.current?.contains(e.target) && !menu.current?.contains(e.target) && close(false);
    const onMove = (e) => !menu.current?.contains(e.target) && close(false);
    document.addEventListener('mousedown', onDown);
    addEventListener('scroll', onMove, true);
    addEventListener('resize', onMove);
    return () => {
      document.removeEventListener('mousedown', onDown);
      removeEventListener('scroll', onMove, true);
      removeEventListener('resize', onMove);
    };
  }, [open, place]);
  const onKey = (e) => {
    const usable = [...menu.current.querySelectorAll('[role="menuitem"]:not([aria-disabled="true"])')];
    const at = usable.indexOf(document.activeElement);
    const moves = { ArrowDown: 1, ArrowUp: -1 };
    if (e.key === 'Escape') { e.preventDefault(); close(); }
    else if (e.key === 'Tab') close(false);
    else if (e.key in moves && usable.length) {
      e.preventDefault();
      usable[(at + moves[e.key] + usable.length) % usable.length].focus();
    }
  };
  const run = (item) => {
    if (item.disabled) return;
    close();
    item.onClick?.();
  };
  return html`<button ref=${button} type="button" class="btn ghost ${text ? '' : 'square'} ${small ? 'sm' : ''}" aria-label=${label}
      title=${label} aria-haspopup="menu" aria-expanded=${open ? 'true' : 'false'}
      onClick=${() => { setPlace(null); setOpen(!open); }}><${Icon} name=${icon} size=${15} />${text}</button>
    ${open && html`<div ref=${menu} class="menu" role="menu" aria-label=${label} onKeyDown=${onKey}
      style=${place ?? { visibility: 'hidden', top: '0px', left: '0px' }}>
      ${items.filter(Boolean).map((item) => html`${item.divider && html`<hr />`}
        <button type="button" role="menuitem" class="mi ${item.danger ? 'danger' : ''}" tabindex="-1"
          aria-disabled=${item.disabled ? 'true' : undefined} title=${item.title} onClick=${() => run(item)}>
          ${item.icon && html`<${Icon} name=${item.icon} size=${14} />`}
          <span class="grow"><span class="lab">${item.label}</span>${item.hint && html`<span class="hint">${item.hint}</span>`}</span>
        </button>`)}
    </div>`}`;
}

/** Sums `values` (one per second) into `buckets` points, oldest first. */
export function buckets(values, count) {
  const size = Math.max(1, Math.ceil(values.length / count));
  const out = [];
  for (let i = 0; i < values.length; i += size) out.push(values.slice(i, i + size).reduce((a, b) => a + (b ?? 0), 0));
  return out;
}

/** A small line of the last values, without axes: `values` one per second, drawn as 30 points. */
export function Spark({ values, width = 64, height = 20 }) {
  const points = buckets(values, 30);
  const max = Math.max(1, ...points);
  const step = width / Math.max(1, points.length - 1);
  const d = points.map((v, i) => `${(i * step).toFixed(1)},${(height - 2 - (v / max) * (height - 4)).toFixed(1)}`).join(' ');
  return html`<svg class="spark" width=${width} height=${height} viewBox="0 0 ${width} ${height}" aria-hidden="true">
    <polyline points=${d} fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" /></svg>`;
}

/** An area chart without axes that fills its box's width: `values` one per second. */
export function AreaChart({ values, height = 64, count = 60 }) {
  const points = buckets(values, count);
  const max = Math.max(1, ...points);
  const w = 320;
  const step = w / Math.max(1, points.length - 1);
  const line = points.map((v, i) => `${(i * step).toFixed(1)},${(height - 2 - (v / max) * (height - 6)).toFixed(1)}`).join(' ');
  return html`<svg class="area-chart" viewBox="0 0 ${w} ${height}" preserveAspectRatio="none" style=${`height: ${height}px`} aria-hidden="true">
    <polygon points=${`0,${height} ${line} ${w},${height}`} class="area" />
    <polyline points=${line} class="ln" /></svg>`;
}

/** Copies `text` (an address, a path); `label` names what is copied. */
export function CopyButton({ text, label }) {
  const name = `${t('common.copy')} ${label}`.trim();
  return html`<button type="button" class="copy" aria-label=${name} title=${name} onClick=${(e) => { e.stopPropagation(); copy(text); }}>
    <${Icon} name="copy" size=${13} /></button>`;
}

/** A settings row: icon, title and description, control on the right. */
export function SetRow({ icon, title, desc, tone = '', children }) {
  return html`<div class="setrow ${tone}">
    ${icon && html`<span class="row-icon"><${Icon} name=${icon} /></span>`}
    <span class="row-text"><b>${title}</b>${desc && html`<span>${desc}</span>`}</span>
    ${children}
  </div>`;
}

export function Empty({ icon = 'info', title, children }) {
  return html`<div class="empty"><span class="ico"><${Icon} name=${icon} size=${20} /></span><b>${title}</b>${children && html`<div>${children}</div>`}</div>`;
}

/** Placeholder while a page loads, or its error. */
export function Loading({ error, height = 220 }) {
  if (error) return html`<div class="card pad" role="alert"><span class="badge danger">${t('common.error', { message: error })}</span></div>`;
  return html`<div class="skeleton" style=${`height:${height}px`} aria-label=${t('common.loading')} role="status"></div>`;
}
