// Strings come from /i18n/<lang>.json.

export let STRINGS = {};
export const t = (key, vars = {}) => (STRINGS[key] ?? key).replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? '');

export function setStrings(strings) {
  STRINGS = strings;
}
