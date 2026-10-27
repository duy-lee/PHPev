// Strings come from /i18n/<lang>.json.

export let STRINGS = {};
/** The language in use (`en`, `vi`), for dates and numbers too. */
export let LANG = 'en';
export const t = (key, vars = {}) => (STRINGS[key] ?? key).replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? '');

export function setStrings(strings, lang) {
  STRINGS = strings;
  LANG = lang;
  document.documentElement.lang = lang;
}
