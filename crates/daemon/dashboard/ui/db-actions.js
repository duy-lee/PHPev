// Import, export and copy for a database; shared by a site's Database tab and Services › MariaDB.
// `base` is the API prefix that owns the database: `/sites/<name>/database` or `/mariadb/databases`.
import { t } from '../i18n.js';
import { api } from '../api.js';
import { showToast } from './feedback.js';
import { confirmAction } from './dialog.js';

export const copy = (text) => navigator.clipboard.writeText(text).then(() => showToast(t('common.copied')), () => showToast(text));

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

/** Asks where to save, then follows the export. */
export function exportFrom(base, database, startJob) {
  api('POST', `${base}/export`, { name: database })
    .then((r) => r.job && startJob(r.job), (e) => showToast(e.message, true));
}
