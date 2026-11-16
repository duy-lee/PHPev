// Strings come from /i18n/<lang>.json.

export let STRINGS = {};
/** The language in use (`en`, `vi`), for dates and numbers too. */
export let LANG = 'en';
/** `{count}` of 1 picks `<key>_one` when the language has it (English: "1 site", "2 sites"). */
export const t = (key, vars = {}) => ((Number(vars.count) === 1 && STRINGS[`${key}_one`]) || STRINGS[key] || key)
  .replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? '');

export function setStrings(strings, lang) {
  STRINGS = strings;
  LANG = lang;
  document.documentElement.lang = lang;
}
