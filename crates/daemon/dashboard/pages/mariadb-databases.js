// Services › MariaDB › a version's databases: list, create, drop.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { size } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { copy, connectionUrl, importInto, exportFrom } from '../ui/db-actions.js';
import { Icon } from '../icons.js';
import { RemoveButton, Empty, Loading } from '../ui/controls.js';

/** API prefix of the databases of the running MariaDB (import and export live under it). */
const BASE = '/mariadb/databases';

/** Same rule as the server: letters, digits, _ $ -, up to 64. */
const VALID_NAME = /^[A-Za-z0-9_$-]{1,64}$/;
/** From this many databases on, a filter box helps. */
const FILTER_FROM = 8;

/** An icon button that needs the running server: disabled, with the reason as its tooltip, otherwise. */
function ActionButton({ icon, label, blocked, onClick }) {
  return html`<button class="icon-btn" disabled=${!!blocked} aria-label=${label} title=${blocked || label} onClick=${onClick}>
    <${Icon} name=${icon} size=${16} /></button>`;
}

function DatabaseRow({ db, running, port, onDrop, onImport, onExport }) {
  const meta = db.tables ? t('services.db_meta', { count: db.tables, size: size(db.bytes) }) : t('services.db_no_tables');
  const blocked = running ? '' : t('services.db_switch_to_use');
  return html`<li class="db-row">
    <span class="row-icon"><${Icon} name="database" size=${16} /></span>
    <span class="name">${db.name}</span>
    ${db.used_by.length > 0 && html`<span class="pill accent">${t('services.db_used_by', { sites: db.used_by.join(', ') })}</span>`}
    <span class="meta">${running ? meta : t('services.db_estimate', { meta })}</span>
    <span class="db-actions">
      <${ActionButton} icon="copy" label=${t('services.db_copy_connection')} blocked=${blocked} onClick=${() => copy(connectionUrl('root', port, db.name))} />
      <${ActionButton} icon="upload" label=${t('site.import_sql')} blocked=${blocked} onClick=${() => onImport(db)} />
      <${ActionButton} icon="download" label=${t('site.export')} blocked=${blocked} onClick=${() => onExport(db)} />
      <${RemoveButton} blocked=${running ? '' : t('services.db_switch_to_manage')} onClick=${() => onDrop(db)} />
    </span>
  </li>`;
}

/** Databases of one installed major. `running`: it is the server that runs now (the only one SQL can reach). */
export function DatabasesPanel({ major, running, port, startJob }) {
  const [state, setState] = useState(null);
  const [name, setName] = useState('');
  const [filter, setFilter] = useState('');
  const [busy, setBusy] = useState(false);

  const load = () => api('GET', `/mariadb/${major}/databases`).then((data) => setState({ data }), (e) => setState({ error: e.message }));
  // Not polled: each call starts the database client. Reloaded after every change and when the server switches.
  useEffect(() => {
    setState(null);
    load();
  }, [major, running]);

  const valid = VALID_NAME.test(name.trim());
  const create = async (e) => {
    e.preventDefault();
    if (!valid || !running || busy) return;
    setBusy(true);
    try {
      const r = await api('POST', '/mariadb/databases', { name: name.trim() });
      showToast(r.message);
      setName('');
    } catch (err) {
      showToast(t('common.error', { message: err.message }), true);
    }
    setBusy(false);
    load();
  };

  const importSql = (db) => importInto(BASE, db.name, startJob, load);
  const exportSql = (db) => exportFrom(BASE, db.name, startJob);

  const drop = async (db) => {
    const ok = await confirmAction({
      title: t('services.db_drop_title', { name: db.name }),
      body: t('services.db_drop_body', { name: db.name }),
      details: db.used_by.length > 0 && html`<p class="callout warn" role="alert">${t('services.db_drop_used', { sites: db.used_by.join(', ') })}</p>`,
      confirmLabel: t('services.db_drop'),
      danger: true,
      confirmWord: db.name,
    });
    if (ok) act(api('DELETE', `/mariadb/databases/${encodeURIComponent(db.name)}`), load);
  };

  if (!state?.data) return html`<${Loading} error=${state?.error} height=${120} />`;
  const all = state.data.databases;
  const shown = all.filter((db) => db.name.includes(filter.trim().toLowerCase()));
  const blocked = running ? '' : t('services.db_switch_to_manage');

  return html`<div class="db-panel">
    <p class="small muted" style="margin: 0">${running ? t('services.db_note_running') : t('services.db_note_stopped', { major })}</p>
    <form class="db-create" onSubmit=${create}>
      <label class="field" title=${blocked}>
        <input placeholder=${t('services.db_new_placeholder')} aria-label=${t('services.db_new')} aria-describedby=${`db-hint-${major}`}
          value=${name} disabled=${!running} spellcheck="false" autocomplete="off" maxlength="64" onInput=${(e) => setName(e.target.value)} />
      </label>
      <button class="btn primary sm" type="submit" disabled=${!running || !valid || busy} title=${blocked}>
        <${Icon} name="plus" size=${15} />${t('services.db_create')}</button>
      <span class="small muted" id=${`db-hint-${major}`}>${name && !valid ? t('services.db_name_invalid') : t('services.db_name_hint')}</span>
    </form>

    ${all.length >= FILTER_FROM && html`<label class="field" style="border-radius: 999px; max-width: 320px; background: var(--surface)"><${Icon} name="search" size=${16} />
      <input placeholder=${t('services.db_filter')} aria-label=${t('services.db_filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>`}

    ${all.length === 0
      ? html`<${Empty} icon="database" title=${t('services.db_empty')}>${running ? t('services.db_empty_hint') : ''}</${Empty}>`
      : shown.length === 0
        ? html`<${Empty} icon="search" title=${t('services.db_no_match')} />`
        : html`<ul class="db-list" aria-label=${t('services.db_list', { major })}>${shown.map((db) => html`<${DatabaseRow} key=${db.name} db=${db} running=${running} port=${port} onDrop=${drop} onImport=${importSql} onExport=${exportSql} />`)}</ul>`}
  </div>`;
}
