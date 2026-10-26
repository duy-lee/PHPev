// Toast messages and the act() helper that reports an action's outcome.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { Icon } from '../icons.js';

let push = () => {};
let nextId = 1;

/** Shows a message in the corner; `action` is an optional { label, run }. */
export function showToast(text, error = false, action = null) {
  push({ id: nextId++, text, error, action });
}

export function Toasts() {
  const [items, setItems] = useState([]);
  const dismiss = (id) => setItems((list) => list.filter((m) => m.id !== id));
  push = (msg) => {
    setItems((list) => [...list.slice(-3), msg]);
    setTimeout(() => dismiss(msg.id), msg.error ? 8000 : 4000);
  };
  return html`<div class="toasts" role="status" aria-live="polite">
    ${items.map((m) => html`<div class="toast ${m.error ? 'error' : ''}" key=${m.id}>
      <${Icon} name=${m.error ? 'error' : 'check-circle'} />
      <span class="grow">${m.text}</span>
      ${m.action && html`<a href="#" onClick=${(e) => { e.preventDefault(); dismiss(m.id); m.action.run(); }}>${m.action.label}</a>`}
      <button class="icon-btn ghost sm" aria-label=${t('common.close')} onClick=${() => dismiss(m.id)}><${Icon} name="close" size=${15} /></button>
    </div>`)}
  </div>`;
}

// Runs an API action, reports the outcome, then refreshes.
export async function act(promise, refresh) {
  try {
    const out = await promise;
    if (out?.message) showToast(out.message);
  } catch (e) {
    showToast(t('common.error', { message: e.message }), true);
  }
  refresh?.();
}
