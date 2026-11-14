// Import, export and copy for a database; shared by a site's Database tab and Services › MariaDB.
// `base` is the API prefix that owns the database: `/sites/<name>/database` or `/mariadb/databases`.
import { html } from '/vendor/htm-preact-standalone.mjs';
import { t } from '../i18n.js';
import { api } from '../api.js';
import { act, copy, showToast } from './feedback.js';
import { confirmAction } from './dialog.js';

export { copy };

/** The address other apps (HeidiSQL, TablePlus, DBeaver) connect with. */
export const connectionUrl = (user, port, database) => `mysql://${user}@127.0.0.1:${port}/${database}`;

/** Picks a .sql file, asks how to import it, then follows the import; `done` runs when it ends. */
export async function importInto(base, database, startJob, done) {
  let picked;
  try {
    picked = await api('POST', `${base}/import/pick`);
  } catch (e) {
    return showToast(e.message, true);
  }
  if (!picked.path) return;
  const file = picked.path.split('\\').pop();
  const answer = await confirmAction({
    title: t('site.import_title', { db: database }),
    body: t('site.import_body', { file }),
    confirmLabel: t('site.import'),
    choice: { label: t('site.import_empty_first'), confirmWord: 'delete' },
  });
  if (!answer) return;
  api('POST', `${base}/import`, { name: database, path: picked.path, empty_first: answer.choice })
    .then((r) => startJob(r.job, done), (e) => showToast(e.message, true));
}

/** Drops a database of PHPev's MariaDB once its name is typed; `usedBy` are the sites that name it. */
export async function dropDatabase(name, usedBy, done) {
  const ok = await confirmAction({
    title: t('services.db_drop_title', { name }),
    body: t('services.db_drop_body', { name }),
    details: usedBy.length > 0 && html`<p class="callout warn" role="alert">${t('services.db_drop_used', { sites: usedBy.join(', ') })}</p>`,
    confirmLabel: t('services.db_drop'),
    danger: true,
    confirmWord: name,
  });
  if (ok) act(api('DELETE', `/mariadb/databases/${encodeURIComponent(name)}`), done);
}

/** Asks where to save, then follows the export. */
export function exportFrom(base, database, startJob) {
  api('POST', `${base}/export`, { name: database })
    .then((r) => r.job && startJob(r.job), (e) => showToast(e.message, true));
}
