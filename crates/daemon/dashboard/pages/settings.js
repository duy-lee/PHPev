// Settings: one section per entry of its side menu (#/settings/<section>). Every change is
// checked by the daemon; ports and some TLD changes restart PHPev.
import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api, useApi } from '../api.js';
import { act, showToast } from '../ui/feedback.js';
import { confirmAction } from '../ui/dialog.js';
import { Icon } from '../icons.js';
import { SetRow, Switch, Select, Tabs, Loading } from '../ui/controls.js';
import { applyTheme } from '../ui/theme.js';

// The sections, in the order of the side menu, with their icons.
const SECTIONS = [
  ['general', 'sliders-horizontal'],
  ['sites', 'globe'],
  ['ports', 'network'],
  ['https', 'shield-check'],
  ['windows', 'app-window'],
  ['storage', 'hard-drive'],
  ['about', 'info'],
];
const CHECK_ICON = { ok: 'circle-check', warn: 'triangle-alert', fail: 'circle-x' };

const size = (bytes) =>
  bytes >= 1073741824 ? `${(bytes / 1073741824).toFixed(1)} GB` : `${(bytes / 1048576).toFixed(1)} MB`;

function General({ s, reload }) {
  const [{ data: editors }, reloadEditors] = useApi('/editors');
  const save = (change) => act(api('PUT', '/settings/general', change), reload);
  // Shown at once; the topbar button may also have changed it since the page loaded.
  const [shown, setShown] = useState(s.theme);
  useEffect(() => setShown(s.theme), [s.theme]);
  const theme = (value) => {
    setShown(value);
    applyTheme(value);
    save({ theme: value });
  };
  // Every string is loaded once at start, so the page reloads in the new language.
  const language = (value) => api('PUT', '/settings/general', { language: value }).then(() => location.reload(), (e) => showToast(t('common.error', { message: e.message }), true));
  return html`<section class="card">
    <${SetRow} icon="power" title=${t('settings.autostart')} desc=${t('settings.autostart_hint')}>
      <${Switch} label=${t('settings.autostart')} checked=${s.autostart} onChange=${(on) => save({ autostart: on })} />
    </${SetRow}>
    <${SetRow} icon="panel-left" title=${t('settings.tray')} desc=${t('settings.tray_hint')}>
      <${Switch} label=${t('settings.tray')} checked=${s.tray} onChange=${(on) => save({ tray: on })} />
    </${SetRow}>
    <${SetRow} icon="refresh-cw" title=${t('settings.updates')} desc=${t('settings.updates_hint')}>
      <${Switch} label=${t('settings.updates')} checked=${s.check_updates} onChange=${(on) => save({ check_updates: on })} />
    </${SetRow}>
    <${SetRow} icon="sun-moon" title=${t('settings.theme')} desc=${t('settings.theme_hint')}>
      <${Tabs} role="radiogroup" label=${t('settings.theme')} value=${shown} onChange=${theme}
        tabs=${[{ id: 'system', label: t('settings.theme_system') }, { id: 'light', label: t('settings.theme_light') }, { id: 'dark', label: t('settings.theme_dark') }]} />
    </${SetRow}>
    <${SetRow} icon="languages" title=${t('settings.language')} desc=${t('settings.language_hint')}>
      <${Select} label=${t('settings.language')} value=${s.language} onChange=${language}
        options=${[{ value: 'system', label: t('settings.theme_system') }, { value: 'en', label: 'English' }, { value: 'vi', label: 'Tiếng Việt' }]} />
    </${SetRow}>
    <${SetRow} icon="code" title=${t('settings.editor')}
      desc=${editors?.editors.length ? t('settings.editor_hint') : t('site.no_editor', { list: editors?.supported.join(', ') ?? '' })}>
      ${editors?.editors.length > 0 && html`<${Select} label=${t('settings.editor')} value=${editors.chosen ?? ''}
        options=${[
          ...(editors.chosen ? [] : [{ value: '', label: t('settings.editor_choose'), disabled: true }]),
          ...editors.editors.map((e) => ({ value: e.id, label: e.name })),
        ]} onChange=${(id) => act(api('PUT', '/settings/general', { editor: id }), () => { reload(); reloadEditors(); })} />`}
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
  return html`
    <section class="card">
      <${SetRow} icon="globe" title=${t('settings.tld')} desc=${t('settings.tld_hint', { tld: s.tld })}>
        <label class="field" style="width: 140px"><span class="suffix">.</span>
          <input aria-label=${t('settings.tld')} value=${tld} spellcheck="false" onInput=${(e) => setTld(e.target.value)}
            onKeyDown=${(e) => e.key === 'Enter' && applyTld()} /></label>
        <button class="btn sm" disabled=${tld.trim().replace(/^\./, '').toLowerCase() === s.tld} onClick=${applyTld}>${t('settings.apply')}</button>
      </${SetRow}>
    </section>`;
}

const PORTS = [
  ['http', 'globe', 'settings.port_http'],
  ['https', 'lock', 'settings.port_https'],
  ['mariadb', 'database', 'settings.port_mariadb'],
  ['dashboard', 'layout-dashboard', 'settings.port_dashboard'],
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
      <label class="field port"><input class="mono" type="number" min="1" max="65535" aria-label=${t(label)} value=${ports[key]}
        onInput=${(e) => setPorts({ ...ports, [key]: e.target.value })} /></label>
    </${SetRow}>`)}
    <div class="card-foot"><span class="small muted grow">${t('settings.ports_note')}</span>
      <button class="btn ghost sm" disabled=${!changed} onClick=${() => setPorts(s.ports)}>${t('common.cancel')}</button>
      <button class="btn primary sm" disabled=${!changed} onClick=${save}>${t('settings.save_restart')}</button></div>
  </section>`;
}

function Https({ s }) {
  return html`<section class="card">
    <${SetRow} icon="shield-check" title=${t('settings.ca')} tone=${s.ca_exists && !s.ca_trusted ? 'warn' : ''}
      desc=${!s.ca_exists ? t('settings.ca_none') : s.ca_trusted ? t('settings.ca_trusted') : t('settings.ca_untrusted')}>
      <span class="badge ${s.ca_trusted ? 'ok' : ''}">${s.ca_trusted ? t('settings.trusted') : s.ca_exists ? t('settings.not_trusted') : t('settings.not_created')}</span>
    </${SetRow}>
    <${SetRow} icon="lock" title=${t('settings.secure_sites')}
      desc=${s.secure_sites.length ? s.secure_sites.join(', ') : t('settings.secure_none')} />
    <${SetRow} icon="network" title=${t('settings.port_https')} desc=${`127.0.0.1:${s.ports.https}`} />
    <div class="card-foot"><span class="small muted">${t('settings.https_note')}</span></div>
  </section>`;
}

function Windows({ s, reload }) {
  const problems = s.checks.filter((c) => c.status !== 'ok').length;
  return html`
    <section class="card">
      <${SetRow} icon="shield" title=${t('settings.defender')} desc=${s.defender_excluded ? t('settings.defender_on') : t('settings.defender_hint')}>
        ${s.defender_excluded
          ? html`<span class="badge ok">${t('settings.excluded')}</span>`
          : html`<button class="btn sm" onClick=${() => act(api('POST', '/settings/defender'), reload)}>${t('settings.exclude')}</button>`}
      </${SetRow}>
      <${SetRow} icon="file-text" title=${t('settings.hosts')} desc=${s.tld === 'localhost' ? t('settings.hosts_localhost') : t('settings.hosts_hint', { tld: s.tld })}>
        <button class="btn sm" onClick=${() => act(api('POST', '/settings/hosts'), reload)}>${t('settings.hosts_sync')}</button>
      </${SetRow}>
    </section>
    <section class="card checklist">
      <div class="card-head"><h2>${t('settings.checks')}</h2><span class="grow"></span>
        <span class="badge ${problems ? 'warn' : 'ok'}">${t('home.checks_count', { fine: s.checks.length - problems, total: s.checks.length })}</span></div>
      ${s.checks.filter((c) => c.status !== 'ok').map((c) => html`<${SetRow} icon=${CHECK_ICON[c.status]} title=${c.title} tone="warn"
        desc=${[...c.details, c.hint ? `→ ${c.hint}` : ''].filter(Boolean).join(' · ')} />`)}
      ${s.checks.length > problems && html`<details>
        <summary class="setrow" style="cursor: pointer; list-style: none"><span class="row-icon ok-t"><${Icon} name="circle-check" /></span>
          <span class="row-text"><b>${t('home.checks_passed', { count: s.checks.length - problems })}</b><span>${t('settings.checks_show')}</span></span>
          <${Icon} name="chevron-down" size=${15} /></summary>
        ${s.checks.filter((c) => c.status === 'ok').map((c) => html`<${SetRow} icon="check" title=${c.title} tone="ok" desc=${c.details.join(' · ')} />`)}
      </details>`}
    </section>`;
}

const STORAGE_ICON = { php: 'code-xml', mariadb: 'database', databases: 'database', composer: 'package', downloads: 'download', logs: 'scroll-text' };

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
      <button class="btn ghost sm" onClick=${() => act(api('POST', '/settings/open-home'))}><${Icon} name="folder-open" size=${13} />${t('settings.open_folder')}</button></div>
    ${data.map((x) => html`<${SetRow} icon=${STORAGE_ICON[x.id]} title=${t(`settings.storage_${x.id}`)} desc=${x.path}>
      <span class="storage-size"><span class="mono small t2">${size(x.bytes)}</span><span class="meter"><i style=${`width: ${Math.max(2, (x.bytes / max) * 100)}%`}></i></span></span>
      <span class="storage-act">${x.clearable && html`<button class="btn sm" disabled=${x.bytes === 0} onClick=${() => clean(x.id)}>${t('settings.clean')}</button>`}</span>
    </${SetRow}>`)}
    <div class="card-foot"><span class="small muted">${t('settings.storage_note', { home: s.home })}</span></div>
  </section>`;
}

function About({ s }) {
  return html`<section class="card">
    <${SetRow} icon="info" title=${`PHPev ${s.version}`} desc=${t('settings.about_desc')}>
      <a class="btn sm" href="https://github.com/duy-lee/PHPev/releases" target="_blank" rel="noreferrer">${t('settings.releases')}<${Icon} name="arrow-up-right" size=${13} /></a>
    </${SetRow}>
    <${SetRow} icon="folder" title=${t('settings.home')} desc=${s.home}>
      <button class="btn ghost sm" onClick=${() => act(api('POST', '/settings/open-home'))}>${t('settings.open_folder')}</button>
    </${SetRow}>
    <${SetRow} icon="settings" title=${t('settings.config_file')} desc=${s.config_file} />
    <${SetRow} icon="file-text" title=${t('settings.license')} desc="MIT OR Apache-2.0">
      <a class="btn ghost sm" href="https://github.com/duy-lee/PHPev" target="_blank" rel="noreferrer">GitHub<${Icon} name="arrow-up-right" size=${13} /></a>
    </${SetRow}>
  </section>`;
}

const VIEWS = { general: General, sites: Sites, ports: Ports, https: Https, windows: Windows, storage: Storage, about: About };

/** Breadcrumb after "Settings /": the section shown. */
export function SettingsCrumbs({ route }) {
  const id = SECTIONS.some(([x]) => x === route[1]) ? route[1] : 'general';
  return html`<a href="#/settings" onClick=${(e) => { e.preventDefault(); location.replace('#/settings'); }}>${t('nav.settings')}</a>
    <span class="sep" aria-hidden="true">/</span><span class="cur">${t(`settings.tab_${id}`)}</span>`;
}

export function Settings({ route }) {
  const [{ data, error }, reload] = useApi('/settings');
  const section = SECTIONS.some(([x]) => x === route[1]) ? route[1] : 'general';
  const go = (id) => (e) => {
    e.preventDefault();
    location.replace(`#/settings/${id}`);
  };
  const View = VIEWS[section];
  return html`
    <div class="page-head"><div class="grow"><h1>${t('nav.settings')}</h1><p>${t('settings.subtitle')}</p></div></div>
    <div class="set-layout">
      <nav class="set-nav" aria-label=${t('settings.sections')}>
        ${SECTIONS.map(([id, icon]) => html`<a href=${`#/settings/${id}`} class=${id === section ? 'on' : ''} aria-current=${id === section ? 'page' : undefined}
          onClick=${go(id)}><${Icon} name=${icon} size=${15} />${t(`settings.tab_${id}`)}</a>`)}
      </nav>
      <div class="set-sec">
        <h2>${t(`settings.tab_${section}`)}</h2><p>${t(`settings.desc_${section}`)}</p>
        ${data ? html`<${View} key=${section} s=${data} reload=${reload} />` : html`<${Loading} error=${error} />`}
      </div>
    </div>`;
}
