// Display formatting shared by the pages.

/** File and database sizes: 512 KB, 3.4 MB, 1.2 GB. */
export const size = (bytes) =>
  bytes >= 1073741824 ? `${(bytes / 1073741824).toFixed(1)} GB` : bytes >= 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
export const mb =(kb) => (kb == null ? '—' : `${(kb / 1024).toFixed(1)} MB`);
export const uptime = (s) => (s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m`);
export const fmtMs = (v) => (v == null ? '—' : v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`);
