// PHPev dashboard. Presentation only: every action is an
// API v1 call, so another UI (Slint, TUI) can replace this file without
// touching the daemon. Strings come from /i18n/<lang>.json.
import { html, render, useState, useEffect, useRef } from '/vendor/htm-preact-standalone.mjs';
import { t, setStrings } from './i18n.js';
import { api, useApi } from './api.js';
import { Icon } from './icons.js';
import { Toasts, showToast } from './ui/feedback.js';
import { DialogHost, confirmAction } from './ui/dialog.js';
import { MenuButton } from './ui/controls.js';
import { JobDialog } from './ui/job-dialog.js';
import { Palette } from './ui/palette.js';
import { prefs } from './ui/prefs.js';
import { Home } from './pages/home.js';
import { Sites, SitesCrumbs, LinkHost, siteHref, startLinking } from './pages/sites.js';
import { Php } from './pages/php.js';
import { Services, ServicesCrumbs, SERVICES } from './pages/services.js';
import { Performance } from './pages/performance.js';
import { Logs } from './pages/logs.js';
import { Settings, SettingsCrumbs } from './pages/settings.js';
import { applyTheme } from './ui/theme.js';

// Page registry: a new feature = one entry here + its API endpoints. `group` is the breadcrumb
// before the page's name; `crumbs` replaces the page's name in the breadcrumb.
const PAGES = [
  { id: 'home', label: 'nav.home', icon: 'house', view: Home },
  { id: 'sites', label: 'nav.sites', icon: 'globe', view: Sites, crumbs: SitesCrumbs },
  { id: 'php', label: 'nav.php', icon: 'code-xml', view: Php, group: 'nav.runtimes' },
  { id: 'services', label: 'nav.services', icon: 'server', view: Services, crumbs: ServicesCrumbs, group: 'nav.runtimes' },
  { id: 'performance', label: 'nav.performance', icon: 'activity', view: Performance, group: 'nav.monitor' },
  { id: 'logs', label: 'nav.logs', icon: 'scroll-text', view: Logs, group: 'nav.monitor' },
  { id: 'settings', label: 'nav.settings', icon: 'settings', view: Settings, crumbs: SettingsCrumbs },
];

// The menu: its groups and their entries; each service's page is an entry of its own.
const MENU = [
  { items: [{ id: 'home', label: 'nav.home', icon: 'house' }, { id: 'sites', label: 'nav.sites', icon: 'globe', sites: true }] },
  {
    label: 'nav.runtimes',
    items: [
      { id: 'php', label: 'nav.php', icon: 'code-xml' },
      ...SERVICES.map((s) => ({ id: `services/${s.id}`, label: s.label, icon: s.icon, running: s.id === 'mariadb' })),
    ],
  },
  { label: 'nav.monitor', items: [{ id: 'performance', label: 'nav.performance', icon: 'activity' }, { id: 'logs', label: 'nav.logs', icon: 'scroll-text' }] },
];

// Ctrl K finds every page, each service's page included.
const PALETTE_PAGES = [
  ...PAGES.filter((p) => p.id !== 'services'),
  ...SERVICES.map((s) => ({ id: `services/${s.id}`, label: s.label, icon: s.icon })),
];

// The menu collapses on its own in a narrow window.
const NARROW = '(max-width: 1100px)';

function useNarrow() {
  const query = matchMedia(NARROW);
  const [narrow, setNarrow] = useState(query.matches);
  useEffect(() => {
    const onChange = () => setNarrow(query.matches);
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, []);
  return narrow;
}

/** Light or dark as shown now: the chosen theme, else the Windows one. */
const shownTheme = () =>
  document.documentElement.dataset.theme ?? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');

// The dashboard window lives outside the daemon's process job (so stopping
// PHPev never closes the user's browser), so it closes itself when PHPev stops.
// window.close() is only allowed while the window has a single history entry,
// hence navigation with location.replace() below.
function useDaemonWatch() {
  const [stopped, setStopped] = useState(false);
  useEffect(() => {
    const events = new EventSource('/api/v1/events');
    events.onerror = async () => {
      // EventSource also errors on a hiccup and retries; confirm first. A 401
      // means a new daemon run (new token): this page cannot continue either.
      const alive = await fetch('/i18n/en.json', { cache: 'no-store' }).then((r) => r.ok, () => false);
      if (alive) return;
      events.close();
      window.close();
      setStopped(true); // reached only if the browser refused to close the window
    };
    return () => events.close();
  }, []);
  return stopped;
}

/** "8.3.25" → "8.3" */
const line = (version) => version?.split('.').slice(0, 2).join('.');

function Sidebar({ route, onSearch, go }) {
  const [{ data: db }] = useApi('/mariadb', 5000);
  const [{ data: sites }] = useApi('/sites', 5000);
  const [{ data: overview }] = useApi('/overview');
  const path = route.join('/');
  const isOn = (id) => path === id || path.startsWith(`${id}/`);
  const item = (n) => {
    // Sites is marked only on the list itself; on a site's page the site is.
    const on = isOn(n.id) && !(n.sites && route[1]);
    const running = n.running && !!db?.running;
    return html`<a href=${`#/${n.id}`} class="nav-item ${on ? 'on' : ''} ${running ? 'has-dot' : ''}"
      aria-current=${isOn(n.id) ? 'page' : undefined} title=${t(n.label)} onClick=${(e) => go(e, `#/${n.id}`)}>
      <${Icon} name=${n.icon} /><span class="lbl">${t(n.label)}</span>
      ${n.sites && sites?.length > 0 && html`<span class="end">${sites.length}</span>`}
      ${running && html`<span class="dot ok" role="img" aria-label=${t('nav.mariadb_running')}></span>`}
    </a>`;
  };
  const siteList = sites?.length > 0 && html`<div class="nav-sites">
    ${sites.map((s) => html`<a href=${siteHref(s.name)} key=${s.name}
      class="site-link ${route[0] === 'sites' && route[1] === s.name ? 'on' : ''}" onClick=${(e) => go(e, siteHref(s.name))}>
      <span class="dot ${s.requests_10m ? 'ok' : ''}" title=${s.requests_10m ? t('nav.site_active') : undefined}></span>
      <span class="lbl">${s.name}</span>${s.php?.Ok && html`<span class="v">${line(s.php.Ok)}</span>`}</a>`)}
    <a href="#" class="site-link add" onClick=${(e) => { e.preventDefault(); startLinking(); }}>
      <${Icon} name="plus" size=${14} /><span class="lbl">${t('sites.link_title')}</span></a>
  </div>`;
  return html`<aside class="side" aria-label=${t('nav.main')}>
    <div class="brand"><img src="/mark.svg" alt="" /><b>${t('app.title')}</b>
      ${overview && html`<span class="ver" title=${t('nav.version', { version: overview.version })}>${overview.version}</span>`}</div>
    <button class="search" aria-label=${t('palette.title')} title=${`${t('palette.title')} (Ctrl K)`} onClick=${onSearch}>
      <${Icon} name="search" size=${15} /><span class="lbl">${t('palette.button')}</span><span class="kbd">Ctrl K</span></button>
    <nav class="nav">
      ${MENU.map((g) => html`${g.label && html`<div class="nav-label">${t(g.label)}</div>`}
        ${g.items.map((n) => html`${item(n)}${n.sites && siteList}`)}`)}
    </nav>
    ${item({ id: 'settings', label: 'nav.settings', icon: 'settings' })}
    <div class="side-foot">
      <span class="dot ok" role="img" aria-label=${t('nav.running')}></span>
      <span class="who"><b>${t('nav.running')}</b><span>${db?.running ? t('nav.mariadb_running') : t('nav.mariadb_off')}</span></span>
      <${MenuButton} label=${t('nav.power')} icon="power" items=${powerMenu()} />
    </div>
  </aside>`;
}

/** Asks, then sends `path`; the page closes itself when PHPev goes away (see useDaemonWatch). */
async function powerAction(path, what, danger) {
  const ok = await confirmAction({ title: t(`nav.${what}_title`), body: t(`nav.${what}_body`), confirmLabel: t(`nav.${what}`), danger });
  if (ok) api('POST', path).then((r) => showToast(r.message), (e) => showToast(e.message, true));
}

const powerMenu = () => [
  { label: t('nav.open_logs'), icon: 'scroll-text', onClick: () => location.replace('#/logs') },
  { label: t('nav.restart'), icon: 'rotate-cw', onClick: () => powerAction('/daemon/restart', 'restart', false) },
  { label: t('nav.stop'), icon: 'power', danger: true, divider: true, onClick: () => powerAction('/daemon/stop', 'stop', true) },
];

function Topbar({ page, route, mini, onToggle, theme, onTheme }) {
  const toggle = mini ? t('nav.expand') : t('nav.collapse');
  const other = theme === 'dark' ? t('nav.theme_light') : t('nav.theme_dark');
  return html`<header class="topbar">
    <button class="btn ghost square" aria-label=${toggle} title=${toggle} aria-expanded=${mini ? 'false' : 'true'} onClick=${onToggle}>
      <${Icon} name="panel-left" /></button>
    <span class="top-sep" aria-hidden="true"></span>
    <nav class="crumbs" aria-label=${t('nav.breadcrumb')}>
      ${page.group && html`<span>${t(page.group)}</span><span class="sep" aria-hidden="true">/</span>`}
      ${page.crumbs ? html`<${page.crumbs} route=${route} />` : html`<span class="cur">${t(page.label)}</span>`}
    </nav>
    <span class="grow"></span>
    <button class="btn ghost square" aria-label=${other} title=${other} onClick=${onTheme}>
      <${Icon} name=${theme === 'dark' ? 'sun' : 'moon'} /></button>
  </header>`;
}

function App() {
  // `#/sites/shop/logs` → ['sites', 'shop', 'logs']
  const current = () => location.hash.replace(/^#\//, '').split('/').map(decodeURIComponent);
  const [route, setRoute] = useState(current());
  const [job, setJob] = useState(null);
  const [palette, setPalette] = useState(false);
  const [collapsed, setCollapsed] = useState(prefs.get('navCollapsed') === '1');
  const [theme, setTheme] = useState(shownTheme());
  const narrow = useNarrow();
  const stopped = useDaemonWatch();
  const scroller = useRef(null);
  useEffect(() => {
    const onHash = () => setRoute(current());
    addEventListener('hashchange', onHash);
    const onKey = (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPalette(true);
      }
    };
    addEventListener('keydown', onKey);
    // The theme button follows Windows ("system") and the choice made in Settings.
    const system = matchMedia('(prefers-color-scheme: dark)');
    const follow = () => setTheme(shownTheme());
    system.addEventListener('change', follow);
    const watch = new MutationObserver(follow);
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => {
      removeEventListener('hashchange', onHash);
      removeEventListener('keydown', onKey);
      system.removeEventListener('change', follow);
      watch.disconnect();
    };
  }, []);
  // The main panel scrolls, not the window: another page starts at its top.
  useEffect(() => {
    scroller.current?.scrollTo(0, 0);
  }, [route[0], route[1]]);
  const go = (e, hash) => {
    e.preventDefault();
    location.replace(hash);
  };
  if (stopped) {
    return html`<div class="stopped"><div class="card pad" style="max-width: 460px">
      <h1 style="font-size: 16px; margin-bottom: 6px">${t('app.stopped')}</h1><p class="muted" style="margin: 0">${t('app.stopped_hint')}</p></div></div>`;
  }
  const page = PAGES.find((p) => p.id === route[0]) ?? PAGES[0];
  const startJob = (id, after) => setJob({ id, after });
  // A narrow window collapses the menu unless the user opened it explicitly.
  const mini = narrow ? prefs.get('navCollapsed') !== '0' : collapsed;
  const toggle = () => {
    prefs.set('navCollapsed', mini ? '0' : '1');
    setCollapsed(!mini);
  };
  const flipTheme = () => {
    const next = theme === 'dark' ? 'light' : 'dark';
    applyTheme(next);
    api('PUT', '/settings/general', { theme: next }).catch((err) => showToast(t('common.error', { message: err.message }), true));
  };
  return html`<div class="app ${mini ? 'collapsed' : ''}">
    <${Sidebar} route=${route} onSearch=${() => setPalette(true)} go=${go} />
    <main class="main">
      <${Topbar} page=${page} route=${route} mini=${mini} onToggle=${toggle} theme=${theme} onTheme=${flipTheme} />
      <div class="scroll" ref=${scroller}><div class="page page-${page.id}">
        <${page.view} startJob=${startJob} route=${route} />
      </div></div>
    </main>
    ${job && html`<${JobDialog} id=${job.id} onClose=${() => { job.after?.(); setJob(null); }} />`}
    ${palette && html`<${Palette} pages=${PALETTE_PAGES} onClose=${() => setPalette(false)} onOpen=${(hash) => { setPalette(false); location.replace(hash); }} />`}
    <${LinkHost} />
    <${DialogHost} />
    <${Toasts} />
  </div>`;
}

applyTheme(prefs.get('theme'));

// The window runs in its own Edge profile; links that would open a tab there go to the
// default browser instead, with the user's logins and extensions.
const external = (e) => {
  const a = e.target.closest?.('a[target="_blank"]');
  if (!a || (e.type === 'auxclick' && e.button !== 1)) return;
  e.preventDefault();
  api('POST', '/open-url', { url: a.href }).catch((err) => showToast(t('common.error', { message: err.message }), true));
};
document.addEventListener('click', external);
document.addEventListener('auxclick', external);

// The daemon knows the chosen language (or the Windows one); English if it cannot answer.
const strings = (lang) => fetch(`/i18n/${lang}.json`).then((r) => (r.ok ? r.json() : Promise.reject()));
api('GET', '/ui')
  .then((ui) => {
    applyTheme(ui.theme);
    return ui.language;
  }, () => 'en')
  .then((lang) => strings(lang).then((s) => [s, lang], () => strings('en').then((s) => [s, 'en'])))
  .then(([s, lang]) => {
    setStrings(s, lang);
    render(html`<${App} />`, document.getElementById('app'));
  });
