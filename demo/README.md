# Dashboard demo

The PHPev dashboard with sample data, for GitHub Pages. It is the real dashboard
(`crates/daemon/dashboard`), copied by `build.mjs`, with `fake-api.js` answering in
the browser what the PHPev daemon answers on a real machine. Nothing in here is
built into PHPev.

- `build.mjs` copies the dashboard to an output folder and makes its links relative
  (the site lives under `/PHPev/`). It fails when the dashboard changes in a way it
  does not expect, instead of publishing a broken demo.
- `fake-api.js` stands in for `/api/v1` (sample answers, live charts and logs, short
  fake installs); changes answer "nothing was changed".
- `fixtures.js` holds the sample sites, versions, databases and logs.
- `serve.mjs` builds the demo and serves it on this machine.

Try it locally (only Node is needed; modules do not load from `file://`):

```
node demo/serve.mjs
```

It builds the demo into `target/demo-site` and serves it at
http://127.0.0.1:8000/PHPev/, the same path as on GitHub Pages. The `Pages` workflow builds and publishes it on
every push to `main` that touches the dashboard or this folder.
