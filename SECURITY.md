# Security policy

## Supported versions

PHPev is pre-1.0. Security fixes go into the latest release only, so please update before reporting.

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Report them privately through
[GitHub Security Advisories](https://github.com/duy-lee/PHPev/security/advisories/new) instead.

Include what you can of:

- the PHPev version (`phpev --version`) and Windows version;
- what an attacker could do, and under which conditions (local user, website in a browser, …);
- steps or a proof of concept to reproduce it.

You should get a first reply within 7 days. Once a fix is released, the advisory is published with
credit to you, unless you prefer to stay anonymous.

## Scope

PHPev is a development tool for a single Windows user. Reports are especially welcome about:

- the dashboard (`127.0.0.1:7600`): authentication, CSRF, cross-origin access;
- the router: reading files outside a site's folder, request smuggling;
- the admin helper (`phpev-elevate.exe`) and anything else that runs with elevated rights;
- the local certificate authority and its name constraints;
- download verification of PHP, MariaDB and Composer.

Out of scope: MariaDB's `root` account without a password and services listening on loopback, which
are documented design choices for local development.
