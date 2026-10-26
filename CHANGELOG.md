# Changelog

All notable changes to PHPev are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, minor versions may contain breaking changes.

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

[0.1.2]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.2
[0.1.1]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.1
[0.1.0]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.0
