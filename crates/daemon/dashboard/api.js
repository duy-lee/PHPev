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

// Loads `path`, reloading every `everyMs` while the tab is visible.
export function useApi(path, everyMs) {
  const [state, setState] = useState({ data: null, error: null });
  const load = useCallback(() => {
    api('GET', path).then((data) => setState({ data, error: null }), (e) => setState((s) => ({ ...s, error: e.message })));
  }, [path]);
  useEffect(() => {
    load();
    if (!everyMs) return;
    const timer = setInterval(() => document.visibilityState === 'visible' && load(), everyMs);
    return () => clearInterval(timer);
  }, [load, everyMs]);
  return [state, load];
}
