// Ctrl K: find a site or a page and open it. It runs no actions.
import { html, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { Icon } from '../icons.js';

/** `pages` = [{ id, label, icon }]; `onOpen(hash)` navigates. */
export function Palette({ pages, onOpen, onClose }) {
  const [query, setQuery] = useState('');
  const [sites, setSites] = useState([]);
  const [at, setAt] = useState(0);
  const input = useRef(null);
  const list = useRef(null);
  useEffect(() => {
    input.current?.focus();
    api('GET', '/sites').then(setSites, () => setSites([]));
  }, []);

  const q = query.trim().toLowerCase();
  const items = [
    ...sites
      .filter((s) => !q || [s.name, s.url, s.path].some((v) => v.toLowerCase().includes(q)))
      .map((s) => ({ key: `site:${s.name}`, group: 'nav.sites', icon: 'globe', label: s.name, hint: s.url.replace(/^https?:\/\//, ''),
        hash: `#/sites/${encodeURIComponent(s.name)}` })),
    ...pages
      .filter((p) => !q || t(p.label).toLowerCase().includes(q))
      .map((p) => ({ key: `page:${p.id}`, group: 'palette.pages', icon: p.icon, label: t(p.label), hint: '', hash: `#/${p.id}` })),
  ];
  const current = Math.min(at, Math.max(0, items.length - 1));
  useEffect(() => {
    list.current?.querySelector(`#pi-${current}`)?.scrollIntoView({ block: 'nearest' });
  }, [current]);
  const pick = (item) => item && onOpen(item.hash);
  const onKey = (e) => {
    if (e.key === 'Escape') onClose();
    else if (e.key === 'ArrowDown') { e.preventDefault(); setAt((current + 1) % Math.max(1, items.length)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setAt((current - 1 + items.length) % Math.max(1, items.length)); }
    else if (e.key === 'Enter') pick(items[current]);
  };
  return html`<div class="scrim" onMouseDown=${(e) => e.target === e.currentTarget && onClose()}>
    <div class="palette" role="dialog" aria-modal="true" aria-label=${t('palette.title')} onKeyDown=${onKey}>
      <label class="palette-in"><${Icon} name="search" />
        <input ref=${input} value=${query} placeholder=${t('palette.placeholder')} role="combobox" aria-expanded="true"
          aria-controls="palette-list" aria-activedescendant=${items[current] ? `pi-${current}` : ''} spellcheck="false"
          onInput=${(e) => { setQuery(e.target.value); setAt(0); }} />
        <span class="kbd">Esc</span></label>
      <div class="palette-list" id="palette-list" role="listbox" ref=${list}>
        ${items.length === 0 && html`<div class="palette-empty">${t('palette.none')}</div>`}
        ${items.map((item, i) => html`
          ${(i === 0 || items[i - 1].group !== item.group) && html`<div class="palette-group" role="presentation">${t(item.group)}</div>`}
          <div class="palette-item" id=${`pi-${i}`} role="option" key=${item.key}
            aria-selected=${i === current ? 'true' : 'false'} onMouseMove=${() => i !== current && setAt(i)} onClick=${() => pick(item)}>
            <${Icon} name=${item.icon} /><span class="grow">${item.label}</span>${item.hint && html`<span class="meta">${item.hint}</span>`}
          </div>`)}
      </div>
      <div class="palette-foot" aria-hidden="true">
        <span><span class="kbd">↑</span><span class="kbd">↓</span>${t('palette.move')}</span>
        <span><span class="kbd">Enter</span>${t('palette.open')}</span>
        <span><span class="kbd">Esc</span>${t('palette.close')}</span>
      </div>
    </div>
  </div>`;
}
