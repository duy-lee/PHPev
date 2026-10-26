import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';

// Follows a long operation (install) over server-sent events.
export function JobDialog({ id, onClose }) {
  const [job, setJob] = useState(null);
  useEffect(() => {
    const source = new EventSource(`/api/v1/jobs/${id}/stream`);
    source.onmessage = (e) => {
      const info = JSON.parse(e.data);
      setJob(info);
      if (info.status !== 'running') source.close();
    };
    return () => source.close();
  }, [id]);
  if (!job) return html`<div class="overlay"><div class="dialog">${t('common.loading')}</div></div>`;
  const [label, done, total] = job.progress ?? [];
  return html`<div class="overlay"><div class="dialog">
    <h2>${job.title}</h2>
    <p>${job.status === 'running' ? t('job.running') : job.status === 'ok' ? t('job.ok') : t('job.error', { message: job.error })}</p>
    ${job.progress && html`<div class="mono">${label}</div><progress value=${done} max=${total ?? undefined}></progress>`}
    <div class="log mono">${job.lines.join('\n')}</div>
    <p><button onClick=${onClose} disabled=${job.status === 'running'}>${t('common.close')}</button></p>
  </div></div>`;
}
