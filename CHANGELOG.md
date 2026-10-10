# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - Unreleased

First public release.

### Added

- Express.js-style API: `App`, `app.get/post/put/patch/delete/…`, path
  parameters and wildcards, `Router` and `app.mount`, middleware with
  `next.run(req)`, shared state, `app.fallback` and `app.on_error`.
- `Request` helpers: params, query strings, headers, cookies, JSON, forms,
  text, bytes and streaming bodies, with a configurable body limit.
- Responses from plain values (`&str`, `String`, `Json`, `Html`,
  `(status, body)`, `Redirect`, `Result`…) and the `res` builder.
- Built-in middleware (`logger`, `cors`, `helmet`, `timeout`) and static
  files (`ServeDir`).
- `vitesse::test`: an in-memory test client.
- Its own HTTP/1.1 engine on tokio: keep-alive, pipelining, chunked bodies,
  `Expect: 100-continue`, size limits, idle timeouts, graceful shutdown on
  Ctrl+C / SIGTERM, thread-per-core with `SO_REUSEPORT` on Linux.
- Documentation in English, French and Spanish, a Docker image and
  one-click deployment to Heroku.

[0.1.0]: https://github.com/maxlestage/Vitesse/releases/tag/v0.1.0
