// PHPev dashboard. Presentation only: every action is an
// API v1 call, so another UI (Slint, TUI) can replace this file without
// touching the daemon. Strings come from /i18n/<lang>.json.
import { html, render, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, setStrings } from './i18n.js';
import { useApi } from './api.js';
import { Icon } from './icons.js';
import { Toasts } from './ui/feedback.js';
import { DialogHost } from './ui/dialog.js';
import { JobDialog } from './ui/job-dialog.js';
import { Palette } from './ui/palette.js';
import { prefs } from './ui/prefs.js';
import { Overview } from './pages/overview.js';
import { Sites, SitesToolbar } from './pages/sites.js';
import { Php } from './pages/php.js';
import { Services } from './pages/services.js';
import { Performance } from './pages/performance.js';
import { Logs } from './pages/logs.js';

// Page registry: a new feature = one entry here + its API endpoints.
const PAGES = [
  { id: 'overview', label: 'nav.overview', icon: 'home', view: Overview },
  { id: 'sites', label: 'nav.sites', icon: 'globe', view: Sites, toolbar: SitesToolbar },
  { id: 'php', label: 'nav.php', icon: 'php', view: Php },
  { id: 'services', label: 'nav.services', icon: 'database', view: Services },
  { id: 'performance', label: 'nav.performance', icon: 'chart', view: Performance },
  { id: 'logs', label: 'nav.logs', icon: 'logs', view: Logs },
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

function Sidebar({ active, mini, onToggle, go }) {
  const [{ data: db }] = useApi('/mariadb', 5000);
  const [{ data: overview }] = useApi('/overview');
  const label = mini ? t('nav.expand') : t('nav.collapse');
  return html`<nav class="side ${mini ? 'mini' : ''}" aria-label=${t('nav.main')}>
    <div class="brand">
      ${mini
        ? html`<img class="mark" src="/mark.svg" alt=${t('app.title')} />`
        : html`<picture><source srcset="/logo-dark.svg" media="(prefers-color-scheme: dark)" /><img src="/logo-light.svg" alt=${t('app.title')} /></picture><span class="grow"></span>`}
      <button class="icon-btn ghost sm" aria-label=${label} title=${label} aria-expanded=${mini ? 'false' : 'true'} onClick=${onToggle}>
        <${Icon} name=${mini ? 'expand-right' : 'collapse-left'} size=${16} /></button>
    </div>
    ${PAGES.map((p) => html`<a href="#/${p.id}" class="nav-i ${p.id === active ? 'on' : ''}" aria-current=${p.id === active ? 'page' : undefined}
      aria-label=${mini ? t(p.label) : undefined} title=${mini ? t(p.label) : undefined} onClick=${(e) => go(e, p.id)}>
      <${Icon} name=${p.icon} /><span class="label">${t(p.label)}</span>
      ${p.id === 'services' && db?.running && html`<span class="dot ok" role="img" aria-label=${t('nav.mariadb_running')}></span>`}
    </a>`)}
    <span class="grow"></span>
    <div class="side-status">
      <span class="dot ok" role="img" aria-label=${t('nav.running')}></span>
      <span class="label grow"><span style="display: block; font-size: 13px; font-weight: 600">${t('nav.running')}</span>
        <span class="small muted">${overview ? t('nav.version', { version: overview.version }) : ' '}</span></span>
    </div>
  </nav>`;
}

function App() {
  // `#/sites/shop/logs` → ['sites', 'shop', 'logs']
  const current = () => location.hash.replace(/^#\//, '').split('/').map(decodeURIComponent);
  const [route, setRoute] = useState(current());
  const [job, setJob] = useState(null);
  const [palette, setPalette] = useState(false);
  const [collapsed, setCollapsed] = useState(prefs.get('navCollapsed') === '1');
  const narrow = useNarrow();
  const stopped = useDaemonWatch();
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
    return () => {
      removeEventListener('hashchange', onHash);
      removeEventListener('keydown', onKey);
    };
  }, []);
  const go = (e, id) => {
    e.preventDefault();
    location.replace(`#/${id}`);
  };
  if (stopped) {
    return html`<div class="stopped"><div class="card pad" style="max-width: 460px">
      <h1 class="display" style="font-size: 22px; margin-bottom: 6px">${t('app.stopped')}</h1><p class="muted" style="margin: 0">${t('app.stopped_hint')}</p></div></div>`;
  }
  const active = PAGES.find((p) => p.id === route[0]) ?? PAGES[0];
  const startJob = (id, after) => setJob({ id, after });
  // A narrow window collapses the menu unless the user opened it explicitly.
  const mini = narrow ? prefs.get('navCollapsed') !== '0' : collapsed;
  const toggle = () => {
    prefs.set('navCollapsed', mini ? '0' : '1');
    setCollapsed(!mini);
  };
  return html`<div class="shell">
    <${Sidebar} active=${active.id} mini=${mini} onToggle=${toggle} go=${go} />
    <main class="content">
      <div class="toolbar">
        ${active.toolbar && html`<${active.toolbar} route=${route} />`}
        <span class="grow"></span>
        <button class="search-btn" aria-label=${t('palette.title')} onClick=${() => setPalette(true)}>
          <${Icon} name="search" size=${16} /><span class="search-label">${t('palette.button')}</span><span class="kbd">Ctrl K</span></button>
      </div>
      <${active.view} startJob=${startJob} route=${route} />
    </main>
    ${job && html`<${JobDialog} id=${job.id} onClose=${() => { job.after?.(); setJob(null); }} />`}
    ${palette && html`<${Palette} pages=${PAGES} onClose=${() => setPalette(false)} onOpen=${(hash) => { setPalette(false); location.replace(hash); }} />`}
    <${DialogHost} />
    <${Toasts} />
  </div>`;
}

const lang = (navigator.language || 'en').slice(0, 2);
fetch(`/i18n/${lang}.json`)
  .then((r) => (r.ok ? r.json() : fetch('/i18n/en.json').then((x) => x.json())))
  .then((s) => {
    setStrings(s);
    render(html`<${App} />`, document.getElementById('app'));
  });
