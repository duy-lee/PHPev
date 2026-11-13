# Changelog

All notable changes to PHPev are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, minor versions may contain breaking changes.

## [Unreleased]

### Added

- **Databases of each MariaDB version**: **Details** under an installed MariaDB (Services ›
  MariaDB) opens a **Databases** tab with every database, its tables and size, and the sites that
  use it. On the running version you can create a database or drop one (after typing its name;
  the dialog names the sites that would break). Other versions are listed too, read from their
  data folder with estimated sizes; switch to a version to change its databases. The server's own
  databases (`mysql`, `sys`, `test`…) are never shown.
- **Databases by hand on a site**: the Database tab (and the Database card on the site's
  Overview) now lists every database of the site, and **Add database** attaches any database that
  exists on PHPev's MariaDB, for example the per-customer ones a project picks at run time and
  that are in no `.env`. The tab is named after the database instead of `MAIN` / `SUB`
  (above five databases it becomes a dropdown), a connection without a database name no longer
  gets a tab, and connection URL, terminal command, import and export all live on that tab.
  Databases added by hand are kept by PHPev, not in the project, and can be removed from the
  site without touching the database.
- **Edit my.ini in the dashboard**: **Details** under an installed MariaDB (Services › MariaDB)
  opens its *my.ini*, for server settings such as `sql_mode`. MariaDB now reads the `my.ini` of
  its data folder at start. Saving keeps the previous file as `my.ini.bak` and restarts MariaDB,
  and puts the previous file back if MariaDB does not start with the new one.
- **`.htaccess` rewrite rules**: `RewriteEngine`, `RewriteBase`, `RewriteCond` and `RewriteRule`
  in the document root and its sub-folders work as with Apache's mod_rewrite, so plain-PHP
  projects written for Apache keep their URLs, images and styles. With active rules, a missing
  file is a 404 instead of going to `index.php`. Lines PHPev cannot use are skipped and logged.
- **Composer version per site**: `phpev composer pin <version>` (or `phpev link --composer`, or
  the site page of the dashboard) makes a site run its own Composer, e.g. 2.2 on one site and 2.8
  on another, whatever their PHP versions. Picked versions are downloaded once and shared;
  `phpev composer list|remove|unpin|which` manage them. A version too new for the site's PHP is
  refused.
- **Services page per service**: Services now lists each service as a card with its own page.
  MariaDB keeps its controls; the new **Composer** page installs and removes Composer versions,
  makes one the default and shows which sites use each one.
- **Default Composer**: `phpev composer use <version>` (or ★ under Services › Composer) picks the
  Composer that sites without their own version run, like the default PHP.
- **Edit php.ini in the dashboard**: **Details** under a PHP version now has two tabs,
  *Extensions* and *php.ini*. Saving checks the file with that PHP first and refuses one it
  rejects, keeps the previous file as `php.ini.bak`, and restarts the version's workers.

### Changed

- Every Composer is now an installed version: `composer.phar` and the separate 2.2 LTS phar are
  gone. Upgrading moves them into versions, makes the newest the default and pins 2.2 for the
  sites on PHP before 7.2.5 that ran it. A Composer too new for a site's PHP is now refused with a
  message instead of being swapped for 2.2. `phpev composer self-update` installs the newest
  version and makes it the default; `composer self-update` itself is refused.
- The default PHP and MariaDB are marked and picked with a star, and versions are removed with a
  trash button, the same on the PHP, MariaDB and Composer pages.
- The PHP and Composer versions of a site, and the appearance, language and code editor in
  Settings, are picked from a dropdown instead of a row of buttons, which stays tidy with many
  versions.

### Fixed

- **Japanese text garbled in scripts of Shift-JIS projects**: static `.js`, `.css` and `.csv`
  files were always sent as UTF-8, so a Shift-JIS page showed `�Ȃ�` in what its scripts print.
  Like Apache, they now carry no charset and the browser reads them in the encoding of the page.
  `AddDefaultCharset` in `.htaccess` (inherited by sub-folders, `Off` supported) now sets the
  charset of static `.html` and `.txt` files; without it they stay UTF-8.

## [1.0.0-rc.1] - 2026-10-27

The first release candidate of 1.0: a new dashboard. Please try it and report problems before
1.0.0.

### Install

Download `phpev-1.0.0-rc.1-setup.exe` below and run it. It installs over an earlier version;
your PHP versions, databases and settings stay in the PHPev home and are not touched.

Or use the zip: download `phpev-1.0.0-rc.1-windows-x64.zip`, right-click it → **Properties** →
**Unblock**, extract it, run `.\phpev.exe setup` in that folder, then open a new terminal.

The files are not code-signed yet, so Windows may warn about them (**More info → Run anyway**).

### Added

- **New dashboard**, light and dark: a collapsible menu, a site list you can hide, Ctrl K to find
  sites and pages, and in-app dialogs.
- **Home** with what runs, your sites, services, memory and health, and three getting-started
  steps on a fresh install.
- **Site page** with tabs: Overview (PHP version, HTTPS, Composer, extensions, workers, health
  with Fix buttons), Requests, Logs, and Database with import (progress, Cancel) and export.
  **Open in editor** for VS Code, PhpStorm and other editors found on the computer.
- **Settings**: start at sign-in, tray icon, theme, language, update check and code editor; the
  TLD and parked folders; ports (PHPev restarts by itself); HTTPS; Defender, hosts file and
  checks; disk use with clean-up of downloads and logs.
- **Update check**: once a day PHPev asks GitHub for a newer release and shows it on Home and in
  the tray, with "Skip this version". It can be turned off in Settings.
- **Vietnamese** for the dashboard and the tray menu. It follows the Windows language unless you
  pick one in Settings. The `phpev` command stays in English.

### Changed

- The dashboard window runs in its own Edge profile (no extensions, sync or GPU process): about
  120 MB, apart from your own Edge. Links from it open in your default browser.

### Fixed

- **PHP 8.2 to 8.5 can be installed again.** windows.php.net now names rebuilds like `8.4.27-1`
  (often security fixes); PHPev lists them newest first and installs them next to the release
  they replace. If `phpev php install 8.4` said "not available", update to this version.

## [0.1.2] - 2026-10-26

### Install

Download `phpev-0.1.2-setup.exe` below and run it. It installs over an earlier version; your PHP
versions, databases and settings stay in the PHPev home and are not touched.

Or use the zip: download `phpev-0.1.2-windows-x64.zip`, right-click it → **Properties** →
**Unblock**, extract it, run `.\phpev.exe setup` in that folder, then open a new terminal.

The files are not code-signed yet, so Windows may warn about them (**More info → Run anyway**).

### Added

- **`mysql`, `mysqldump`, `mariadb`, `mariadb-dump`** in any terminal: the client of the active
  MariaDB, connected to it as `root` (e.g. `mysql shop < dump.sql` in cmd). Your own `-u`, `-h`
  and `-P` take precedence.

## [0.1.1] - 2026-10-25

### Install

Download `phpev-0.1.1-setup.exe` below and run it. It installs over 0.1.0; your PHP versions,
databases and settings stay in the PHPev home and are not touched.

Or use the zip: download `phpev-0.1.1-windows-x64.zip`, right-click it → **Properties** →
**Unblock**, extract it, run `.\phpev.exe setup` in that folder, then open a new terminal.

The files are not code-signed yet, so Windows may warn about them (**More info → Run anyway**).

### Added

- **PHP 7.0 – 7.3**: `phpev php install 7.1` installs the final release of each end-of-life line
  from the windows.php.net archive (SHA-256 pinned in PHPev, since the archive publishes none),
  with a `php.ini` in the format these versions understand.
- **`phpev php add <folder>`** and **Add a PHP you already have** on the dashboard (with the
  Windows folder picker): copy a 64-bit PHP build you already have into PHPev, with a generated
  `php.ini` (the original is kept as `php.ini.orig`).
- **Composer 2.2 LTS for PHP before 7.2.5**: Composer 2.3+ refuses to run on such versions, so
  PHPev also installs 2.2 LTS when one is present, and the `composer` shim picks it automatically.
- **Removing versions**: `phpev php remove <version>` and `phpev db remove <version>`, and a
  **Remove** button on the dashboard. The default PHP and the active MariaDB cannot be removed;
  removing MariaDB keeps its databases unless you choose to delete them.

## [0.1.0] - 2026-10-24

The first public pre-release. Windows 10 (1809+) and 11, x64.

### Install

Download `phpev-0.1.0-setup.exe` below and run it (for you only, without admin rights, or for all
users). PHPev then appears in the Start menu and in Settings → Apps.

Or use the zip: download `phpev-0.1.0-windows-x64.zip`, right-click it → **Properties** →
**Unblock**, extract it, run `.\phpev.exe setup` in that folder, then open a new terminal and run
`phpev doctor`.

The files are not code-signed yet, so Windows may warn about them (**More info → Run anyway**).

See the [README](https://github.com/duy-lee/PHPev#readme) for the full guide.

### Added

- **PHP**: install 7.4 – 8.5 (official builds from windows.php.net, SHA-256 verified), several
  versions side by side, chosen per folder (`.php-version`), per site or globally through the
  `php` / `composer` shims. Generated `php.ini` with OPcache and a CA bundle.
- **MariaDB** 10.6 – 13.0: install, start/stop, switch versions (one data directory per major),
  restored after a reboot.
- **Composer**: install and self-update, always run with the project's PHP version.
- **Sites**: `park` a folder or `link` a project to get `http://<name>.localhost`; other TLDs with
  automatic hosts file entries; `phpev hosts sync` for apps that call themselves through PHP streams.
- **`phpev import`**: makes an existing Laravel or PHP project run (PHP version from
  `composer.json`, extensions, `composer install`, `.env`, database, domain), asking before each step.
- **HTTPS** per site with a local certificate authority limited by name constraints to local TLDs.
- **Built-in router** (HTTP/HTTPS, static files, FastCGI keep-alive to a scaling `php-cgi` pool),
  as fast as nginx + php-cgi in our benchmarks and without its stuck requests.
- **Background service** with supervision, clean MariaDB shutdown on log off, and start at log in.
- **Dashboard** in its own window: sites, PHP versions and extensions, services, live performance
  charts (requests/s, latency percentiles, slowest routes, CPU/RAM per process) and live logs
  (PHPev, PHP, MariaDB, Laravel `storage/logs`).
- **Tray icon** with the dashboard, sites, MariaDB start/stop, start with Windows and exit.
- **`phpev doctor`** checks PATH order, ports, the Visual C++ runtime, the CA and more, and fixes
  what it can.
- **Installer** (`setup.exe`): install for the current user or for all users, choose the program
  and data folders, optional start at sign-in; Start menu entry and uninstall from Settings → Apps.
- **`phpev uninstall`** removes everything PHPev changed (one UAC prompt for admin-level changes)
  and asks whether to delete the databases too (kept by default).

### Known limitations

- The executables are not code-signed, so SmartScreen may warn about them.
- MariaDB uses `root` without a password; it listens on 127.0.0.1 only and is meant for local
  development.
- Only MariaDB is supported (no MySQL); no Xdebug toggle, Node.js, Redis or mail catcher yet.
- Tested mainly on Windows 11. Please report problems on other setups.

[1.0.0-rc.1]: https://github.com/duy-lee/PHPev/releases/tag/v1.0.0-rc.1
[0.1.2]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.2
[0.1.1]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.1
[0.1.0]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.0
