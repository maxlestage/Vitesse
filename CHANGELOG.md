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
- Router middlewares cover their whole mount prefix, like `router.use`:
  they also run for the `404`, `405` and automatic `OPTIONS` responses
  under it (a CORS middleware on a router answers its preflights).
- Panics in handlers and in middlewares become `500` responses that go
  back through the outer middlewares and `app.on_error`.
- `Request` helpers: params, query strings, headers, cookies, JSON, forms,
  text, bytes and streaming bodies, with a configurable body limit.
- Responses from plain values (`&str`, `String`, `Json`, `Html`,
  `(status, body)`, `Redirect`, `Result`…) and the `res` builder.
- `res::file` / `res::download` with `ETag`, `Last-Modified`, `304 Not
  Modified`, `Range` / `206 Partial Content` and `If-Range`.
- Built-in middleware (`logger`, `cors`, `helmet`, `timeout`) and static
  files (`ServeDir`).
- `vitesse::test`: an in-memory test client.
- Its own HTTP/1.1 engine on tokio: keep-alive, pipelining, chunked bodies,
  `Expect: 100-continue`, size limits, idle timeouts, graceful shutdown on
  Ctrl+C / SIGTERM (`app.run`, `app.listen` and `Server::run`),
  thread-per-core with `SO_REUSEPORT` on Linux.
- WebSocket (`ws` feature, on by default): `app.ws(path, |req, socket| …)`
  on apps and routers, `ws::Upgrade` for subprotocols and size limits,
  `split()` to send and receive from two tasks, automatic pongs.
- HTTP/3 over QUIC (`http3` feature): `app.http3(Http3::from_pem_files(…))`
  serves the same routes over UDP next to HTTP/1.1, with TLS 1.3
  (rustls), graceful shutdown (GOAWAY) and `Alt-Svc` advertising.
- Documentation in English, French and Spanish, a Docker image and
  one-click deployment to Heroku.
- A 100% Rust repository: the website and the benchmark runner
  (`bench/runner`) are Rust.
- The website (https://maxlestage.github.io/Vitesse/) is served by Vitesse
  itself and written with [active](https://github.com/maxlestage/Active):
  pages rendered on the server with complete SEO tags, interactive parts
  hydrated as WebAssembly islands, real addresses per language (`/en/`,
  `/fr/docs/routing/`…, the old `#/docs/…` links lead to them), the
  documentation rendered on the server with its search, and a static export
  for GitHub Pages. It is a Progressive Web App: installable on a phone's
  home screen, and readable offline.

[0.1.0]: https://github.com/maxlestage/Vitesse/releases/tag/v0.1.0
