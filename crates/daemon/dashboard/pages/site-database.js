// A site's databases: the card on its Overview and the Database tab.
// They are the ones its .env names (on PHPev's MariaDB) plus the ones added by hand.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, LANG } from '../i18n.js';
import { api } from '../api.js';
import { size } from '../format.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction, Dialog } from '../ui/dialog.js';
import { copy, connectionUrl, importInto, exportFrom } from '../ui/db-actions.js';
import { Icon } from '../icons.js';
import { Select, Tabs, SetRow, Empty, Loading } from '../ui/controls.js';

/** Up to this many databases get tabs (and a size on the Overview); more get a dropdown. */
const MAX_TABS = 5;

const enc = encodeURIComponent;

/** What the server says about one database of the site; `null` until loaded (or when `database` is null). */
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
    ${state?.error && html`<span class="pill danger" role="alert">${t('common.error', { message: state.error })}</span>`}
    ${data && !data.running && html`<p class="callout warn" role="alert">${t('site.add_db_stopped')}</p>`}
    ${data?.running && options.length === 0 && html`<p class="callout">${t('site.add_db_none')}</p>`}
    ${options.length > 0 && html`<div class="row-flex" style="gap: 12px">
      <span class="small muted" id="add-db-label">${t('site.add_db_label')}</span>
      <${Select} label=${t('site.add_db_label')} value=${pick} options=${options} onChange=${setPick} />
    </div>`}
    <div class="dialog-actions">
      <button class="btn quiet" onClick=${onClose}>${t('common.cancel')}</button>
      <button class="btn primary" disabled=${!pick || busy} onClick=${add}>${t('site.add_db_confirm')}</button>
    </div>
  </${Dialog}>`;
}

/** Button and dialog for adding a database by hand. */
function AddButton({ siteName, onAdded, primary = false }) {
  const [open, setOpen] = useState(false);
  return html`<button class="btn ${primary ? 'primary' : ''} sm" onClick=${() => setOpen(true)}>
      <${Icon} name="plus" size=${15} />${t('site.add_db')}</button>
    ${open && html`<${AddDatabase} siteName=${siteName} onClose=${() => setOpen(false)} onAdded=${(name) => { setOpen(false); onAdded(name); }} />`}`;
}

function CardRow({ siteName, db, withSize, onOpen }) {
  const info = useSiteDatabase(siteName, withSize ? db.database : null);
  return html`<div class="setrow">
    <span class="row-icon" style="background: var(--accent-soft); color: var(--accent)"><${Icon} name="database" /></span>
    <span class="row-text"><b class="mono">${db.database}</b>${withSize && html`<span>${databaseDesc(info)}</span>`}</span>
    ${db.manual && html`<span class="pill">${t('site.db_manual')}</span>`}
    <button class="btn quiet sm" aria-label=${`${t('site.open_db')} ${db.database}`} onClick=${() => onOpen(db.database)}>
      ${t('site.open_db')}<${Icon} name="chevron-right" size=${15} /></button>
  </div>`;
}

/** Overview: every database of the site, each leading to the Database tab. */
export function DatabaseCard({ d, onOpen, onAdded }) {
  const dbs = d.site_databases;
  const other = dbs.length === 0 && d.databases.find((x) => x.database);
  const status = d.mariadb_version
    ? html`<span class="pill ok"><span class="dot ok"></span>MariaDB ${d.mariadb_version.split('.').slice(0, 2).join('.')}</span>`
    : html`<span class="pill">${t('site.db_stopped')}</span>`;
  return html`<section class="card">
    <div class="card-head"><h2>${t('site.database')}</h2><span class="grow"></span>${dbs.length > 0 && status}</div>
    ${dbs.length === 0 && html`<${SetRow} icon="database" title=${other ? other.database : t('site.db_none')}
      desc=${other ? t('site.db_elsewhere', { connection: other.connection, host: other.host }) : t('site.db_none_hint')} />`}
    ${dbs.map((db) => html`<${CardRow} key=${db.database} siteName=${d.site.name} db=${db} withSize=${dbs.length <= MAX_TABS} onOpen=${onOpen} />`)}
    <div class="card-foot"><${AddButton} siteName=${d.site.name} onAdded=${onAdded} /></div>
  </section>`;
}

/** The database tab: switch between the site's databases, connect to, import, export, tables. */
export function Database({ d, current, onPick, onChanged, startJob }) {
  const dbs = d.site_databases;
  const found = dbs.find((x) => x.database.toLowerCase() === (current ?? '').toLowerCase()) ?? dbs[0];
  const [version, setVersion] = useState(0);
  const info = useSiteDatabase(d.site.name, found.database, version);
  const port = String(info?.port ?? 3306);
  const url = connectionUrl(found.user, port, found.database);
  const base = `/sites/${enc(d.site.name)}/database`;
  const rows = [
    ['Host', '127.0.0.1'],
    [t('site.port'), port],
    [t('site.user'), found.user],
    ['Database', found.database],
    [t('site.connection'), url],
    [t('site.terminal'), `mysql ${found.database}`],
  ];
  const detach = async () => {
    const ok = await confirmAction({
      title: t('site.db_detach_title', { db: found.database }),
      body: t('site.db_detach_body'),
      confirmLabel: t('site.db_detach'),
    });
    if (ok) act(api('DELETE', `/sites/${enc(d.site.name)}/databases/${enc(found.database)}`), onChanged);
  };
  const choices = dbs.map((x) => ({ id: x.database, label: x.database }));

  return html`
    <div class="row-flex" style="gap: 12px">
      ${dbs.length > MAX_TABS
        ? html`<${Select} label=${t('site.databases_of_site')} value=${found.database} onChange=${onPick}
            options=${dbs.map((x) => ({ value: x.database, label: x.database, hint: x.manual ? t('site.db_manual') : undefined }))} />`
        : html`<${Tabs} role="radiogroup" label=${t('site.databases_of_site')} value=${found.database} onChange=${onPick} tabs=${choices} />`}
      <span class="grow"></span>
      <${AddButton} siteName=${d.site.name} onAdded=${(name) => { onChanged(); onPick(name); }} />
    </div>
    <div class="cols">
    <div class="col-side"><section class="card" style="overflow: hidden">
      <div class="card-head" style="padding-bottom: 10px"><h2>${t('site.connect')}</h2>${found.manual && html`<span class="grow"></span><span class="pill">${t('site.db_manual')}</span>`}</div>
      <p class="small muted" style="margin: 0; padding: 0 22px 10px">${t('site.connect_hint')}</p>
      ${rows.map(([k, v]) => html`<div class="copyline"><span class="key">${k}</span><span class="val mono">${v}</span>
        <button class="icon-btn ghost sm" aria-label=${`${t('common.copy')} ${k}`} title=${t('common.copy')} onClick=${() => copy(v)}><${Icon} name="copy" size=${15} /></button></div>`)}
      <div class="copyline"><span class="key">${t('site.password')}</span><span class="val muted">${found.has_password ? t('site.password_set') : t('site.password_empty')}</span></div>
      <div class="card-foot">
        <button class="btn primary sm" disabled=${!info?.mariadb_running} onClick=${() => importInto(base, found.database, startJob, () => setVersion(version + 1))}>
          <${Icon} name="upload" size=${15} />${t('site.import_sql')}</button>
        <button class="btn sm" disabled=${!info?.exists} onClick=${() => exportFrom(base, found.database, startJob)}><${Icon} name="download" size=${15} />${t('site.export')}</button>
        <span class="grow"></span>
        ${found.manual && html`<button class="btn link-danger sm" onClick=${detach}>${t('site.db_detach')}</button>`}
      </div>
    </section></div>
    <div class="col-main"><section class="card" style="overflow: hidden">
      <div class="card-head" style="padding-bottom: 6px"><h2>${t('site.tables')}</h2><span class="small muted">${databaseDesc(info)}</span></div>
      ${!info ? html`<div style="padding: 0 22px 18px"><${Loading} height=${120} /></div>`
        : info.error ? html`<div style="padding: 0 22px 18px"><span class="pill danger">${info.error}</span></div>`
        : info.tables.length === 0 ? html`<${Empty} icon="database" title=${databaseDesc(info)} />`
        : html`<div class="scroll-x"><table class="table">
          <thead><tr><th>${t('site.table')}</th><th class="num">${t('site.rows')}</th><th class="num">${t('site.size')}</th></tr></thead>
          <tbody>${info.tables.map((x) => html`<tr><td class="mono">${x.name}</td><td class="num">${x.rows.toLocaleString(LANG)}</td><td class="num">${size(x.bytes)}</td></tr>`)}</tbody>
        </table></div>`}
    </section></div>
  </div>`;
}
