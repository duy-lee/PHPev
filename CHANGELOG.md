# Changelog

All notable changes to PHPev are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/). Until 1.0, minor versions may contain breaking changes.

## [0.1.0] - 2026-10-24

The first public pre-release. Windows 10 (1809+) and 11, x64.

### Install

1. Download `phpev-0.1.0-windows-x64.zip` below, right-click it → **Properties** → **Unblock**, and
   extract it. (The executables are not code-signed yet, so Windows may warn about them.)
2. In that folder run `.\phpev.exe setup`, then open a new terminal and run `phpev doctor`.

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
- **`phpev uninstall`** removes everything PHPev changed (one UAC prompt for admin-level changes),
  keeping the databases unless `--purge`.

### Known limitations

- The executables are not code-signed, so SmartScreen may warn about them.
- MariaDB uses `root` without a password; it listens on 127.0.0.1 only and is meant for local
  development.
- Only MariaDB is supported (no MySQL); no Xdebug toggle, Node.js, Redis or mail catcher yet.
- Tested mainly on Windows 11. Please report problems on other setups.

[0.1.0]: https://github.com/duy-lee/PHPev/releases/tag/v0.1.0
