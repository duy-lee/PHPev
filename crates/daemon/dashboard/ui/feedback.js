// Toast messages and the act() helper that reports an action's outcome.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';

export let showToast = () => {};
export function Toast() {
  const [msg, setMsg] = useState(null);
  showToast = (text, error = false) => {
    setMsg({ text, error });
    setTimeout(() => setMsg(null), error ? 8000 : 3500);
  };
  return msg && html`<div class="toast ${msg.error ? 'error' : ''}">${msg.text}</div>`;
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
