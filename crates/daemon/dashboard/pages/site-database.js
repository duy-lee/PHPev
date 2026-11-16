// A site's Database tab: the databases its .env names (on PHPev's MariaDB) plus the ones added by hand.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api } from '../api.js';
import { size } from '../format.js';
import { act, copy, showToast } from '../ui/feedback.js';
import { confirmAction, Dialog } from '../ui/dialog.js';
import { connectionUrl, importInto, exportFrom, dropDatabase } from '../ui/db-actions.js';
import { Icon } from '../icons.js';
import { Select, Tabs, Empty, Loading, MenuButton, CopyButton } from '../ui/controls.js';

/** Up to this many databases get a segmented picker; more get a dropdown. */
const MAX_SEGMENTS = 5;

const enc = encodeURIComponent;

/** What the server says about one database of the site; `null` until loaded. */
function useSiteDatabase(siteName, database, reloadKey = 0) {
  const [state, setState] = useState(null);
  useEffect(() => {
    setState(null);
    if (!database) return;
    api('GET', `/sites/${enc(siteName)}/database?name=${enc(database)}`).then(setState, (e) => setState({ error: e.message }));
  }, [siteName, database, reloadKey]);
  return state;
}

function databaseDesc(info) {
  if (!info) return t('common.loading');
  if (info.error) return info.error;
  if (!info.mariadb_running) return t('site.db_stopped');
  if (!info.exists) return t('site.db_missing');
  return t('site.db_size', { count: info.tables.length, size: size(info.bytes) });
}

/** Choose one of the running MariaDB's databases the site does not have yet. */
function AddDatabase({ siteName, onClose, onAdded }) {
  const [state, setState] = useState(null);
  const [pick, setPick] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api('GET', `/sites/${enc(siteName)}/databases/available`).then(
      (data) => { setState({ data }); setPick(data.databases[0]?.name ?? ''); },
      (e) => setState({ error: e.message }));
  }, [siteName]);

  const add = async () => {
    setBusy(true);
    try {
      const r = await api('POST', `/sites/${enc(siteName)}/databases`, { database: pick });
      showToast(r.message);
      onAdded(pick);
    } catch (e) {
      showToast(t('common.error', { message: e.message }), true);
      setBusy(false);
    }
  };

  const data = state?.data;
  const options = (data?.databases ?? []).map((db) => ({
    value: db.name,
    label: db.name,
    hint: db.tables ? t('site.db_size', { count: db.tables, size: size(db.bytes) }) : t('services.db_no_tables'),
  }));
  return html`<${Dialog} title=${t('site.add_db_title')} onClose=${onClose}>
    <p>${t('site.add_db_body')}</p>
    ${!state && html`<${Loading} height=${44} />`}
    ${state?.error && html`<span class="badge danger" role="alert">${t('common.error', { message: state.error })}</span>`}
    ${data && !data.running && html`<p class="callout warn" role="alert">${t('site.add_db_stopped')}</p>`}
    ${data?.running && options.length === 0 && html`<p class="callout">${t('site.add_db_none')}</p>`}
    ${options.length > 0 && html`<div class="form-field">
      <label id="add-db-label">${t('site.add_db_label')}</label>
      <${Select} label=${t('site.add_db_label')} value=${pick} options=${options} onChange=${setPick} />
    </div>`}
    <div class="dialog-actions">
      <button class="btn" onClick=${onClose}>${t('common.cancel')}</button>
      <button class="btn primary" disabled=${!pick || busy} onClick=${add}>${t('site.add_db_confirm')}</button>
    </div>
  </${Dialog}>`;
}

/** Button and dialog for adding a database by hand. */
function AddButton({ siteName, onAdded, primary = false }) {
  const [open, setOpen] = useState(false);
  return html`<button class="btn ${primary ? 'primary' : ''} sm" onClick=${() => setOpen(true)}>
      <${Icon} name="plus" size=${13} />${t('site.add_db')}</button>
    ${open && html`<${AddDatabase} siteName=${siteName} onClose=${() => setOpen(false)} onAdded=${(name) => { setOpen(false); onAdded(name); }} />`}`;
}

function Tables({ info }) {
  const [filter, setFilter] = useState('');
  const tables = info?.tables ?? [];
  const q = filter.trim().toLowerCase();
  const shown = tables.filter((x) => !q || x.name.toLowerCase().includes(q));
  return html`<section class="card clip">
    <div class="card-head"><h2>${t('site.tables')}</h2>${info?.tables && html`<span class="sub">${tables.length}</span>`}<span class="grow"></span>
      ${tables.length > 8 && html`<label class="field sm" style="width: 180px"><${Icon} name="search" size=${13} />
        <input placeholder=${t('site.filter_tables')} aria-label=${t('site.filter_tables')} value=${filter} onInput=${(e) => setFilter(e.target.value)} /></label>`}</div>
    ${!info ? html`<div class="card-body"><${Loading} height=${120} /></div>`
      : info.error ? html`<div class="card-body"><span class="badge danger">${info.error}</span></div>`
      : tables.length === 0 ? html`<${Empty} icon="database" title=${databaseDesc(info)} />`
      : html`<div class="scroll-x"><table class="table dense">
        <thead><tr><th>${t('site.table')}</th><th class="num">${t('site.rows')}</th><th class="num">${t('site.size')}</th></tr></thead>
        <tbody>${shown.map((x) => html`<tr key=${x.name}><td class="mono">${x.name}</td><td class="num mono t2">${x.rows.toLocaleString(LANG)}</td>
          <td class="num mono muted">${size(x.bytes)}</td></tr>`)}
          ${shown.length === 0 && html`<tr><td colspan="3" class="muted">${t('site.no_table_match')}</td></tr>`}</tbody>
      </table></div>`}
  </section>`;
}

/** The database tab: switch between the site's databases, import, export, tables, connect from another app. */
export function Database({ d, current, onPick, onChanged, startJob }) {
  const dbs = d.site_databases;
  const found = dbs.find((x) => x.database.toLowerCase() === (current ?? '').toLowerCase()) ?? dbs[0];
  const [version, setVersion] = useState(0);
  const info = useSiteDatabase(d.site.name, found?.database, version);
  const refresh = () => setVersion((v) => v + 1);

  if (!found) {
    // A connection to another server says where it goes; otherwise there is nothing yet.
    const other = d.databases.find((x) => x.database && !x.local);
    return html`<section class="card"><${Empty} icon="database" title=${other ? other.database : t('site.db_none')}>
      <p>${other ? t('site.db_elsewhere', { connection: other.connection, host: other.host }) : t('site.db_none_hint')}</p>
      <${AddButton} siteName=${d.site.name} onAdded=${(name) => { onChanged(); onPick(name); }} />
    </${Empty}></section>`;
  }

  const port = String(info?.port ?? 3306);
  const url = connectionUrl(found.user, port, found.database);
  const base = `/sites/${enc(d.site.name)}/database`;
  const fromEnv = d.databases.some((x) => x.local && x.database?.toLowerCase() === found.database.toLowerCase());
  const detach = async () => {
    const ok = await confirmAction({
      title: t('site.db_detach_title', { db: found.database }),
      body: t('site.db_detach_body'),
      confirmLabel: t('site.db_detach'),
    });
    if (!ok) return;
    const next = dbs.find((x) => x !== found);
    act(api('DELETE', `/sites/${enc(d.site.name)}/databases/${enc(found.database)}`), () => { onChanged(); if (next) onPick(next.database); });
  };
  const soon = t('site.db_soon');
  const menu = [
    { label: t('site.copy_connection'), icon: 'link', onClick: () => copy(url) },
    { label: t('site.db_terminal'), icon: 'square-terminal', disabled: !info?.exists, onClick: () => act(api('POST', `/mariadb/databases/${enc(found.database)}/terminal`)) },
    { label: t('site.db_empty'), icon: 'eraser', disabled: true, title: soon },
    { label: t('site.db_detach'), icon: 'x', hint: t('site.db_detach_hint'), divider: true, onClick: detach },
    { label: t('services.db_drop_menu'), icon: 'trash-2', danger: true, disabled: !info?.exists,
      onClick: () => dropDatabase(found.database, [d.site.name], () => { onChanged(); refresh(); }) },
  ];
  const rows = [
    ['Host', '127.0.0.1'],
    [t('site.port'), port],
    [t('site.user'), found.user],
    [t('site.password'), null],
    ['Database', found.database],
    [t('site.terminal'), `mysql ${found.database}`],
  ];

  return html`
    <div class="row-flex">
      <span class="small muted" id="site-db-label">${t('site.database')}</span>
      ${dbs.length > MAX_SEGMENTS
        ? html`<${Select} label=${t('site.databases_of_site')} value=${found.database} onChange=${onPick}
            options=${dbs.map((x) => ({ value: x.database, label: x.database }))} />`
        : html`<${Tabs} role="radiogroup" label=${t('site.databases_of_site')} value=${found.database} onChange=${onPick}
            tabs=${dbs.map((x) => ({ id: x.database, label: html`<span class="mono">${x.database}</span>` }))} />`}
      <span class="grow"></span>
      <${AddButton} siteName=${d.site.name} onAdded=${(name) => { onChanged(); onPick(name); }} />
    </div>
    <section class="card"><div class="setrow">
      <span class="row-icon"><${Icon} name="database" /></span>
      <span class="row-text"><b class="mono">${found.database}</b><span>${databaseDesc(info)} · ${fromEnv ? t('site.db_from_env') : t('site.db_by_hand')}</span></span>
      <button class="btn sm" disabled=${!info?.mariadb_running} onClick=${() => importInto(base, found.database, startJob, refresh)}>
        <${Icon} name="upload" size=${13} />${t('site.import')}</button>
      <button class="btn sm" disabled=${!info?.exists || info.tables.length === 0} title=${info?.exists && info.tables.length === 0 ? t('site.export_nothing') : undefined}
        onClick=${() => exportFrom(base, found.database, startJob)}><${Icon} name="download" size=${13} />${t('site.export')}</button>
      <${MenuButton} label=${t('site.db_more', { db: found.database })} items=${menu} />
    </div></section>
    <div class="cols">
      <${Tables} info=${info} />
      <section class="card">
        <div class="card-head"><h2>${t('site.connection')}</h2></div>
        <div class="kv">
          ${rows.map(([k, v]) => html`<span class="k">${k}</span>
            ${v === null
              ? html`<span class="v muted">${found.has_password ? t('site.password_set') : t('site.password_empty')}</span><span class="a"></span>`
              : html`<span class="v">${v}</span><span class="a"><${CopyButton} text=${v} label=${k} /></span>`}`)}
        </div>
        <div class="card-body" style="border-top: 1px solid var(--line)">
          <p class="muted small" style="margin: 0 0 10px">${t('site.connect_hint')}</p>
          <button class="btn sm" onClick=${() => copy(url)}><${Icon} name="link" size=${13} />${t('site.copy_connection')}</button>
        </div>
      </section>
    </div>`;
}
