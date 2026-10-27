// Settings: one section per tab (#/settings/<section>). Every change is
// checked by the daemon; ports and some TLD changes restart PHPev.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { SetRow, Switch, Picker, Tabs, Loading } from '../ui/controls.js';
import { applyTheme } from '../ui/theme.js';

const SECTIONS = ['general', 'sites', 'ports', 'https', 'windows', 'storage', 'about'];
const CHECK_ICON = { ok: 'check', warn: 'warning', fail: 'error' };

const size = (bytes) =>
  bytes >= 1073741824 ? `${(bytes / 1073741824).toFixed(1)} GB` : `${(bytes / 1048576).toFixed(1)} MB`;

function General({ s, reload }) {
  const [{ data: editors }] = useApi('/editors');
  const save = (change) => act(api('PUT', '/settings/general', change), reload);
  const theme = (value) => {
    applyTheme(value);
    save({ theme: value });
  };
  // Every string is loaded once at start, so the page reloads in the new language.
  const language = (value) => api('PUT', '/settings/general', { language: value }).then(() => location.reload(), (e) => showToast(t('common.error', { message: e.message }), true));
  return html`<section class="card">
    <${SetRow} icon="power" title=${t('settings.autostart')} desc=${t('settings.autostart_hint')}>
      <${Switch} label=${t('settings.autostart')} checked=${s.autostart} onChange=${(on) => save({ autostart: on })} />
    </${SetRow}>
    <${SetRow} icon="tray" title=${t('settings.tray')} desc=${t('settings.tray_hint')}>
      <${Switch} label=${t('settings.tray')} checked=${s.tray} onChange=${(on) => save({ tray: on })} />
    </${SetRow}>
    <${SetRow} icon="download" title=${t('settings.updates')} desc=${t('settings.updates_hint')}>
      <${Switch} label=${t('settings.updates')} checked=${s.check_updates} onChange=${(on) => save({ check_updates: on })} />
    </${SetRow}>
    <${SetRow} icon="theme" title=${t('settings.theme')} desc=${t('settings.theme_hint')}>
      <${Picker} label=${t('settings.theme')} value=${s.theme} onChange=${theme}
        options=${[{ value: 'system', label: t('settings.theme_system') }, { value: 'light', label: t('settings.theme_light') }, { value: 'dark', label: t('settings.theme_dark') }]} />
    </${SetRow}>
    <${SetRow} icon="globe" title=${t('settings.language')} desc=${t('settings.language_hint')}>
      <${Picker} label=${t('settings.language')} value=${s.language} onChange=${language}
        options=${[{ value: 'system', label: t('settings.theme_system') }, { value: 'en', label: 'English' }, { value: 'vi', label: 'Tiếng Việt' }]} />
    </${SetRow}>
    <${SetRow} icon="editor" title=${t('settings.editor')}
      desc=${editors?.editors.length ? t('settings.editor_hint') : t('site.no_editor', { list: editors?.supported.join(', ') ?? '' })}>
      ${editors?.editors.length > 0 && html`<${Picker} label=${t('settings.editor')} value=${editors.chosen}
        options=${editors.editors.map((e) => ({ value: e.id, label: e.name }))} onChange=${(id) => save({ editor: id })} />`}
    </${SetRow}>
  </section>`;
}

function Sites({ s, reload }) {
  const [tld, setTld] = useState(s.tld);
  useEffect(() => setTld(s.tld), [s.tld]);
  const applyTld = async () => {
    const wanted = tld.trim().replace(/^\./, '').toLowerCase();
    if (!wanted || wanted === s.tld) return;
    const ok = await confirmAction({
      title: t('settings.tld_confirm', { tld: wanted }),
      body: wanted === 'localhost' ? t('settings.tld_back_hint') : t('settings.tld_hosts_hint'),
      confirmLabel: t('settings.apply'),
    });
    if (ok) act(api('PUT', '/settings/tld', { tld: wanted }), reload);
  };
  const park = async () => {
    try {
      const { path } = await api('POST', '/settings/parked/pick');
      if (path) await act(api('POST', '/settings/parked', { path }), reload);
    } catch (e) {
      showToast(t('common.error', { message: e.message }), true);
    }
  };
  const unpark = async (path) => {
    const ok = await confirmAction({ title: t('settings.unpark_title'), body: t('settings.unpark_body', { path }), confirmLabel: t('settings.unpark') });
    if (ok) act(api('POST', '/settings/parked/remove', { path }), reload);
  };
  return html`
    <section class="card">
      <div class="card-head"><h2>${t('settings.domain')}</h2></div>
      <${SetRow} icon="globe" title=${t('settings.tld')} desc=${t('settings.tld_hint', { tld: s.tld })}>
        <label class="field" style="width: 160px"><span>.</span>
          <input aria-label=${t('settings.tld')} value=${tld} spellcheck="false" onInput=${(e) => setTld(e.target.value)}
            onKeyDown=${(e) => e.key === 'Enter' && applyTld()} /></label>
        <button class="btn sm" disabled=${tld.trim().replace(/^\./, '').toLowerCase() === s.tld} onClick=${applyTld}>${t('settings.apply')}</button>
      </${SetRow}>
    </section>
    <section class="card">
      <div class="card-head"><h2>${t('settings.parked')}</h2><span class="grow"></span>
        <button class="btn sm" onClick=${park}><${Icon} name="plus" size=${15} />${t('settings.park')}</button></div>
      <p class="small muted" style="margin: 0; padding: 0 22px 12px">${t('settings.parked_hint', { tld: s.tld })}</p>
      ${s.parked.length === 0
        ? html`<${SetRow} icon="folder" title=${t('settings.parked_none')} />`
        : s.parked.map((path) => html`<${SetRow} icon="folder" title=${path.split('\\').pop()} desc=${path}>
            <button class="btn quiet sm" onClick=${() => unpark(path)}>${t('settings.unpark')}</button>
          </${SetRow}>`)}
    </section>`;
}

const PORTS = [
  ['http', 'globe', 'settings.port_http'],
  ['https', 'lock', 'settings.port_https'],
  ['mariadb', 'database', 'settings.port_mariadb'],
  ['dashboard', 'chart', 'settings.port_dashboard'],
];

function Ports({ s }) {
  const [ports, setPorts] = useState(s.ports);
  const changed = PORTS.some(([k]) => Number(ports[k]) !== s.ports[k]);
  const save = async () => {
    const ok = await confirmAction({ title: t('settings.ports_confirm'), body: t('settings.ports_confirm_body'), confirmLabel: t('settings.save_restart') });
    if (!ok) return;
    try {
      const body = Object.fromEntries(PORTS.map(([k]) => [k, Number(ports[k]) || 0]));
      const r = await api('PUT', '/settings/ports', body);
      showToast(r.message);
    } catch (e) {
      showToast(t('common.error', { message: e.message }), true);
    }
  };
  return html`<section class="card">
    ${PORTS.map(([key, icon, label]) => html`<${SetRow} icon=${icon} title=${t(label)} desc=${t(`${label}_hint`)}>
      <label class="field" style="width: 110px"><input type="number" min="1" max="65535" aria-label=${t(label)} value=${ports[key]}
        onInput=${(e) => setPorts({ ...ports, [key]: e.target.value })} /></label>
    </${SetRow}>`)}
    <div class="card-foot"><span class="small muted grow">${t('settings.ports_note')}</span>
      <button class="btn" disabled=${!changed} onClick=${() => setPorts(s.ports)}>${t('common.cancel')}</button>
      <button class="btn primary" disabled=${!changed} onClick=${save}>${t('settings.save_restart')}</button></div>
  </section>`;
}

function Https({ s }) {
  return html`<section class="card">
    <${SetRow} icon="shield" title=${t('settings.ca')} tone=${s.ca_exists && !s.ca_trusted ? 'warn' : ''}
      desc=${!s.ca_exists ? t('settings.ca_none') : s.ca_trusted ? t('settings.ca_trusted') : t('settings.ca_untrusted')}>
      <span class="pill ${s.ca_trusted ? 'ok' : ''}">${s.ca_trusted ? t('settings.trusted') : s.ca_exists ? t('settings.not_trusted') : t('settings.not_created')}</span>
    </${SetRow}>
    <${SetRow} icon="lock" title=${t('settings.secure_sites')}
      desc=${s.secure_sites.length ? s.secure_sites.join(', ') : t('settings.secure_none')} />
    <${SetRow} icon="plug" title=${t('settings.port_https')} desc=${`127.0.0.1:${s.ports.https}`} />
    <div class="card-foot"><span class="small muted">${t('settings.https_note')}</span></div>
  </section>`;
}

function Windows({ s, reload }) {
  const problems = s.checks.filter((c) => c.status !== 'ok').length;
  return html`
    <section class="card">
      <${SetRow} icon="shield" title=${t('settings.defender')} desc=${s.defender_excluded ? t('settings.defender_on') : t('settings.defender_hint')}>
        ${s.defender_excluded
          ? html`<span class="pill ok">${t('settings.excluded')}</span>`
          : html`<button class="btn sm" onClick=${() => act(api('POST', '/settings/defender'), reload)}>${t('settings.exclude')}</button>`}
      </${SetRow}>
      <${SetRow} icon="globe" title=${t('settings.hosts')} desc=${s.tld === 'localhost' ? t('settings.hosts_localhost') : t('settings.hosts_hint', { tld: s.tld })}>
        <button class="btn sm" onClick=${() => act(api('POST', '/settings/hosts'), reload)}>${t('settings.hosts_sync')}</button>
      </${SetRow}>
    </section>
    <section class="card checklist">
      <div class="card-head"><h2>${t('settings.checks')}</h2><span class="grow"></span>
        <span class="pill ${problems ? 'warn' : 'ok'}">${t('home.checks_count', { fine: s.checks.length - problems, total: s.checks.length })}</span></div>
      ${s.checks.filter((c) => c.status !== 'ok').map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status]} title=${c.title} tone="warn"
        desc=${[...c.details, c.hint ? `→ ${c.hint}` : ''].filter(Boolean).join(' · ')} />`)}
      ${s.checks.length > problems && html`<details>
        <summary class="setrow" style="cursor: pointer; list-style: none"><span class="row-icon"><${Icon} name="check-circle" /></span>
          <span class="row-text"><b>${t('home.checks_passed', { count: s.checks.length - problems })}</b><span>${t('settings.checks_show')}</span></span>
          <${Icon} name="chevron-down" size=${16} /></summary>
        ${s.checks.filter((c) => c.status === 'ok').map((c) => html`<${SetRow} icon="check" title=${c.title} tone="ok" desc=${c.details.join(' · ')} />`)}
      </details>`}
    </section>`;
}

const STORAGE_ICON = { php: 'php', mariadb: 'database', databases: 'database', composer: 'package', downloads: 'download', logs: 'logs' };

function Storage({ s }) {
  const [{ data, error }, reload] = useApi('/settings/storage');
  const clean = async (id) => {
    const ok = await confirmAction({ title: t(`settings.storage_${id}`), body: t(`settings.clean_${id}`), confirmLabel: t('settings.clean') });
    if (ok) act(api('POST', `/settings/storage/${id}/clean`), reload);
  };
  if (!data) return html`<${Loading} error=${error} height=${320} />`;
  const total = data.reduce((a, x) => a + x.bytes, 0);
  const max = Math.max(1, ...data.map((x) => x.bytes));
  return html`<section class="card">
    <div class="card-head"><h2>${t('settings.storage_total', { size: size(total) })}</h2><span class="grow"></span>
      <button class="btn quiet sm" onClick=${() => act(api('POST', '/settings/open-home'))}><${Icon} name="folder" size=${15} />${t('settings.open_folder')}</button></div>
    ${data.map((x) => html`<${SetRow} icon=${STORAGE_ICON[x.id]} title=${t(`settings.storage_${x.id}`)} desc=${x.path}>
      <span class="storage-size"><span class="small">${size(x.bytes)}</span><span class="meter"><i style=${`width: ${Math.max(2, (x.bytes / max) * 100)}%`}></i></span></span>
      <span class="storage-act">${x.clearable && html`<button class="btn quiet sm" disabled=${x.bytes === 0} onClick=${() => clean(x.id)}>${t('settings.clean')}</button>`}</span>
    </${SetRow}>`)}
    <div class="card-foot"><span class="small muted">${t('settings.storage_note', { home: s.home })}</span></div>
  </section>`;
}

function About({ s }) {
  return html`<section class="card">
    <${SetRow} icon="info" title=${`PHPev ${s.version}`} desc=${t('settings.about_desc')}>
      <a class="btn sm" href="https://github.com/duy-lee/PHPev/releases" target="_blank" rel="noreferrer">${t('settings.releases')}</a>
    </${SetRow}>
    <${SetRow} icon="folder" title=${t('settings.home')} desc=${s.home}>
      <button class="btn quiet sm" onClick=${() => act(api('POST', '/settings/open-home'))}>${t('settings.open_folder')}</button>
    </${SetRow}>
    <${SetRow} icon="settings" title=${t('settings.config_file')} desc=${s.config_file} />
    <${SetRow} icon="blocks" title=${t('settings.license')} desc="MIT OR Apache-2.0">
      <a class="btn quiet sm" href="https://github.com/duy-lee/PHPev" target="_blank" rel="noreferrer">GitHub</a>
    </${SetRow}>
  </section>`;
}

const VIEWS = { general: General, sites: Sites, ports: Ports, https: Https, windows: Windows, storage: Storage, about: About };

export function Settings({ route }) {
  const [{ data, error }, reload] = useApi('/settings');
  const section = SECTIONS.includes(route[1]) ? route[1] : 'general';
  const go = (id) => location.replace(`#/settings/${id}`);
  const View = VIEWS[section];
  return html`
    <div class="page-head"><div class="grow"><h1>${t('nav.settings')}</h1><p>${t('settings.subtitle')}</p></div></div>
    <${Tabs} label=${t('settings.sections')} value=${section} onChange=${go}
      tabs=${SECTIONS.map((id) => ({ id, label: t(`settings.tab_${id}`) }))} />
    ${data ? html`<${View} key=${section} s=${data} reload=${reload} />` : html`<${Loading} error=${error} />`}`;
}
