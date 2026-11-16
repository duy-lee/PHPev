// Runtimes › MariaDB › Databases: the databases of one installed major; create, import, export, drop.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { size } from '../format.js';
import { copy, showToast } from '../ui/feedback.js';
import { connectionUrl, importInto, exportFrom, dropDatabase } from '../ui/db-actions.js';
import { Icon } from '../icons.js';
import { Empty, Loading, MenuButton, SiteLinks } from '../ui/controls.js';

/** API prefix of the databases of the running MariaDB (import and export live under it). */
const BASE = '/mariadb/databases';

/** Same rule as the server: letters, digits, _ $ -, up to 64. */
const VALID_NAME = /^[A-Za-z0-9_$-]{1,64}$/;
/** From this many databases on, a filter box helps. */
const FILTER_FROM = 8;

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

  const stopped = running ? '' : t('services.db_switch_to_use');
  const soon = t('site.db_soon');
  const menu = (db) => [
    { label: t('site.copy_connection'), icon: 'link', disabled: !running, title: stopped, onClick: () => copy(connectionUrl('root', port, db.name)) },
    { label: t('site.db_terminal'), icon: 'square-terminal', disabled: true, title: soon },
    { label: t('site.db_empty'), icon: 'eraser', disabled: true, title: soon },
    { label: t('services.db_drop_menu'), icon: 'trash-2', danger: true, divider: true, disabled: !running, title: running ? '' : t('services.db_switch_to_manage'),
      onClick: () => dropDatabase(db.name, db.used_by, load) },
  ];

  const all = state?.data?.databases ?? [];
  const shown = all.filter((db) => db.name.includes(filter.trim().toLowerCase()));
  return html`<section class="card clip">
    <div class="tools">
      <form class="row-flex" style="gap: 8px" onSubmit=${create}>
        <label class="field sm" style="width: 220px" title=${running ? '' : t('services.db_switch_to_manage')}><${Icon} name="plus" size=${13} />
          <input placeholder=${t('services.db_new_placeholder')} aria-label=${t('services.db_new')} aria-describedby=${`db-hint-${major}`}
            value=${name} disabled=${!running} spellcheck="false" autocomplete="off" maxlength="64" onInput=${(e) => setName(e.target.value)} /></label>
        <button class="btn sm" type="submit" disabled=${!running || !valid || busy}>${t('services.db_create')}</button>
        ${name && html`<span class="small ${valid ? 'muted' : 'danger-t'}" id=${`db-hint-${major}`}>${valid ? t('services.db_name_hint') : t('services.db_name_invalid')}</span>`}
      </form>
      <span class="grow"></span>
      ${all.length >= FILTER_FROM && html`<label class="field sm" style="width: 200px"><${Icon} name="search" size=${13} />
        <input placeholder=${t('services.db_filter')} aria-label=${t('services.db_filter')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>`}
    </div>
    ${!state ? html`<div class="card-body"><${Loading} height=${120} /></div>`
      : state.error ? html`<div class="card-body"><span class="badge danger">${state.error}</span></div>`
      : all.length === 0 ? html`<${Empty} icon="database" title=${t('services.db_empty')}>${running && html`<p>${t('services.db_empty_hint')}</p>`}</${Empty}>`
      : shown.length === 0 ? html`<${Empty} icon="search" title=${t('services.db_no_match')} />`
      : html`<div class="scroll-x"><table class="table dense" aria-label=${t('services.db_list', { major })}>
        <thead><tr><th>Database</th><th>${t('php.used_by')}</th><th class="num">${t('services.db_tables')}</th><th class="num">${t('site.size')}</th>
          <th class="fit"><span class="sr-only">${t('sites.actions')}</span></th></tr></thead>
        <tbody>${shown.map((db) => html`<tr key=${db.name}>
          <td class="mono">${db.name}</td>
          <td class="t2"><${SiteLinks} names=${db.used_by} /></td>
          <td class="num mono t2">${db.tables || html`<span class="muted">${t('services.db_no_tables')}</span>`}</td>
          <td class="num mono muted" title=${running ? '' : t('services.db_estimate_hint')}>${db.tables ? `${running ? '' : '≈ '}${size(db.bytes)}` : '—'}</td>
          <td><div class="row-acts">
            <button class="btn sm" disabled=${!running} title=${stopped} onClick=${() => importInto(BASE, db.name, startJob, load)}>
              <${Icon} name="upload" size=${13} />${t('site.import')}</button>
            <button class="btn sm" disabled=${!running || !db.tables} title=${!running ? stopped : db.tables ? '' : t('site.export_nothing')}
              onClick=${() => exportFrom(BASE, db.name, startJob)}><${Icon} name="download" size=${13} />${t('site.export')}</button>
            <${MenuButton} label=${t('site.db_more', { db: db.name })} items=${menu(db)} />
          </div></td>
        </tr>`)}</tbody>
      </table></div>`}
    <div class="foot"><${Icon} name="info" size=${13} />${running ? t('services.db_note_running') : t('services.db_note_stopped', { major })}</div>
  </section>`;
}
