import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { Icon } from '../icons.js';
import { Dialog } from './dialog.js';

const size = (n) => (n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.ceil(n / 1024)} KB`);

// Follows a long operation (install) over server-sent events.
export function JobDialog({ id, onClose }) {
  const [job, setJob] = useState(null);
  const [details, setDetails] = useState(false);
  useEffect(() => {
    const source = new EventSource(`/api/v1/jobs/${id}/stream`);
    source.onmessage = (e) => {
      const info = JSON.parse(e.data);
      setJob(info);
      if (info.status !== 'running') source.close();
    };
    return () => source.close();
  }, [id]);
  const running = !job || job.status === 'running';
  const close = running ? null : onClose;
  if (!job) return html`<${Dialog} title=${t('job.running')}><p>${t('common.loading')}</p></${Dialog}>`;
  const [label, done, total] = job.progress ?? [];
  const failed = job.status === 'error';
  const cancelled = job.status === 'cancelled';
  const cancel = () => api('POST', `/jobs/${id}/cancel`).catch(() => {});
  return html`<${Dialog} title=${job.title} onClose=${close} wide>
    <div class="job-state ${failed ? 'err' : cancelled ? '' : running ? 'run' : 'ok'}">
      ${running ? html`<span class="spin"><${Icon} name="rotate-cw" size=${15} /></span>` : html`<${Icon} name=${failed ? 'circle-x' : cancelled ? 'x' : 'circle-check'} size=${15} />`}
      <span class="grow">${running ? t('job.running') : failed ? t('job.error', { message: job.error }) : cancelled ? t('job.cancelled') : t('job.ok')}</span>
    </div>
    ${running && job.progress && html`<div class="job-progress">
      <div class="small muted mono">${label} · ${total ? `${size(done)} / ${size(total)}` : size(done)}</div>
      <div class="prog ${total ? '' : 'busy'}" role="progressbar" aria-label=${label} aria-valuemin="0"
        aria-valuemax=${total || undefined} aria-valuenow=${total ? done : undefined}><i style=${total ? `width: ${(100 * done) / total}%` : ''}></i></div>
    </div>`}
    ${job.lines.length > 0 && html`<div style="margin-top: 14px">
      <button class="btn ghost sm" aria-expanded=${details || failed ? 'true' : 'false'} onClick=${() => setDetails(!details)}>
        <${Icon} name=${details || failed ? 'chevron-down' : 'chevron-right'} size=${14} />${t('job.details')}</button>
      ${(details || failed) && html`<div class="joblog" style="margin-top: 10px">${job.lines.join('\n')}</div>`}
    </div>`}
    <div class="dialog-actions">
      ${running && job.cancellable && html`<button class="btn danger-text" onClick=${cancel}>${t('common.cancel')}</button>`}
      <button class="btn ${running ? '' : 'primary'}" onClick=${onClose} disabled=${running}>${t('common.close')}</button>
    </div>
  </${Dialog}>`;
}
