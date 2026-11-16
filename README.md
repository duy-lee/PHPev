<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/phpev-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/phpev-logo-light.svg">
  <img src="assets/phpev-logo-light.svg" alt="PHPev" width="420">
</picture>

**A fast, lightweight, open-source local PHP & Laravel environment for Windows.**

English · [Tiếng Việt](README.vi.md) · [Dashboard demo](https://duy-lee.github.io/PHPev/)

</div>

PHPev installs PHP, MariaDB and Composer, gives every project folder an address such as
`http://shop.localhost`, and lets each project use its own PHP version. Everything is done from the
`phpev` command or the dashboard, without admin rights for daily use.

## Features

- **Sites**: link a folder and it is served at `http://<name>.localhost` (or another TLD such as
  `.test`). Rename a site, pick the folder it serves, turn HTTPS on per site. Apache
  `.htaccess` rewrite rules work.
- **Import a project**: `phpev import` picks the PHP version from `composer.json`, enables missing
  extensions, runs `composer install`, creates `.env` and the database, and links the folder.
- **PHP**: several versions side by side (7.0 and later, official builds), or a PHP folder you
  already have. A site uses `.php-version`, its own pinned version or the default. `php` and
  `composer` in any terminal run the right version for the current folder. Toggle extensions and
  edit `php.ini` from the dashboard.
- **MariaDB**: several versions, each with its own databases; start, stop, restart, switch.
  Create, import, export, empty and drop databases; edit `my.ini`. `mysql` and `mysqldump` in any
  terminal connect to it.
- **Composer**: several versions side by side, one default, another one per site if needed.
- **HTTPS**: a local certificate authority, limited to local names, trusted for your user only.
- **Dashboard** (its own window, light or dark, English or Vietnamese):
  - **Home**: what runs, what needs attention, your sites.
  - **Sites**: a page per site with *Overview* (PHP, Composer, HTTPS, document root, extensions,
    health checks with Fix buttons), *Requests* (the latest requests), *Logs* and *Database*.
  - **Runtimes**: PHP, MariaDB (versions, databases, `my.ini`) and Composer.
  - **Monitor**: live performance (requests, latency, slowest routes, CPU and RAM per process) and
    live logs of PHPev, PHP, MariaDB and Laravel.
  - **Settings**: start at sign-in, tray icon, theme, language, editor, TLD, ports, HTTPS,
    Windows (Defender, hosts file, checks), storage, about.
  - `Ctrl K` finds a site or a page.
- **Tray icon**: open the dashboard or a site, start or stop MariaDB, exit.

## Requirements

- Windows 10 (1809 or later) or Windows 11, x64.
- Free ports 80 and 443 (sites), 3306 (MariaDB) and 7600 (dashboard); all can be changed.
- Internet to download PHP, MariaDB and Composer.

## Installation

Download from [Releases](https://github.com/duy-lee/PHPev/releases):

- **Installer** (`phpev-<version>-setup.exe`): run it, choose the program folder and the PHPev data
  folder (default `%USERPROFILE%\.phpev`). PHPev then appears in the Start menu.
- **Zip** (`phpev-<version>-windows-x64.zip`): extract it, then run `.\phpev.exe setup` in that
  folder. It copies PHPev into `%USERPROFILE%\.phpev` and adds it to your PATH.

Open a new terminal and run `phpev doctor` to check the machine. The files are not code-signed:
if Windows shows *"Windows protected your PC"*, choose **More info → Run anyway**.

## Quick start

```powershell
phpev php install 8.4      # the first version installed becomes the default
cd C:\code\my-app
phpev import               # or: phpev link
phpev dashboard
```

`http://my-app.localhost` now serves the project.

## Commands

| Command | Description |
|---|---|
| `phpev setup [--home DIR]` | Create the PHPev home, put it on PATH, check the machine |
| `phpev doctor [--fix-path] [--defender-exclude]` | Check for problems (PATH, ports, certificates, …) |
| `phpev php list\|install\|add\|remove\|use\|ini\|which\|warm` | PHP versions |
| `phpev db list\|install\|remove\|start\|stop\|use` | MariaDB versions |
| `phpev composer install\|use\|self-update\|list\|remove\|pin\|unpin\|which` | Composer versions |
| `phpev link [NAME] [--php VER]` / `phpev unlink [NAME]` | Serve the current folder / stop serving it |
| `phpev rename <NEW>` | Give a site another name |
| `phpev docroot [FOLDER] [--auto]` | Show or pick the folder a site serves |
| `phpev sites` | List the sites |
| `phpev import [DIR] [--yes] [--migrate]` | Make an existing project run |
| `phpev secure [NAME]` / `phpev unsecure [NAME]` | HTTPS for a site |
| `phpev ca status\|uninstall` | The local certificate authority |
| `phpev tld [TLD]` | Show or change the TLD (default `localhost`) |
| `phpev hosts sync` | Write the sites into the Windows hosts file |
| `phpev start\|stop\|status` | The background service |
| `phpev autostart on\|off` | Start PHPev when you sign in |
| `phpev dashboard [--browser]` | Open the dashboard |
| `phpev tray on\|off` | Show or hide the tray icon |
| `phpev uninstall [--purge]` | Remove PHPev (asks whether to keep your databases) |

`phpev <command> --help` shows every option.

## Good to know

- **Which PHP runs:** a `.php-version` file in the folder or a parent, then the site's pinned
  version, then the default. `phpev php which` tells you why.
- **`.localhost` and PHP:** browsers and curl find `*.localhost` by themselves, but PHP functions
  such as `file_get_contents` do not. Run `phpev hosts sync` once, or use `phpev tld test`.
- **MariaDB** listens on `127.0.0.1:3306`, user `root`, no password: for local development only.
- **Settings** live in `%USERPROFILE%\.phpev\config.toml`; logs in `%USERPROFILE%\.phpev\logs`.
- **Another PHP (XAMPP, Laragon) answers `php -v`:** it is on the system PATH before PHPev.
  `phpev doctor --fix-path` puts PHPev first.
- **A port is busy:** `phpev doctor` shows which program uses it; change the port in Settings.

## Building from source

Needs [Rust](https://rustup.rs/) (stable) and the Visual Studio 2022 Build Tools (*Desktop
development with C++*).

```powershell
cargo build --release
.\target\release\phpev.exe setup
```

Without Visual Studio, use the GNU toolchain with [WinLibs](https://winlibs.com/) on PATH:
`cargo +stable-x86_64-pc-windows-gnu build --release`.

The dashboard demo (sample data, no PHPev needed) runs with `node demo/serve.mjs`.
