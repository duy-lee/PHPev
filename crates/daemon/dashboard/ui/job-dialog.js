import { html, useState, useEffect } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
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
  return html`<${Dialog} title=${job.title} onClose=${close} wide>
    <div class="row-flex" style="margin-bottom: 12px">
      ${running ? html`<span class="dot ok"></span>` : html`<${Icon} name=${failed ? 'error' : 'check-circle'} />`}
      <span class="grow" style=${failed ? 'color: var(--danger)' : ''}>${running ? t('job.running') : failed ? t('job.error', { message: job.error }) : t('job.ok')}</span>
    </div>
    ${running && job.progress && html`<div class="small muted mono" style="margin-bottom: 6px">${label}${total ? ` · ${size(done)} / ${size(total)}` : ''}</div>
      <progress value=${done} max=${total ?? undefined}></progress>`}
    ${job.lines.length > 0 && html`<div style="margin-top: 14px">
      <button class="btn quiet sm" aria-expanded=${details || failed ? 'true' : 'false'} onClick=${() => setDetails(!details)}>
        <${Icon} name=${details || failed ? 'chevron-down' : 'chevron-right'} size=${15} />${t('job.details')}</button>
      ${(details || failed) && html`<div class="joblog" style="margin-top: 10px">${job.lines.join('\n')}</div>`}
    </div>`}
    <div class="dialog-actions">
      <button class="btn ${running ? 'quiet' : 'primary'}" onClick=${onClose} disabled=${running}>${t('common.close')}</button>
    </div>
  </${Dialog}>`;
}
