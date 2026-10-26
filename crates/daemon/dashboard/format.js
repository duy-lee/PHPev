// Display formatting shared by the pages.

export const mb = (kb) => (kb == null ? '—' : `${(kb / 1024).toFixed(1)} MB`);
export const uptime = (s) => (s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m`);
export const fmtMs = (v) => (v == null ? '—' : v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`);
