# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - Unreleased

First public release.

### Changed

- License: Vitesse is proprietary software by Maxime Nathan Lestage, all
  rights reserved (see `LICENSE`); it is free to use as a dependency,
  commercially too, but not to modify or redistribute. The MIT/Apache-2.0
  dual license is withdrawn.
- Credits: Maxime Nathan Lestage is named as the author in the crate
  metadata, the README and the website's footer, which now links the seven
  projects of the ecosystem.
- Credits: Solidify joins the projects linked by the README, the license and
  the website's footer.

### Added

Closing the gaps with axum and actix-web (see the new guide "Coming from axum
or actix-web"):

- HTTPS (feature `tls`): `app.tls(Tls::from_pem_files(…)?)` serves TLS 1.2
  and 1.3 over TCP with rustls; WebSocket works over it (`wss://`).
- HTTP/2 (feature `http2`), with the h2 crate: negotiated through ALPN over
  TLS, and detected from its preface in clear text (h2c "prior knowledge"),
  on the same port as HTTP/1.1.
- Unix domain sockets: `app.bind_unix`, `app.listen_unix`, `app.run_unix`.
- Engine settings: `app.keep_alive`, `app.header_timeout`,
  `app.shutdown_timeout` and `app.max_connections` (the limits were fixed).
- Trusted proxies: `app.trust_proxy(…)` (`true`, a number of hops, or
  networks such as `"loopback, 10.0.0.0/8"`), read by `req.ip()`,
  `req.protocol()`, `req.secure()`, `req.hostname()` and the new
  `req.host()`.
- `req.route()`: the pattern of the matched route (axum's `MatchedPath`).
- `router.fallback(handler)`: a fallback per router, after its middlewares.
- `req.accepts(…)` and `req.accepts_language(…)`: content negotiation.
- Body limit per route: `middleware::body_limit(n)`, `req.set_body_limit(n)`.
- File uploads (feature `multipart`): `req.multipart()`, streamed with multer.
- Server-Sent Events: `vitesse::sse` (`Sse`, `Event`, `KeepAlive`, and
  `sse::channel` to push events from another task).
- Middlewares: `request_id`, `basic_auth`, `bearer_auth`,
  `concurrency_limit`, `rate_limit` and, with `compress`, `decompress`.
- tower interop (feature `tower`): `middleware::layer(…)` runs any tower
  layer (tower-http, tower-governor…), and `app.into_service()` turns the
  application into a tower `Service`.
- Route patterns: several parameters in a segment (`/flights/:from-:to`,
  `/files/:name.:ext`), constraints (`/users/:id(\d+)`), optional trailing
  parameters (`/archive/:year?/:month?`), and `app.case_sensitive(false)`.
- Named routes and URL generation: `.name("user")`, `req.path_for(…)` and
  `req.url_for(…)` (actix-web's `url_for`).
- Signed cookies and sessions (feature `session`): `app.cookie_secret`,
  `req.sign_cookie`, `req.signed_cookie`, `middleware::session(store)` and
  `req.session()`, with `MemoryStore` and a `SessionStore` trait.
- HTML templates (feature `templates`): `app.views(dir)`, `app.templates(env)`
  and `req.render(name, context)`, with minijinja.
- WebSocket over HTTP/2 (RFC 8441): browsers on HTTPS + HTTP/2 open their
  WebSockets on the same connection; `app.ws` handlers are unchanged.
- HTTP/3 request bodies are streamed to the handler (they were read in full
  before routing), so per-route body limits apply.
- `req.set_method(…)`; a body limited upstream reads as `413`.
- Examples `https` (TLS + HTTP/2) and `live` (SSE + uploads).

- `middleware::compress()` (feature `compress`): responses in Brotli, or gzip
  when the browser does not accept Brotli, for the text-like bodies worth it.

- `vitesse::VERSION`, the version of the crate.

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
  itself and written with [Active](https://maxlestage.github.io/Active/):
  pages rendered on the server with complete SEO tags, interactive parts
  hydrated as WebAssembly islands, real addresses per language (`/en/`,
  `/fr/docs/routing/`…, the old `#/docs/…` links lead to them), the
  documentation rendered on the server with its search, and a static export
  for GitHub Pages. It is a Progressive Web App: installable on a phone's
  home screen, and readable offline.

[0.1.0]: https://crates.io/crates/vitesse/0.1.0
