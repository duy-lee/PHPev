// Edit site: the button after the address in a site's header and the dialog it opens,
// with the site's name (its address) and the folder it serves.
import { html, useState } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { showToast } from '../ui/feedback.js';
import { Dialog } from '../ui/dialog.js';
import { Icon } from '../icons.js';

/** Same rule as the server: a DNS label of a-z, 0-9 and -, not starting or ending with -, up to 63. */
const VALID_NAME = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;
/** Relative to the project: not absolute, no drive, no `..` (the server checks again, and that the folder exists). */
const VALID_DOCROOT = (path) => !/^[/\\]|:/.test(path) && !path.split(/[/\\]/).some((part) => part.trim() === '..');
/** The way a folder is typed and shown: from the project folder, `./public`, and `./` for the folder itself. */
const shown = (rel) => (rel === '.' ? './' : `./${rel}`);
/** What the server keeps for what was typed (`./public/` and `public` give `public`, `./` gives `.`, nothing gives ''). */
const kept = (text) => {
  if (text.trim() === '') return '';
  return text.split(/[/\\]/).map((p) => p.trim()).filter((p) => p && p !== '.').join('/') || '.';
};

/** `http://shop.localhost:8080` with the site's name swapped for `name`, without the scheme. */
function addressOf(url, oldName, name) {
  const u = new URL(url);
  u.hostname = u.hostname.replace(new RegExp(`^${oldName}\\.`), `${name}.`);
  return u.host;
}

function EditDialog({ d, onClose, reload, onRenamed }) {
  const { site, docroot_auto: auto, docroot_picked: picked } = d;
  const [name, setName] = useState(site.name);
  const [folder, setFolder] = useState(picked ? shown(picked) : '');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);

  const newName = name.trim();
  const newFolder = folder.trim();
  const nameOk = VALID_NAME.test(newName);
  const folderOk = !newFolder || VALID_DOCROOT(newFolder);
  const nameChanged = newName !== site.name;
  const folderChanged = kept(newFolder) !== (picked ?? '');
  const base = `/sites/${encodeURIComponent(site.name)}`;

  const save = async (e) => {
    e.preventDefault();
    if (!nameOk || !folderOk || !(nameChanged || folderChanged) || busy) return;
    setBusy(true);
    setError('');
    try {
      // The folder first, while the site still has its old name: a refused folder changes nothing.
      if (folderChanged) await api('PUT', `${base}/docroot`, { docroot: kept(newFolder) || null });
      if (nameChanged) {
        const r = await api('PUT', `${base}/name`, { name: newName });
        showToast(r.message);
        onRenamed(r.name);
      } else {
        showToast(t('msg.site_docroot', { name: site.name }));
        onClose();
        reload();
      }
    } catch (err) {
      setError(err.message);
      setBusy(false);
      if (folderChanged) reload();
    }
  };

  return html`<${Dialog} title=${t('site.edit_title', { name: site.name })} onClose=${onClose}>
    <form onSubmit=${save} style="display: contents">
      <div class="stack" style="display: grid; gap: 6px">
        <label for="edit-site-name" style="font-weight: 600">${t('site.name')}</label>
        <div class="field"><input id="edit-site-name" aria-describedby="edit-site-name-hint" value=${name}
          spellcheck="false" autocomplete="off" maxlength="63" onInput=${(e) => setName(e.target.value)} /></div>
        <p class="small ${newName && !nameOk ? '' : 'muted'}" id="edit-site-name-hint" style="margin: 0" role=${newName && !nameOk ? 'alert' : undefined}>
          ${nameOk ? html`<span class="mono">${addressOf(site.url, site.name, newName)}</span>` : t('site.rename_invalid')}</p>
      </div>
      <div class="stack" style="display: grid; gap: 6px">
        <label for="edit-site-docroot" style="font-weight: 600">${t('site.docroot')}</label>
        <div class="field"><input id="edit-site-docroot" aria-describedby="edit-site-docroot-hint" value=${folder}
          placeholder=${shown(auto)} spellcheck="false" autocomplete="off"
          onInput=${(e) => setFolder(e.target.value)} /></div>
        <p class="small ${folderOk ? 'muted' : ''}" id="edit-site-docroot-hint" style="margin: 0" role=${folderOk ? undefined : 'alert'}>
          ${folderOk ? t('site.docroot_hint', { auto: shown(auto) }) : t('site.docroot_invalid')}</p>
      </div>
      ${nameChanged && html`<p class="callout" style="margin: 0">${t('site.rename_note')}</p>`}
      ${error && html`<p class="callout warn" role="alert" style="margin: 0">${error}</p>`}
      <div class="dialog-actions">
        <button type="button" class="btn quiet" onClick=${onClose}>${t('common.cancel')}</button>
        <button type="submit" class="btn primary" disabled=${!nameOk || !folderOk || !(nameChanged || folderChanged) || busy}>${t('site.edit_save')}</button>
      </div>
    </form>
  </${Dialog}>`;
}

/** The pencil after the address; `onRenamed(name)` runs once the server accepted a new name, and the page moves to it. */
export function EditSite({ d, reload, onRenamed }) {
  const [open, setOpen] = useState(false);
  const label = t('site.edit');
  return html`<button class="icon-btn ghost sm" aria-label=${label} title=${label} onClick=${() => setOpen(true)}>
      <${Icon} name="edit" size=${15} /></button>
    ${open && html`<${EditDialog} d=${d} reload=${reload} onClose=${() => setOpen(false)} onRenamed=${(name) => { setOpen(false); onRenamed(name); }} />`}`;
}
