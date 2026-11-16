// Every action is an API v1 call.
import { useState, useEffect, useCallback } from '/vendor/htm-preact-standalone.mjs';

export async function api(method, path, body) {
  const options = { method, credentials: 'same-origin', headers: {} };
  if (method !== 'GET') {
    // The daemon only accepts JSON for state changes (CSRF protection).
    options.headers['Content-Type'] = 'application/json';
    options.body = JSON.stringify(body ?? {});
  }
  const response = await fetch('/api/v1' + path, options);
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || response.statusText);
  return data;
}

// Rust `Result<T, String>` arrives as {"Ok": T} or {"Err": "message"}.
export const res = (r) => (r && 'Ok' in r ? { ok: true, value: r.Ok } : { ok: false, error: r?.Err ?? '?' });

const REFRESH = 'phpev:refresh';

/** Reloads everything shown (after a change seen by several places: a site linked or unlinked). */
export const refreshAll = () => dispatchEvent(new Event(REFRESH));

// Loads `path`, reloading every `everyMs` while the tab is visible, and on refreshAll().
export function useApi(path, everyMs) {
  const [state, setState] = useState({ data: null, error: null });
  const load = useCallback(() => {
    api('GET', path).then((data) => setState({ data, error: null }), (e) => setState((s) => ({ ...s, error: e.message })));
  }, [path]);
  useEffect(() => {
    load();
    addEventListener(REFRESH, load);
    if (!everyMs) return () => removeEventListener(REFRESH, load);
    // Nothing is fetched while the window is minimised or hidden; on return, at once.
    const timer = setInterval(() => document.visibilityState === 'visible' && load(), everyMs);
    const onShow = () => document.visibilityState === 'visible' && load();
    document.addEventListener('visibilitychange', onShow);
    return () => {
      removeEventListener(REFRESH, load);
      clearInterval(timer);
      document.removeEventListener('visibilitychange', onShow);
    };
  }, [load, everyMs]);
  return [state, load];
}
