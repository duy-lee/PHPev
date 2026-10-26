// Small building blocks shared by the pages.
import { html } from '/vendor/htm-preact-standalone.mjs';
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

/** Pick one of a few short values, e.g. PHP versions: `options` = [{ value, label }]. */
export function Picker({ value, options, onChange, label }) {
  return html`<div class="picker" role="radiogroup" aria-label=${label} onKeyDown=${arrowKeys}>
    ${options.map((o) => html`<button role="radio" aria-checked=${o.value === value ? 'true' : 'false'}
      tabindex=${o.value === value ? 0 : -1} onClick=${() => o.value !== value && onChange(o.value)}>${o.label ?? o.value}</button>`)}
  </div>`;
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
