<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/phpev-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/phpev-logo-light.svg">
  <img src="assets/phpev-logo-light.svg" alt="PHPev" width="420">
</picture>

**A fast, lightweight, open-source local PHP & Laravel environment for Windows.**

Install PHP, Composer and MariaDB in seconds, switch versions per project, and get a
`http://<project>.localhost` domain (with optional HTTPS) for every folder, all from a
small set of native executables with no admin rights needed for day-to-day use.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
![Platform: Windows 10/11 x64](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-0078D6.svg)
![Written in Rust](https://img.shields.io/badge/written%20in-Rust-orange.svg)
[![CI](https://github.com/duy-lee/PHPev/actions/workflows/ci.yml/badge.svg)](https://github.com/duy-lee/PHPev/actions/workflows/ci.yml)
![Status: pre-release](https://img.shields.io/badge/status-pre--release-yellow.svg)

English · [Tiếng Việt](README.vi.md)

</div>

---

## Table of contents

- [Why PHPev](#why-phpev)
- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Quick start](#quick-start)
- [Usage](#usage)
  - [Sites and domains](#sites-and-domains)
  - [Importing an existing project](#importing-an-existing-project)
  - [PHP versions](#php-versions)
  - [MariaDB](#mariadb)
  - [Composer](#composer)
  - [HTTPS](#https)
  - [Dashboard and tray icon](#dashboard-and-tray-icon)
  - [Background service](#background-service)
- [Command reference](#command-reference)
- [Configuration](#configuration)
- [How it works](#how-it-works)
- [Performance](#performance)
- [Security](#security)
- [Troubleshooting](#troubleshooting)
- [Uninstalling](#uninstalling)
- [Building from source](#building-from-source)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)
- [Acknowledgements](#acknowledgements)

---

## Why PHPev

Tools like [Laravel Herd](https://herd.laravel.com/windows) made local PHP development on Windows
pleasant, but they are closed source. XAMPP-style bundles ship one fixed PHP version and a full
Apache stack. PHPev aims for the Herd experience with three priorities:

- **Open source.** Dual-licensed MIT / Apache-2.0. Read it, fork it, ship it.
- **Light.** The background service uses about **2 MB of RAM** when idle. There is no Electron,
  no WebView2 and no bundled web server: the HTTP router is part of the service.
- **Fast.** The built-in router serves PHP through a pool of `php-cgi` workers with FastCGI
  keep-alive. In our benchmarks it matches or beats nginx + php-cgi (see [Performance](#performance)).

## Features

- **One command to make a project run.** `phpev import` detects Laravel or plain PHP, picks the
  PHP version from `composer.json`, enables missing extensions, runs `composer install`, creates
  `.env`, creates the database and gives the project a domain.
- **Several PHP versions side by side** (7.4 – 8.5, official builds from windows.php.net), chosen
  per project with `.php-version`, per site, or globally. `php` and `composer` in any terminal
  automatically use the right version for the current folder.
- **MariaDB** (10.6 – 13.0, official builds), one data directory per major version, started on
  demand and restored after a reboot.
- **Composer** installed and kept up to date, run with the project's PHP version.
- **Automatic domains**: `park` a folder and every sub-folder becomes `http://<name>.localhost`;
  `link` a single project. No hosts file edits and no admin rights for `.localhost`.
- **HTTPS per site** with a local certificate authority that is limited to local TLDs.
- **Web dashboard** (in its own app window) with sites, PHP versions and extensions, services,
  **live performance charts** (requests/s, p50/p95/p99 latency, slowest routes, CPU/RAM per process)
  and **live logs** (PHPev, PHP, MariaDB and each Laravel app's `storage/logs`).
- **Tray icon** to open the dashboard and sites, start/stop MariaDB and quit.
- **Safe by default**: every download is verified by SHA-256, admin rights are asked only for
  optional features, and actions with side effects ask before they run.

## Requirements

| | |
|---|---|
| OS | Windows 10 (1809 or later) or Windows 11, x64 |
| Disk | ~90 MB per PHP version, ~60 MB per MariaDB version (plus your databases) |
| Internet | Only to download PHP, MariaDB and Composer the first time |
| Admin rights | Not needed for normal use (see [Security](#security) for the optional exceptions) |
| Free ports | 80 and 443 (sites), 3306 (MariaDB), 7600 (dashboard); all configurable |

Nothing has to be installed beforehand: the executables are built with a static C runtime, and
PHPev installs the Visual C++ runtime that PHP needs if it is missing. Microsoft Edge (preinstalled
on Windows) is used for the dashboard window when available; otherwise your default browser opens.

## Installation

> **Not code-signed yet.** Windows may show *"Windows protected your PC"* for files from the
> release. Right-click the downloaded zip → **Properties** → tick **Unblock** before extracting, or
> choose **More info → Run anyway**. You can check the download against `SHA256SUMS.txt` from the
> same release with `Get-FileHash phpev-<version>-windows-x64.zip`.

1. Download `phpev-<version>-windows-x64.zip` from
   [Releases](https://github.com/duy-lee/PHPev/releases) and extract it anywhere (for example
   your Downloads folder).
2. Open a terminal in that folder and run:

   ```powershell
   .\phpev.exe setup
   ```

   `setup` creates the PHPev home (`%USERPROFILE%\.phpev`), copies the executables into
   `%USERPROFILE%\.phpev\shims`, adds that single folder to your **user** PATH and checks the
   machine. The extracted folder is no longer needed afterwards.
3. **Open a new terminal** so it picks up the new PATH, then check everything:

   ```powershell
   phpev doctor
   ```

> If your Windows user name contains non-ASCII characters (for example `Mê Ga Lô Đôn`), PHP cannot
> load its `php.ini` from that path. `setup` detects this and suggests `C:\PHPev` as the home;
> you can also choose one yourself with `phpev setup --home D:\PHPev`.

## Quick start

```powershell
# Install a PHP version (the first one becomes the default)
phpev php install 8.4

# Serve every project inside C:\code as http://<folder>.localhost
phpev park C:\code

# Or make one existing project run end to end
cd C:\code\my-laravel-app
phpev import

# Open the dashboard
phpev dashboard
```

`http://my-laravel-app.localhost` now serves the app. The background service starts by itself
the first time a command needs it.

## Usage

### Sites and domains

PHPev serves sites through its own router on ports 80/443. A site's document root is its
`public` folder if it has `public/index.php` (Laravel, Symfony, …), otherwise the project folder.

```powershell
phpev park C:\code          # every sub-folder of C:\code becomes <folder>.localhost
phpev unpark C:\code

cd C:\code\shop
phpev link                  # this folder becomes shop.localhost
phpev link store --php 8.3  # ...as store.localhost, pinned to PHP 8.3
phpev unlink

phpev sites                 # list sites with URL, PHP version and folder
```

Site names are derived from folder names: lower case, accents removed and other characters
replaced with `-` (`Dự án Mới` → `du-an-moi`).

**Why `.localhost`?** Browsers, `curl` and PHP's curl extension (Laravel `Http::`, Guzzle) resolve
`*.localhost` to 127.0.0.1 by themselves, so no hosts file entry and no admin rights are needed.
Windows' own resolver does not, so code that uses PHP streams (`file_get_contents`,
Symfony HttpClient's native transport) or PowerShell cannot reach `*.localhost`. In that case,
write the sites into the hosts file once (one UAC prompt):

```powershell
phpev hosts sync
```

You can also use another TLD, such as `.test`. The hosts file is then updated automatically
whenever sites change:

```powershell
phpev tld test       # sites become http://<name>.test
phpev tld localhost  # back to the default (PHPev's hosts entries are removed)
```

### Importing an existing project

```powershell
cd C:\code\my-app
phpev import                 # asks before every step that changes something
phpev import --yes           # accept every step (database migrations excluded)
phpev import --yes --migrate # ...and run `php artisan migrate`
```

`import` runs these steps and is safe to run again:

1. **Detect** Laravel (`artisan` + `laravel/framework`), a Composer project, or a plain PHP folder.
2. **PHP**: use `.php-version` if present, otherwise the highest version that satisfies
   `require.php` (and `config.platform.php`). It offers to install one if none fits.
3. **Extensions**: compare the `ext-*` requirements of `composer.json`/`composer.lock` with
   the PHP build and offer to enable the missing ones. PECL-only extensions (redis, imagick, …)
   are reported.
4. **Composer**: install Composer if needed and run `composer install` with the chosen PHP.
5. **Domain**: link the project as `<folder>.localhost` (or `--name`).
6. **`.env`** (Laravel): create it from `.env.example` with `APP_URL`, the MariaDB connection
   and a fresh `APP_KEY`. An existing `.env` is **never modified**; mismatches are only reported.
7. **Database**: create the SQLite file, or install/start MariaDB and create the database.
8. **Front-end**: remind you to run `npm install && npm run build` if assets are not built.

The chosen PHP version is stored in PHPev's config, so no file is added to your repository.

Optionally, `import` can add a Microsoft Defender exclusion for the project folder, which makes
Composer and Artisan faster. Malware written into that folder would then not be scanned on write,
so it is **off by default**, always asked, and never enabled by `--yes`. Use
`--defender-exclude` to enable it without the question.

### PHP versions

```powershell
phpev php list --available   # installed and downloadable versions
phpev php install 8.3        # latest 8.3.x  (or an exact version: 8.3.36)
phpev php use 8.4            # default version outside projects
phpev php which              # which PHP `php` runs here, and why
phpev php ini 8.4            # path of that version's php.ini
```

`php` and `composer` in any terminal are small shims that pick the version in this order:

1. a `.php-version` file in the current folder or the nearest parent (`8.4`, `8.4.27`, `php-8.4`);
2. the site that contains the current folder (`phpev link --php`, or the version chosen by `import`);
3. the default version.

Child processes inherit the choice, so `php artisan serve`, queue workers and Composer scripts run
on the same version. Set `PHPEV_SHIM_DEBUG=1` to print the decision.

Each version has its own `php.ini` generated by PHPev, with OPcache enabled and a CA bundle
configured so HTTPS requests from PHP work out of the box. Extensions can be toggled from the
dashboard's PHP page or by editing the `php.ini`.

### MariaDB

```powershell
phpev db list --available
phpev db install 11.4   # latest 11.4.x
phpev db start          # remembered: MariaDB starts again with PHPev
phpev db stop
phpev db use 10.11      # switch versions; each major keeps its own databases
```

MariaDB listens on `127.0.0.1:3306` with user `root` and **no password**: it is meant for local
development only and is not reachable from other machines.

### Composer

```powershell
phpev composer install       # download composer.phar if missing
phpev composer self-update   # update to the latest stable release
composer --version           # runs with the PHP version of the current folder
```

### HTTPS

```powershell
cd C:\code\shop
phpev secure          # https://shop.localhost; http:// redirects to it
phpev unsecure
phpev ca status
phpev ca uninstall    # remove the CA from Windows and PHP, delete its key, disable HTTPS everywhere
```

The first `phpev secure` creates a local certificate authority and adds it to the **current
user's** trusted root store (Windows shows one confirmation). The CA carries *name constraints*:
it can only issue certificates for `localhost`, `test`, `local` and your configured TLD, so even
a leaked key could not be used to impersonate a real website. Site certificates are created in
memory on demand. The CA is also added to the CA bundle used by PHP.

### Dashboard and tray icon

```powershell
phpev dashboard             # open in its own app window (Microsoft Edge)
phpev dashboard --browser   # open in a normal browser tab instead
phpev tray off              # hide the tray icon (phpev tray on to show it again)
```

The dashboard runs on `http://127.0.0.1:7600` and has these pages:

| Page | What you can do |
|---|---|
| **Overview** | Service status, uptime and RAM, router/HTTPS, MariaDB, PHP pools, `doctor` warnings |
| **Sites** | Link/unlink, HTTPS on/off, change PHP version, open the site, its folder or a terminal |
| **PHP** | Install versions (with progress), set the default, enable/disable extensions, restart workers |
| **Services** | Start/stop MariaDB, switch or install versions |
| **Performance** | Live requests/s, p50/p95 latency, latency histogram, status codes, slowest routes, CPU/RAM of each process, per site, with or without static files |
| **Logs** | Follow the PHPev, PHP, MariaDB and Laravel (`storage/logs/*.log`) logs live, with a filter |

Performance data is kept in memory only (the last 10 minutes of per-second aggregates and the last
2,000 requests). Query strings are never stored, and IDs in URLs are grouped (`/users/42` →
`/users/{n}`).

The **tray icon** opens the dashboard with a left click. The right-click menu has *Open dashboard*,
*Sites*, *Start/Stop MariaDB*, *Start with Windows* and *Exit PHPev*. The dashboard window closes
by itself when PHPev stops.

### Background service

`phpev-daemon.exe` runs the router, the PHP workers, MariaDB and the dashboard. It starts
automatically when a command needs it, and keeps running when you close the terminal.

```powershell
phpev status         # what is running, with ports, PIDs and RAM
phpev start
phpev stop           # stop everything (MariaDB shuts down cleanly)
phpev autostart on   # start at Windows log in (no admin needed)
phpev autostart off
```

PHP workers start on the first request: 2 workers per PHP version, growing to 8 under load,
shrinking back after 60 seconds idle, and stopping after 10 minutes without traffic. Crashed processes are restarted with backoff. All child
processes are tied to the service, so none are left behind if it is killed, and MariaDB is shut
down cleanly when Windows logs off or shuts down.

## Command reference

| Command | Description |
|---|---|
| `phpev setup [--home DIR]` | Create the PHPev home, install the shims on PATH, check the machine |
| `phpev doctor [--fix-path] [--defender-exclude]` | Check for problems (PATH order, ports, runtime, CA, …) |
| `phpev php list\|install\|use\|ini\|which\|warm` | Manage PHP versions |
| `phpev db list\|install\|start\|stop\|use` | Manage MariaDB |
| `phpev composer install\|self-update` | Manage Composer |
| `phpev start\|stop\|status` | Control the background service |
| `phpev autostart on\|off` | Start the service at log in |
| `phpev dashboard [--browser]` | Open the dashboard |
| `phpev tray on\|off` | Show or hide the tray icon |
| `phpev park [DIR]` / `unpark [DIR]` | Serve every sub-folder of a folder |
| `phpev link [NAME] [--php VER]` / `unlink [NAME]` | Serve one folder |
| `phpev sites` | List sites |
| `phpev import [DIR] [--name N] [--yes] [--migrate] [--defender-exclude]` | Make an existing project run |
| `phpev secure [NAME]` / `unsecure [NAME]` | HTTPS for a site |
| `phpev ca status\|uninstall` | Manage the local certificate authority |
| `phpev tld [TLD]` | Show or change the TLD (default `localhost`) |
| `phpev hosts sync` | Write all sites into the Windows hosts file |
| `phpev uninstall [--purge] [--yes]` | Remove PHPev from this computer |

Run `phpev <command> --help` for every option.

## Configuration

Settings live in `%USERPROFILE%\.phpev\config.toml`. The CLI and the dashboard update it for you,
but it is plain TOML and safe to edit by hand. Site changes apply on the next request; port
changes apply after `phpev stop` and `phpev start`.

```toml
php = "8.4"           # default PHP version
mariadb = "11.4"      # active MariaDB major version
tld = "localhost"
parked = ['C:\code']
# tray = false        # hide the tray icon

[ports]
http = 80
https = 443
mariadb = 3306
dashboard = 7600

[[sites]]
name = "shop"
path = 'C:\code\shop'
php = "8.3"           # optional: pin a PHP version
secure = true         # optional: HTTPS
```

Environment variables:

| Variable | Effect |
|---|---|
| `PHPEV_HOME` | Use another home directory (set by `phpev setup --home`) |
| `PHPEV_SHIM_DEBUG=1` | Make the `php`/`composer` shims print which version they chose and why |
| `PHPEV_NO_METRICS=1` | Disable request metrics in the service (used for benchmarking) |

Files and folders:

```
%USERPROFILE%\.phpev\
├── shims\          the only folder on PATH: php.exe, composer.exe, phpev*.exe
├── bin\            php\<version>\, mariadb\<version>\, composer\, cacert.pem
├── data\mariadb\   databases, one folder per major version
├── certs\          local certificate authority
├── logs\           daemon.log, php-<version>.log, mariadb-<version>.err
├── cache\          downloads in progress
└── config.toml
```

## How it works

```
                 ┌──────────────────────── phpev-daemon.exe (≈2 MB RAM idle) ─────────────────────────┐
 browser ──80/443──▶ HTTP/HTTPS router ── static files                                                 │
                 │        │                                                                            │
                 │        └── FastCGI (keep-alive) ──▶ php-cgi pool per PHP version (2–8 workers)      │
                 │                                                                                     │
 dashboard ─7600─▶ API v1 (JSON + SSE) ── metrics, logs, jobs        MariaDB (supervised)              │
 tray icon ──────▶ hidden window (also handles Windows shutdown)                                       │
                 └──────────────▲──────────────────────────────────────────────────────────────────────┘
                                │ named pipe
 terminal: phpev.exe ───────────┘        php.exe / composer.exe shims ──▶ real php.exe of the right version
```

- **`phpev.exe`**: the command line. It talks to the service over a per-user named pipe.
- **`phpev-daemon.exe`**: the background service, a Rust/tokio process with the router (hyper,
  rustls), process supervision, the dashboard and the tray icon. All children run in a
  kill-on-close Job Object.
- **`php.exe` / `composer.exe`**: tiny shims that choose the PHP version and start the real one.
- **`phpev-elevate.exe`**: a helper that runs the few admin-only operations behind one UAC prompt.

The dashboard is a small Preact app embedded in the service binary. It uses only the documented
API (`/api/v1`), so another front end could replace it.

## Performance

Measured on the development machine (Windows 11, PHP 8.4, 8 PHP workers, 8 concurrent connections):

| Scenario | PHPev | nginx 1.30 + php-cgi |
|---|---|---|
| `ping.php`, sequential, median time to first byte | 1.4 ms | 1.3 ms |
| Laravel welcome page, sequential, median TTFB | 42.0 ms | 43.1 ms |
| `ping.php`, 8 connections | 5,700–6,900 req/s, 0 errors | 4,400–7,600 req/s, 32–57 errors, requests stuck for 60 s |

Other figures: background service idle RAM **~2 MB** (private working set, as shown by Task
Manager); dashboard live charts cost **0.07 %** CPU while open; recording request metrics costs
less than our measurement noise; the `php` shim adds about one process start (~11 ms) per call;
opening the dashboard from the tray takes about **0.6 s**.

These are single-machine numbers. Please open an issue with yours.

## Security

- **Everything listens on loopback only** (`127.0.0.1` / `::1`).
- **Dashboard**: a new random token on every service start, exchanged for an `HttpOnly`,
  `SameSite=Strict` cookie. It checks `Host` and `Origin`, requires JSON for changes (CSRF) and
  sends a strict Content-Security-Policy. There is no web shell: the dashboard can only call the
fixed set of API actions.
- **Downloads** (PHP, MariaDB, Composer) come from the official sources and are verified by
  SHA-256 before they are used.
- **Admin rights** are requested (one UAC prompt each) only for optional actions: writing the hosts
  file, Defender exclusions, putting the shims before another PHP on the system PATH, and installing
  the Visual C++ runtime.
- **The local CA** is limited by name constraints to local TLDs and trusted for the current user only.
- **MariaDB** uses `root` without a password and is only reachable from this machine. Do not
  use PHPev to host anything on a network.

Found a vulnerability? Please report it privately through
[GitHub Security Advisories](https://github.com/duy-lee/PHPev/security/advisories/new) rather than a public issue.

## Troubleshooting

**`php -v` shows another PHP (XAMPP, Laragon, …).**
Windows puts the *system* PATH before the *user* PATH, so a PHP on the system PATH wins.
`phpev doctor` shows which one. Fix it with `phpev doctor --fix-path`, which puts PHPev first
(admin), or remove the old PHP from the system PATH. Then open a new terminal.
Note that `where php` behaves differently in `cmd` (it is `where.exe`) and in PowerShell (where it
is an alias of `Where-Object`; use `where.exe php`).

**Port 80, 443 or 3306 is busy.**
`phpev status` and `phpev doctor` show which program holds the port (often XAMPP's Apache or MySQL,
IIS or Skype). Stop it, or change the port in `[ports]` in `config.toml`.

**`file_get_contents('http://app.localhost')` fails inside PHP.**
Windows does not resolve `*.localhost`; run `phpev hosts sync` once.

**The browser warns about the certificate.**
Run `phpev ca status`. If the CA is not trusted, run `phpev secure` again, then restart the browser.
Firefox uses its own store: enable `security.enterprise_roots.enabled` in `about:config`.

**Composer or Artisan is slow.**
Real-time antivirus scanning is the usual cause. `phpev doctor --defender-exclude` excludes the
PHPev home, and `phpev import --defender-exclude` excludes a project. A
[Dev Drive](https://learn.microsoft.com/windows/dev-drive/) is a safer alternative for your code.

**Something else.**
Look at `%USERPROFILE%\.phpev\logs\daemon.log` or the dashboard's Logs page, then
[open an issue](https://github.com/duy-lee/PHPev/issues) with the output of `phpev doctor` and `phpev status`.

## Uninstalling

```powershell
phpev uninstall           # keeps your databases and config.toml
phpev uninstall --purge   # deletes them too
```

`uninstall` shows what it will do and asks before changing anything. It stops PHPev and MariaDB,
turns off start at log in, removes the PHPev certificate authority and PHPev's entry in your PATH,
and deletes the PHPev home. If PHPev changed admin-level settings (hosts file entries, the system
PATH, Microsoft Defender exclusions for the home or your projects), it removes them after a single
UAC prompt. Your project folders are never touched.

Without `--purge`, the databases (`data\`) and `config.toml` stay in `%USERPROFILE%\.phpev`, so
installing PHPev again picks them up. `--purge` asks you to type `delete` before removing them.

## Building from source

Requirements: [Rust](https://rustup.rs/) (stable) and the Visual Studio 2022 Build Tools with the
*Desktop development with C++* workload (MSVC linker, Windows SDK and `rc.exe`).

```powershell
git clone https://github.com/duy-lee/PHPev.git
cd PHPev
cargo build --release
.\target\release\phpev.exe setup
```

Release builds link the C runtime statically (see `.cargo/config.toml`), so they run on a clean
Windows machine.

Without Visual Studio you can build with the GNU toolchain and
[WinLibs](https://winlibs.com/) MinGW-w64 (MSVCRT) on PATH:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
winget install BrechtSanders.WinLibs.POSIX.MSVCRT
cargo +stable-x86_64-pc-windows-gnu build --release
```

Project layout:

| Crate | Output | Purpose |
|---|---|---|
| `crates/core` | library | Config, downloads, PHP/MariaDB/Composer, sites, CA, API types, shared operations |
| `crates/cli` | `phpev.exe` | Command line |
| `crates/daemon` | `phpev-daemon.exe` | Background service, router, dashboard (`crates/daemon/dashboard`), tray |
| `crates/shim` | `phpev-shim.exe` | Installed as `php.exe` and `composer.exe` |
| `crates/elevate` | `phpev-elevate.exe` | Admin-only operations |

Checks run before every commit:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Roadmap

Phase 1 (everything above) is complete. Planned next:

- [x] Continuous integration and release builds
- [ ] Code-signed releases, `winget` and Scoop packages
- [ ] Xdebug on/off per PHP version
- [ ] Optional nginx or Apache backend per site (for `.htaccess` and nginx rewrite rules)
- [ ] More services: mail catcher, Redis, queue workers, Node.js version management

Ideas and votes are welcome in [issues](https://github.com/duy-lee/PHPev/issues).

## Contributing

Contributions are welcome, from bug reports to code.

1. Open an issue first for larger changes, so we can agree on the approach.
2. Fork the repository and create a branch.
3. Keep the checks in [Building from source](#building-from-source) green, and add tests for new logic.
4. Open a pull request that describes what changed and how you tested it.

PHPev cares about staying small and fast: please mention the RAM, size or latency impact of a
change when it might have one.

## License

PHPev is dual-licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

PHP, MariaDB and Composer are downloaded from their official sources at run time and remain under
their own licenses. The dashboard bundles [Preact](https://preactjs.com/) and
[htm](https://github.com/developit/htm) (MIT) and [uPlot](https://github.com/leeoniya/uPlot) (MIT);
their licenses are in `crates/daemon/dashboard/vendor/`.

## Acknowledgements

- [Laravel Herd](https://herd.laravel.com/) and [Laravel Valet](https://laravel.com/docs/valet), for
  showing how pleasant local PHP development can be.
- [hyper](https://hyper.rs/), [tokio](https://tokio.rs/), [rustls](https://github.com/rustls/rustls)
  and [rcgen](https://github.com/rustls/rcgen), which the service is built on.
- [windows.php.net](https://windows.php.net/), [MariaDB Foundation](https://mariadb.org/) and
  [Composer](https://getcomposer.org/) for their official Windows builds and download APIs.

PHPev is not affiliated with Laravel, PHP, MariaDB or Microsoft.
