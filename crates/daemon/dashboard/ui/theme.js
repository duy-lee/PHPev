// Light, dark or the Windows setting. The choice lives in config.toml
// ([ui] theme); the copy in prefs only avoids a flash before it loads.
import { prefs } from './prefs.js';

export function applyTheme(theme) {
  const root = document.documentElement;
  if (theme === 'light' || theme === 'dark') root.dataset.theme = theme;
  else delete root.dataset.theme;
  prefs.set('theme', theme ?? 'system');
}
