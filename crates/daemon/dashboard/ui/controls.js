// Small building blocks shared by the pages.
import { html, useState, useRef, useEffect, useLayoutEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';

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
      <span class="select-value">${current ? current.label ?? current.value : ''}</span><${Icon} name="chevron-down" size=${15} /></button>
    ${open && html`<div ref=${list} class="dropdown" role="listbox" tabindex="-1" aria-label=${label}
      aria-activedescendant=${active >= 0 ? `${uid.current}-${active}` : undefined}
      style=${place ?? { visibility: 'hidden', top: '0px', left: '0px' }} onKeyDown=${onListKey}>
      ${options.map((o, i) => html`
        ${o.divider && html`<div class="dropdown-sep" role="separator"></div>`}
        <div id=${`${uid.current}-${i}`} role="option" class="dropdown-item ${i === active ? 'active' : ''}"
          aria-selected=${o.value === value ? 'true' : 'false'} aria-disabled=${o.disabled ? 'true' : undefined}
          onMouseMove=${() => !o.disabled && i !== active && setActive(i)} onClick=${() => pick(o)}>
          <span class="tick">${o.value === value && html`<${Icon} name="check" size=${15} />`}</span>
          <span class="grow"><span class="dropdown-label" title=${o.label ?? o.value}>${o.label ?? o.value}</span>
            ${o.hint && html`<span class="dropdown-hint">${o.hint}</span>`}</span>
        </div>`)}
    </div>`}`;
}

/** Pill tabs: `tabs` = [{ id, label, count? }]. */
export function Tabs({ value, tabs, onChange, label, role = 'tablist' }) {
  const item = role === 'tablist' ? 'tab' : 'radio';
  const state = role === 'tablist' ? 'aria-selected' : 'aria-checked';
  return html`<div class="tabs" role=${role} aria-label=${label} onKeyDown=${arrowKeys}>
    ${tabs.map((tab) => html`<button role=${item} ...${{ [state]: tab.id === value ? 'true' : 'false' }}
      tabindex=${tab.id === value ? 0 : -1} onClick=${() => onChange(tab.id)}>${tab.label}${tab.count ? html`<span class="pill danger">${tab.count}</span>` : ''}</button>`)}
  </div>`;
}

/** The star of a version list: filled on the default (`on`), else a button that makes this one the default. */
export function DefaultStar({ on, label, onLabel, onClick }) {
  return on
    ? html`<span class="icon-btn star on" role="img" aria-label=${onLabel} title=${onLabel}><${Icon} name="star" size=${16} /></span>`
    : html`<button class="icon-btn star" aria-label=${label} title=${label} onClick=${onClick}><${Icon} name="star" size=${16} /></button>`;
}

/** Trash button of a version list; disabled with the reason as its tooltip when `blocked` says why. */
export function RemoveButton({ blocked, onClick }) {
  const label = t('common.remove');
  return html`<button class="icon-btn" disabled=${!!blocked} aria-label=${label} title=${blocked || label}
    onClick=${onClick}><${Icon} name="trash" size=${16} /></button>`;
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
  return html`<div class="empty"><${Icon} name=${icon} size=${26} /><b>${title}</b>${children && html`<div>${children}</div>`}</div>`;
}

/** Placeholder while a page loads, or its error. */
export function Loading({ error, height = 220 }) {
  if (error) return html`<div class="card pad" role="alert"><span class="pill danger">${t('common.error', { message: error })}</span></div>`;
  return html`<div class="skeleton" style=${`height:${height}px`} aria-label=${t('common.loading')} role="status"></div>`;
}
