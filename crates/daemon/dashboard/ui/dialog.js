// In-app dialogs, replacing the browser's confirm() and prompt().
import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select, [tabindex]:not([tabindex="-1"])';

/** Modal frame: Esc closes, Tab stays inside, focus returns where it was. */
export function Dialog({ title, onClose, wide = false, children }) {
  const box = useRef(null);
  useEffect(() => {
    const before = document.activeElement;
    // Start on the first field, else on the first button (Cancel: never a destructive one).
    const first = box.current.querySelector('input:not([disabled])') ?? box.current.querySelector(FOCUSABLE);
    first?.focus();
    return () => before?.focus?.();
  }, []);
  const onKey = (e) => {
    if (e.key === 'Escape' && onClose) {
      e.stopPropagation();
      onClose();
    } else if (e.key === 'Tab') {
      const items = [...box.current.querySelectorAll(FOCUSABLE)];
      if (!items.length) return;
      const [head, tail] = [items[0], items[items.length - 1]];
      if (e.shiftKey && document.activeElement === head) {
        e.preventDefault();
        tail.focus();
      } else if (!e.shiftKey && document.activeElement === tail) {
        e.preventDefault();
        head.focus();
      }
    }
  };
  return html`<div class="scrim" onMouseDown=${(e) => e.target === e.currentTarget && onClose?.()}>
    <div class="dialog ${wide ? 'wide' : ''}" role="dialog" aria-modal="true" aria-label=${title} ref=${box} onKeyDown=${onKey}>
      <h2>${title}</h2>
      ${children}
    </div>
  </div>`;
}

let open = () => {};

/**
 * Asks before an action. Resolves to null when cancelled, else { choice }.
 * - danger: red confirm button
 * - choice: { label, confirmWord? } an extra checkbox (e.g. "also delete the
 *   databases"); with confirmWord the user must type it when the box is ticked
 */
export function confirmAction({ title, body, confirmLabel, danger = false, choice = null }) {
  return new Promise((resolve) => open({ title, body, confirmLabel, danger, choice, resolve }));
}

export function DialogHost() {
  const [ask, setAsk] = useState(null);
  const [checked, setChecked] = useState(false);
  const [typed, setTyped] = useState('');
  open = (a) => {
    setChecked(false);
    setTyped('');
    setAsk(a);
  };
  if (!ask) return null;
  const done = (result) => {
    setAsk(null);
    ask.resolve(result);
  };
  const word = ask.choice?.confirmWord;
  const needsWord = checked && word;
  const ready = !needsWord || typed.trim().toLowerCase() === word;
  return html`<${Dialog} title=${ask.title} onClose=${() => done(null)}>
    ${ask.body && html`<p>${ask.body}</p>`}
    ${ask.choice && html`<label class="check"><input type="checkbox" checked=${checked} onChange=${(e) => setChecked(e.target.checked)} />${ask.choice.label}</label>`}
    ${needsWord && html`<div style="margin-top: 14px">
      <label class="small muted" for="confirm-word">${t('dialog.type_to_confirm', { word })}</label>
      <div class="field" style="margin-top: 6px"><input id="confirm-word" value=${typed} onInput=${(e) => setTyped(e.target.value)} autocomplete="off" /></div>
    </div>`}
    <div class="dialog-actions">
      <button class="btn quiet" onClick=${() => done(null)}>${t('common.cancel')}</button>
      <button class="btn ${ask.danger ? 'danger' : 'primary'}" disabled=${!ready} onClick=${() => done({ choice: checked })}>${ask.confirmLabel}</button>
    </div>
  </${Dialog}>`;
}
