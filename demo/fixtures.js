// Sample answers of API v1 for the dashboard demo, shaped after crates/core/src/api.rs.
// Fictional sites and folders; nothing here comes from a real machine.

const VERSION = '1.0.0-rc.1';
const HOME = 'C:\\Users\\dev\\AppData\\Local\\PHPev';
const MARIADB = '11.4.8';
const MARIADB_PORT = 3306;

/** Deterministic pseudo-random numbers, so every visit shows the same demo. */
export function seeded(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

const trend = (seed, busy) => {
  const r = seeded(seed);
  return Array.from({ length: 30 }, (_, i) => (busy ? Math.round(busy * (0.4 + r() * 0.9) * (0.7 + 0.3 * Math.sin(i / 4))) : 0));
};

const fw = (name, version = null) => ({ name, version });

// name, framework, PHP, extra: the same six sites as the design.
const SITE_LIST = [
  { name: 'shop', framework: fw('Laravel', '11.9'), php: '8.3.25', php_pinned: '8.3', secure: true, dbs: ['shop', 'shop_test'], busy: 28, path: 'C:\\Users\\dev\\Code\\shop', docroot: 'public', source: 'pinned' },
  { name: 'api', framework: fw('Laravel', '12.1'), php: '8.4.12', secure: true, dbs: ['api'], busy: 10, path: 'C:\\Users\\dev\\Code\\api', docroot: 'public', source: 'version-file' },
  { name: 'blog', framework: fw('WordPress', '6.8'), php: '8.2.29', php_pinned: '8.2', dbs: ['blog_wp'], busy: 4, path: 'C:\\Users\\dev\\Sites\\blog', docroot: '.', source: 'pinned' },
  { name: 'docs', framework: fw('Static'), php: '8.3.25', secure: true, dbs: [], busy: 0.4, path: 'C:\\Users\\dev\\Code\\docs', docroot: 'dist', source: 'default' },
  { name: 'admin-panel', framework: fw('Symfony', '7.3'), php: '8.3.25', dbs: [], busy: 0, path: 'C:\\Users\\dev\\Code\\admin-panel', docroot: 'public', source: 'default' },
  { name: 'legacy-crm', framework: fw('PHP'), php: '7.4.33', php_pinned: '7.4', composer_pinned: '2.2', dbs: ['crm_legacy'], busy: 0, path: 'D:\\Work\\legacy-crm', docroot: 'web', source: 'pinned' },
];

const siteInfo = (s, i) => {
  const t = trend(i + 3, s.busy);
  return {
    name: s.name,
    url: `${s.secure ? 'https' : 'http'}://${s.name}.test`,
    path: s.path,
    php_pinned: s.php_pinned ?? null,
    php: { Ok: s.php },
    composer_pinned: s.composer_pinned ?? null,
    secure: !!s.secure,
    framework: s.framework,
    requests_10m: t.reduce((a, b) => a + b, 0),
    requests_trend: t,
    databases: s.dbs,
  };
};
export const sites = SITE_LIST.map(siteInfo);

const pools = [
  { version: '8.4.12', workers: [{ pid: 11020, port: 9101, memory_kb: 37200 }, { pid: 11024, port: 9102, memory_kb: 35400 }] },
  { version: '8.3.25', workers: [{ pid: 10232, port: 9001, memory_kb: 34800 }, { pid: 10236, port: 9002, memory_kb: 31700 }, { pid: 10240, port: 9003, memory_kb: 32100 }] },
  { version: '8.2.29', workers: [{ pid: 11544, port: 9201, memory_kb: 22600 }, { pid: 11548, port: 9202, memory_kb: 23100 }] },
];
const service = { version: MARIADB, port: MARIADB_PORT, pid: 8448, memory_kb: 151552, state: 'running' };

const checks = [
  { status: 'warn', title: 'Defender scans the PHPev folder', details: ['Every PHP start is scanned, which slows sites down.'], hint: 'Settings › Windows › Exclude PHPev folder' },
  ...['Port 80 is free for PHPev', 'Port 443 is free for PHPev', 'Port 3306 answers: MariaDB 11.4', 'Certificate authority is trusted', 'Hosts file lists every .test site',
    'PHP builds match their checksums', 'Visual C++ runtime is installed', 'The PHPev folder is writable', 'Dashboard listens on 127.0.0.1 only']
    .map((title) => ({ status: 'ok', title, details: [], hint: null })),
];

const PHP_VERSIONS = [
  { version: '8.4.12', pinned_by: [] },
  { version: '8.3.25', pinned_by: ['shop'] },
  { version: '8.2.29', pinned_by: ['blog'] },
  { version: '7.4.33', pinned_by: ['legacy-crm'] },
];
const DEFAULT_PHP = '8.3.25';

const DATABASES = [
  { name: 'api', tables: 31, bytes: 44_773_376, used_by: ['api'] },
  { name: 'blog_wp', tables: 12, bytes: 6_396_313, used_by: ['blog'] },
  { name: 'crm_legacy', tables: 88, bytes: 220_200_960, used_by: ['legacy-crm'] },
  { name: 'playground', tables: 0, bytes: 0, used_by: [] },
  { name: 'shop', tables: 24, bytes: 19_293_798, used_by: ['shop'] },
  { name: 'shop_test', tables: 24, bytes: 2_411_724, used_by: ['shop'] },
];

const TABLES = ['products', 'orders', 'order_items', 'customers', 'stock_levels_old', 'carts', 'cart_items', 'coupons', 'categories', 'migrations', 'sessions', 'jobs']
  .map((name, i) => ({ name, rows: [12840, 10293, 31877, 4211, 920, 812, 2390, 48, 36, 74, 211, 3][i], bytes: [6_291_456, 3_145_728, 4_718_592, 1_572_864, 262_144, 196_608, 393_216, 16_384, 16_384, 16_384, 65_536, 16_384][i] }));

const EXTENSIONS = [
  ['bcmath', 1], ['curl', 1], ['exif', 0], ['fileinfo', 1], ['gd', 1], ['gmp', 0], ['intl', 1], ['ldap', 0], ['mbstring', 1], ['openssl', 1],
  ['pdo_mysql', 1], ['pdo_sqlite', 0], ['soap', 0], ['sockets', 0], ['sodium', 1], ['xsl', 0], ['zip', 1], ['xdebug', 0],
].map(([name, on]) => ({ name, loaded: !!on, toggleable: true }))
  .concat(['ctype', 'json', 'opcache'].map((name) => ({ name, loaded: true, toggleable: false })));

const LARAVEL_LOG = [
  "[2025-11-14 13:55:02] local.DEBUG: Cache miss: products.featured",
  "[2025-11-14 13:55:47] local.WARNING: Slow query (1.2 s): select * from `orders` where `status` = ? order by `created_at` desc",
  "[2025-11-14 14:12:03] local.ERROR: Stripe\\Exception\\SignatureVerificationException: No signatures found matching the expected signature for payload",
  "[2025-11-14 14:28:40] local.INFO: Order #10293 placed (3 items, € 148.00)",
  "[2025-11-14 14:30:12] local.WARNING: Deprecated: Creation of dynamic property App\\Support\\Cart::$coupon is deprecated in app/Support/Cart.php on line 48",
  "[2025-11-14 14:31:58] local.ERROR: SQLSTATE[42S02]: Base table or view not found: 1146 Table 'shop.stock_levels' doesn't exist (SQL: select * from `stock_levels` where `product_id` = 1293)",
  "#0 vendor/laravel/framework/src/Illuminate/Database/Connection.php(825): Illuminate\\Database\\Connection->runQueryCallback()",
  "#1 vendor/laravel/framework/src/Illuminate/Database/Connection.php(392): Illuminate\\Database\\Connection->run()",
  "#2 app/Http/Controllers/Api/StockController.php(31): Illuminate\\Database\\Query\\Builder->get()",
];

const DAEMON_LOG = [
  '2025-11-14 14:19:58 INFO  phpev 1.0.0-rc.1 started; dashboard on 127.0.0.1:7600',
  '2025-11-14 14:20:00 INFO  mariadb 11.4.8 ready for connections on 127.0.0.1:3306',
  '2025-11-14 14:24:59 INFO  pool php 8.2.29: worker 11544 started (1 of 4)',
  '2025-11-14 14:27:11 INFO  router: certificate for shop.test renewed (valid until 2026-02-12)',
  '2025-11-14 14:30:12 WARN  php 8.3.25: Deprecated: Creation of dynamic property App\\Support\\Cart::$coupon',
  '2025-11-14 14:31:30 ERROR shop GET /api/stock/1293 → 500 (388 ms)',
  '2025-11-14 14:31:33 INFO  pool php 8.3.25: worker 10236 started (2 of 4)',
  '2025-11-14 14:31:40 WARN  shop GET /favicon.ico → 404 (2 ms)',
  '2025-11-14 14:31:58 ERROR shop GET /api/stock/1293 → 500 (420 ms)',
];

const PHP_LOG = [
  "[14-Nov-2025 14:30:12 UTC] PHP Deprecated:  Creation of dynamic property App\\Support\\Cart::$coupon is deprecated in C:\\Users\\dev\\Code\\shop\\app\\Support\\Cart.php on line 48",
  "[14-Nov-2025 14:31:58 UTC] PHP Fatal error:  Uncaught PDOException: SQLSTATE[42S02]: Base table or view not found: 1146 Table 'shop.stock_levels' doesn't exist in C:\\Users\\dev\\Code\\shop\\vendor\\laravel\\framework\\src\\Illuminate\\Database\\Connection.php:416",
];

const MARIADB_LOG = [
  '2025-11-14 14:20:00 0 [Note] InnoDB: Buffer pool(s) load completed',
  '2025-11-14 14:20:00 0 [Note] Server socket created on IP: 127.0.0.1, port: 3306.',
  '2025-11-14 14:20:00 0 [Note] mariadbd: ready for connections.',
];

/** Lines of a log source (`daemon`, `php:<v>`, `mariadb:<m>`, `site:<name>:<file>`). */
export function logLines(source) {
  if (source.startsWith('site:')) return source.startsWith('site:shop:') ? LARAVEL_LOG : [];
  if (source.startsWith('php:')) return PHP_LOG;
  if (source.startsWith('mariadb:')) return MARIADB_LOG;
  return DAEMON_LOG;
}

const LOG_SOURCES = [
  { id: 'daemon', label: 'PHPev daemon', path: `${HOME}\\logs\\daemon.log` },
  ...PHP_VERSIONS.map((p) => ({ id: `php:${p.version}`, label: `PHP ${p.version} errors`, path: `${HOME}\\logs\\php-${p.version}.log` })),
  { id: 'mariadb:11.4', label: 'MariaDB 11.4', path: `${HOME}\\logs\\mariadb-11.4.err` },
  { id: 'site:shop:laravel.log', label: 'shop · laravel.log', path: 'C:\\Users\\dev\\Code\\shop\\storage\\logs\\laravel.log' },
];

const siteDetail = (s) => {
  const info = sites.find((x) => x.name === s.name);
  // A static site starts no PHP worker.
  const pool = s.framework.name === 'Static' ? null : pools.find((p) => p.version === s.php);
  const mine = info.databases;
  return {
    site: info,
    php_source: s.source,
    php_lines: ['8.4', '8.3', '8.2', '7.4'],
    workers: pool?.workers.length ?? 0,
    workers_memory_kb: pool?.workers.reduce((a, w) => a + w.memory_kb, 0) ?? 0,
    ca_trusted: true,
    composer: s.framework.name === 'Static' || s.name === 'blog' ? null : s.composer_pinned ? '2.2.25' : '2.8.11',
    composer_source: s.composer_pinned ? 'site' : 'default',
    composer_error: null,
    composer_versions: [{ version: '2.8.11', min_php: '7.2.5', runs: s.php !== '7.4.33' }, { version: '2.2.25', min_php: '5.3.2', runs: true }],
    databases: mine.slice(0, 1).map((db) => ({ name: 'mysql', connection: 'mysql', host: '127.0.0.1', port: MARIADB_PORT, database: db, username: 'root', has_password: false, local: true })),
    site_databases: mine.map((database) => ({ database, user: 'root', has_password: false })),
    docroot: s.docroot,
    docroot_auto: s.docroot,
    docroot_picked: null,
    mariadb_version: mine.length ? MARIADB : null,
    checks: s.name === 'shop'
      ? [{ id: 'storage-link', status: 'warn', title: 'Storage link missing', hint: 'public/storage does not point to storage/app/public; uploaded files will not show.', fix: null },
        { id: 'env', status: 'ok', title: '.env matches PHPev', hint: null, fix: null }]
      : [{ id: 'env', status: 'ok', title: s.framework.name === 'Static' ? 'Static files only' : '.env matches PHPev', hint: null, fix: null }],
    logs: s.name === 'shop' ? [{ id: 'site:shop:laravel.log', label: 'laravel.log', path: 'storage\\logs\\laravel.log' }] : [],
  };
};

// Request list of a site: built once, its times follow the clock.
const PATHS = {
  shop: [['GET', '/', 200, 38, 12_700], ['GET', '/products', 200, 61, 49_200], ['POST', '/cart/add', 302, 112, null], ['GET', '/api/stock/1293', 500, 420, 2_150],
    ['GET', '/build/assets/app-4f2a.js', 200, 3, 188_400, false], ['GET', '/checkout', 200, 182, 31_700], ['GET', '/favicon.ico', 404, 2, null, false],
    ['GET', '/products/espresso-grinder', 200, 74, 23_100], ['POST', '/login', 302, 141, null], ['GET', '/login', 200, 29, 10_000], ['GET', '/account/orders', 200, 96, 17_700]],
  api: [['GET', '/v1/health', 200, 4, 120], ['POST', '/v1/orders', 201, 88, 940], ['GET', '/v1/orders/10293', 200, 31, 2_300], ['GET', '/v1/products', 200, 54, 18_400]],
  blog: [['GET', '/', 200, 210, 41_000], ['GET', '/wp-admin/', 200, 344, 52_000], ['GET', '/wp-content/themes/twentytwentyfive/style.css', 200, 2, 22_000, false]],
  docs: [['GET', '/', 200, 3, 8_900, false], ['GET', '/getting-started/', 200, 2, 12_400, false]],
};
export function recentRequests(host, withStatic, limit) {
  const name = host.split('.')[0];
  const base = PATHS[name] ?? [];
  const r = seeded(name.length * 7);
  const now = Date.now();
  const out = [];
  for (let i = 0; i < Math.min(limit, base.length ? 60 : 0); i++) {
    const [method, path, status, ms, bytes, php = true] = base[Math.floor(r() * base.length)];
    if (!withStatic && !php) continue;
    out.push({ at_ms: now - i * 9_000 - Math.floor(r() * 4_000), method, path, status, ms: ms * (0.7 + r() * 0.6), bytes, php });
  }
  return out;
}

const SLOWEST = [
  ['shop.test', 'GET', '/api/stock/{n}', 18, 402, 520, 611], ['blog.test', 'GET', '/wp-admin/', 6, 210, 344, 380], ['shop.test', 'GET', '/checkout', 41, 150, 238, 296],
  ['api.test', 'POST', '/v1/orders', 63, 88, 161, 204], ['shop.test', 'GET', '/products', 212, 54, 120, 187],
].map(([host, method, route, count, avg_ms, p95_ms, max_ms]) => ({ host, method, route, count, avg_ms, p95_ms, max_ms }));

/** Request statistics over `window` seconds, moving with the clock. */
export function requestStats(window, host, withStatic) {
  const sec = Math.floor(Date.now() / 1000);
  const busy = host ? (SITE_LIST.find((s) => `${s.name}.test` === host)?.busy ?? 0) / 20 : 2.1;
  const scale = withStatic ? 1.6 : 1;
  const per_second = [], p50 = [], p95 = [];
  for (let i = window - 1; i >= 0; i--) {
    const r = seeded(sec - i);
    const n = busy ? Math.round(busy * scale * (0.5 + r() * 1.5)) : 0;
    per_second.push(n);
    p50.push(n ? 30 + r() * 30 : null);
    p95.push(n ? 120 + r() * 160 : null);
  }
  const total = per_second.reduce((a, b) => a + b, 0);
  const hist = [0.06, 0.1, 0.12, 0.14, 0.2, 0.18, 0.1, 0.06, 0.03, 0.007, 0.003];
  const labels = ['≤1 ms', '≤2 ms', '≤5 ms', '≤10 ms', '≤25 ms', '≤50 ms', '≤100 ms', '≤250 ms', '≤500 ms', '≤1000 ms', '>1000 ms'];
  const s5xx = Math.round(total * 0.002);
  return {
    window_secs: window,
    generated_ms: Date.now(),
    total,
    per_second,
    p50_per_second: p50,
    p95_per_second: p95,
    p50_ms: total ? 41 : null,
    p95_ms: total ? 186 : null,
    p99_ms: total ? 412 : null,
    histogram: labels.map((l, i) => [l, Math.round(total * hist[i])]),
    status: { s2xx: total - s5xx - Math.round(total * 0.03) - Math.round(total * 0.01), s3xx: Math.round(total * 0.03), s4xx: Math.round(total * 0.01), s5xx },
    slowest: total ? SLOWEST.filter((x) => !host || x.host === host) : [],
    hosts: SITE_LIST.filter((s) => s.busy).map((s) => `${s.name}.test`),
  };
}

export function processes() {
  const r = seeded(Math.floor(Date.now() / 1000));
  const p = (name, pid, cpu, kb) => ({ name, pid, cpu_percent: Math.max(0, cpu * (0.6 + r() * 0.8)), memory_kb: kb });
  return [
    p('phpev-daemon.exe', 9120, 1.2, 18_400), p('mariadbd.exe', 8448, 3.8, 151_552),
    ...pools.flatMap((pool) => pool.workers.map((w) => p('php-cgi.exe', w.pid, pool.version === '8.3.25' ? 9 : 2, w.memory_kb))),
  ];
}

const settings = {
  version: VERSION, autostart: true, tray: true, theme: 'system', check_updates: true, language: 'en', tld: 'test',
  ports: { http: 80, https: 443, mariadb: MARIADB_PORT, dashboard: 7600 },
  ca_exists: true, ca_trusted: true, secure_sites: SITE_LIST.filter((s) => s.secure).map((s) => s.name),
  defender_excluded: false, checks, home: HOME, config_file: `${HOME}\\config.toml`,
};

const MY_INI = ['# my.ini of MariaDB 11.4 (sample)', '[mysqld]', 'sql_mode = STRICT_TRANS_TABLES,NO_ENGINE_SUBSTITUTION', 'innodb_buffer_pool_size = 256M', 'max_allowed_packet = 64M', ''].join('\n');
const PHP_INI = ['; php.ini of PHP 8.3.25 (sample)', '[PHP]', 'memory_limit = 512M', 'upload_max_filesize = 64M', 'post_max_size = 64M', 'max_execution_time = 120', 'display_errors = On', ''].join('\n');

/** Answers for GET `path` (without /api/v1 and query), or undefined. */
export function get(path, query) {
  const fixed = {
    '/ui': { theme: 'system', language: 'en' },
    '/overview': {
      version: VERSION,
      status: {
        pid: 9120, uptime_secs: 15_320, memory_kb: 18_400, mariadb: service, php_pools: pools,
        router: { listening: ['127.0.0.1:80', '[::1]:80'], error: null, tld: 'test', port: 80, https_listening: ['127.0.0.1:443', '[::1]:443'], https_error: null, https_port: 443 },
        dashboard: { Ok: 'http://127.0.0.1:7600' },
      },
      checks,
    },
    '/sites': sites.map((s) => (query.get('databases') === '1' ? s : { ...s, databases: [] })),
    '/php': {
      default: DEFAULT_PHP,
      installed: PHP_VERSIONS.map((p) => {
        const pool = pools.find((x) => x.version === p.version);
        return { version: p.version, default: p.version === DEFAULT_PHP, workers: pool?.workers.length ?? 0, workers_memory_kb: pool?.workers.reduce((a, w) => a + w.memory_kb, 0) ?? 0, pinned_by: p.pinned_by };
      }),
    },
    '/php/available': [
      { line: '8.5', version: '8.5.0', installed: false, archived: false }, { line: '8.4', version: '8.4.12', installed: true, archived: false },
      { line: '8.3', version: '8.3.25', installed: true, archived: false }, { line: '8.2', version: '8.2.29', installed: true, archived: false },
      { line: '8.1', version: '8.1.33', installed: false, archived: false }, { line: '8.0', version: '8.0.30', installed: false, archived: true },
      { line: '7.4', version: '7.4.33', installed: true, archived: true }, { line: '7.3', version: '7.3.33', installed: false, archived: true },
    ],
    '/mariadb': {
      installed: ['11.4.8', '10.11.14'],
      releases: [{ version: '11.4.8', installed: '2025-09-02', url: 'https://archive.mariadb.org/', sha256: '', lts: true, eol: '2029-05-29' }],
      active: '11.4', port: MARIADB_PORT, running: service, data_dir: `${HOME}\\data\\mariadb\\11.4`,
    },
    '/mariadb/available': [
      { major: '12.0', lts: false, eol: null, eol_passed: false, installed: false }, { major: '11.8', lts: true, eol: '2028-06-04', eol_passed: false, installed: false },
      { major: '11.4', lts: true, eol: '2029-05-29', eol_passed: false, installed: true }, { major: '10.11', lts: true, eol: '2028-02-16', eol_passed: false, installed: true },
      { major: '10.6', lts: true, eol: '2026-07-06', eol_passed: false, installed: false }, { major: '10.5', lts: true, eol: '2025-06-24', eol_passed: true, installed: false },
    ],
    '/composer': {
      default: '2.8.11',
      installed: [
        { version: '2.8.11', min_php: '7.2.5', default: true, used_by: ['shop', 'api', 'admin-panel'], needed_by: [] },
        { version: '2.2.25', min_php: '5.3.2', default: false, used_by: ['legacy-crm'], needed_by: ['legacy-crm'] },
      ],
    },
    '/composer/available': [{ line: '2.8', version: '2.8.12', min_php: '7.2.5', installed: false }, { line: '2.2', version: '2.2.25', min_php: '5.3.2', installed: true }],
    '/editors': { editors: [{ id: 'code', name: 'Visual Studio Code' }, { id: 'phpstorm', name: 'PhpStorm' }], chosen: 'code', supported: ['VS Code', 'PhpStorm', 'Cursor'] },
    '/logs': LOG_SOURCES,
    '/settings': settings,
    '/settings/storage': [
      { id: 'php', path: `${HOME}\\php`, bytes: 432_013_312, clearable: false }, { id: 'mariadb', path: `${HOME}\\mariadb`, bytes: 398_458_880, clearable: false },
      { id: 'databases', path: `${HOME}\\data\\mariadb`, bytes: 901_775_360, clearable: false }, { id: 'composer', path: `${HOME}\\composer`, bytes: 100_663_296, clearable: false },
      { id: 'downloads', path: `${HOME}\\downloads`, bytes: 220_200_960, clearable: true }, { id: 'logs', path: `${HOME}\\logs`, bytes: 39_845_888, clearable: true },
    ],
    '/update': { current: VERSION, latest: VERSION, available: null, url: null, checked_at: Math.floor(Date.now() / 1000) - 3600 },
  };
  if (path in fixed) return fixed[path];

  if (path === '/metrics/requests') return requestStats(Number(query.get('window') ?? 60), query.get('host') || null, query.get('static') === '1');
  if (path === '/metrics/recent') return recentRequests(query.get('host') ?? '', query.get('static') === '1', Number(query.get('limit') ?? 200));

  let m;
  if ((m = path.match(/^\/sites\/([^/]+)$/))) {
    const s = SITE_LIST.find((x) => x.name === decodeURIComponent(m[1]));
    return s && siteDetail(s);
  }
  if ((m = path.match(/^\/sites\/([^/]+)\/database$/))) {
    const name = query.get('name') ?? '';
    const db = DATABASES.find((d) => d.name === name);
    return {
      database: name, host: '127.0.0.1', port: MARIADB_PORT, user: 'root', mariadb_running: true, mariadb_version: MARIADB, exists: !!db,
      tables: db ? TABLES.slice(0, Math.min(db.tables, TABLES.length)) : [], bytes: db?.bytes ?? 0,
    };
  }
  if (path.match(/^\/sites\/[^/]+\/databases\/available$/)) return { running: true, databases: DATABASES.map(({ name, tables, bytes }) => ({ name, tables, bytes })) };
  if (path.match(/^\/mariadb\/[^/]+\/databases$/)) return { running: path.includes('/11.4/'), databases: path.includes('/11.4/') ? DATABASES : [] };
  if (path.match(/^\/mariadb\/[^/]+\/ini$/)) return { text: MY_INI, stamp: 'demo' };
  if (path.match(/^\/php\/[^/]+\/extensions$/)) return EXTENSIONS;
  if (path.match(/^\/php\/[^/]+\/ini$/)) return { text: PHP_INI.replace('8.3.25', path.split('/')[2]), stamp: 'demo' };
  return undefined;
}
