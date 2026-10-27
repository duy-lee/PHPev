// Per-viewer layout choices. Storage may be unavailable (private window,
// blocked site data); the dashboard works the same without it.
export const prefs = {
  get(key) {
    try {
      return localStorage.getItem(`phpev.${key}`);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(`phpev.${key}`, value);
    } catch {
      /* not remembered */
    }
  },
};
