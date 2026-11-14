// An ini file (php.ini, my.ini) in a textarea, saved through the API with the
// stamp it was read with, so a file changed meanwhile is not overwritten.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { showToast } from './feedback.js';
import { Icon } from '../icons.js';
import { Loading } from './controls.js';

// A textarea reports `\n` line endings whatever the file uses.
const lf = (text) => text.replace(/\r\n/g, '\n');

/** `path`: the API path to GET and PUT `{ text, stamp }`; reloaded each time `active` turns on, unless edited. */
export function IniEditor({ path, label, hint, active = true, onSaved }) {
  const [ini, setIni] = useState(null);
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const dirty = ini?.text !== undefined && text !== lf(ini.text);
  const load = () => api('GET', path).then((r) => {
    setIni(r);
    setText(lf(r.text));
  }, (e) => setIni({ error: e.message }));
  useEffect(() => {
    if (active && !dirty) load();
  }, [active, path]);
  if (!ini) return html`<${Loading} height=${200} />`;
  if (ini.error) return html`<span class="badge danger">${t('common.error', { message: ini.error })}</span>`;
  const save = async () => {
    if (!dirty || busy) return;
    setBusy(true);
    try {
      const r = await api('PUT', path, { text, stamp: ini.stamp });
      showToast(r.message);
      onSaved?.();
      await load();
    } catch (e) {
      showToast(t('common.error', { message: e.message }), true);
      // Keep the edit to fix it; take the stamp of the file as it is now (put back after a failed start).
      await api('GET', path).then(setIni, () => {});
    }
    setBusy(false);
  };
  const onKey = (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      save();
    }
  };
  return html`<div class="ini-editor">
    <textarea class="code" spellcheck="false" aria-label=${label} aria-describedby="ini-hint" value=${text}
      onInput=${(e) => setText(e.target.value)} onKeyDown=${onKey}></textarea>
    <div class="row-flex">
      ${dirty
        ? html`<span class="small warn-t row-flex" style="gap: 6px"><${Icon} name="circle-alert" size=${13} />${t('php.ini_unsaved')}</span>`
        : html`<span class="small muted grow" id="ini-hint">${hint}</span>`}
      <span class="grow"></span>
      <button class="btn ghost sm" disabled=${!dirty || busy} onClick=${() => setText(lf(ini.text))}>${t('php.ini_discard')}</button>
      <button class="btn primary sm" disabled=${!dirty || busy} onClick=${save}>${busy ? t('php.ini_saving') : t('php.ini_save')}</button>
    </div>
  </div>`;
}
