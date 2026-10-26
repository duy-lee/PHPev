// PHPev dashboard. Presentation only: every action is an
// API v1 call, so another UI (Slint, TUI) can replace this file without
// touching the daemon. Strings come from /i18n/<lang>.json.
import { html, render, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t, setStrings } from './i18n.js';
import { Toast } from './ui/feedback.js';
import { JobDialog } from './ui/job-dialog.js';
import { Overview } from './pages/overview.js';
import { Sites } from './pages/sites.js';
import { Php } from './pages/php.js';
import { Services } from './pages/services.js';
import { Performance } from './pages/performance.js';
import { Logs } from './pages/logs.js';

// Page registry: a new feature = one entry here + its API endpoints.
const PAGES = [
  { id: 'overview', label: 'nav.overview', view: Overview },
  { id: 'sites', label: 'nav.sites', view: Sites },
  { id: 'php', label: 'nav.php', view: Php },
  { id: 'services', label: 'nav.services', view: Services },
  { id: 'performance', label: 'nav.performance', view: Performance },
  { id: 'logs', label: 'nav.logs', view: Logs },
];

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

function App() {
  const current = () => location.hash.replace(/^#\//, '') || 'overview';
  const [page, setPage] = useState(current());
  const [job, setJob] = useState(null);
  const stopped = useDaemonWatch();
  useEffect(() => {
    const onHash = () => setPage(current());
    addEventListener('hashchange', onHash);
    return () => removeEventListener('hashchange', onHash);
  }, []);
  const go = (e, id) => {
    e.preventDefault();
    location.replace(`#/${id}`);
  };
  if (stopped) {
    return html`<div class="overlay"><div class="dialog"><h1>${t('app.stopped')}</h1><p>${t('app.stopped_hint')}</p></div></div>`;
  }
  const active = PAGES.find((p) => p.id === page) ?? PAGES[0];
  const startJob = (id, after) => setJob({ id, after });
  return html`<div class="layout">
    <nav><div class="brand"><picture><source srcset="/logo-dark.svg" media="(prefers-color-scheme: dark)" /><img src="/logo-light.svg" alt=${t('app.title')} /></picture></div>
      ${PAGES.map((p) => html`<a href="#/${p.id}" onClick=${(e) => go(e, p.id)} class=${p.id === active.id ? 'active' : ''}>${t(p.label)}</a>`)}</nav>
    <main><${active.view} startJob=${startJob} /></main>
    ${job && html`<${JobDialog} id=${job.id} onClose=${() => { job.after?.(); setJob(null); }} />`}
    <${Toast} />
  </div>`;
}

const lang = (navigator.language || 'en').slice(0, 2);
fetch(`/i18n/${lang}.json`)
  .then((r) => (r.ok ? r.json() : fetch('/i18n/en.json').then((x) => x.json())))
  .then((s) => {
    setStrings(s);
    render(html`<${App} />`, document.getElementById('app'));
  });
