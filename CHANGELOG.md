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
- Links: the websites of Vitesse and its sibling projects are now their
  Heroku apps (README, documentation, crate `homepage`, `app.json`, rustdoc
  logo, the demo and the website's footer) instead of GitHub Pages; the
  static export for GitHub Pages is unchanged.
- Website: no longer published on GitHub Pages (the deploy job of the `Site`
  workflow is removed): it is served by Heroku only, and GitHub only hosts
  the code. The READMEs no longer describe a Pages deployment; the `Site`
  workflow still checks the static export.
- Credits: the ecosystem is now five projects (Active, Vitesse, Securite,
  Memoire, Paquets): FerroSwift and Solidify are no longer listed in the
  README, the publishing guide or the website's footer.
- Website: every card of a shared link gives the size and description of its
  image, so that it shows large from the first share.
- `app.bind("…:0")` with HTTP/3 picks another port when the UDP port of the
  TCP port the system chose is taken or reserved (Windows reserves port ranges
  per protocol), instead of failing now and then.
- Heroku: the demo app is now built with the Rust buildpack `emk/rust` on the
  heroku-26 stack (`RustConfig` sets the Rust version and skips the
  buildpack's own build, `heroku-build.sh` builds the example, the `Procfile`
  starts it), by the Deploy button and by the "Deploy to Heroku" workflow.
  The Docker build (`heroku.yml` and the `Dockerfile`, on the container
  stack) remains available: set the repository variable `HEROKU_BUILD` to `docker`. The
  Heroku guide explains both, how to deploy another example, and how to
  deploy your own project with the buildpack and a `Procfile`.
- Heroku: the buildpack now deploys the Vitesse website (`site/`, served by
  Vitesse itself, with its documentation, its islands and the PWA) instead of
  the demo app: `heroku-build.sh` builds `vitesse-site` and the islands, the
  `Procfile` starts it with `SITE_DIR` and `DOCS_DIR`, and the "Deploy to
  Heroku" workflow also runs on changes to `site/` and `docs/`. The Docker
  build (`HEROKU_BUILD` set to `docker`) still deploys the demo app, or the
  example named by `ARG EXAMPLE` in the `Dockerfile`. The Heroku guide, the
  READMEs and the Docker guide say so, in English, French and Spanish.

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

Taking the best of Fastify, Laravel, FastAPI and axum (all opt-in, nothing
added to the path of a request that doesn't use them):

- Request validation (`vitesse::validate`, Laravel's `$request->validate()`,
  Fastify's schemas): implement `Validate` with chained rules
  (`v.field("email", &self.email).email()`, `not_empty`, `min_length`,
  `max_length`, `min`, `max`, `between`, `one_of`, `url`, `alphanumeric`,
  `check`, custom `message`s, `optional`/`required`, `nested`, `each`), then
  `req.valid_json()`, `req.valid_form()` or `req.valid_query()` answer `422`
  with the messages of every invalid field. New guide page "Validation".
- `Error::with_details(…)` and `err.details()`: structured details sent next
  to the message, as `{"error":"…","details":{…}}`.
- OpenAPI 3.1 (`vitesse::openapi`, FastAPI's `/docs`, `@fastify/swagger`):
  `app.openapi("/openapi.json", OpenApi::new(title, version))` serves the
  document generated from every route (path parameters and their
  constraints included), `.doc(Operation::new()…)` describes a route,
  `.swagger_ui("/docs")` serves Swagger UI, and `app.openapi_document(&spec)`
  returns it. New guide page "OpenAPI and Swagger UI".
- Lifecycle hooks (Fastify's `onRequest`, `preHandler`, `onSend`):
  `app.on_request`, `app.pre_handler` and `app.on_response`, plain functions
  that can change the request or the response, or stop the request with an
  error.
- `app.param(name, middleware)` (Express's `app.param`) and
  `app.bind_param(name, loader)` (Laravel's route model binding), on apps and
  routers: run before the handler of every route that has the parameter.
- Route groups: `app.group(prefix, |group| …)` (Laravel's
  `Route::prefix()->group()`), on apps and routers.
- Plugins: `app.register(plugin)` and the `Plugin` trait (Fastify's
  `register`), for any `FnOnce(&mut App)`.
- `req.params_as::<T>()`: every route parameter deserialized into a struct
  (axum's `Path<T>`).

The best of Phoenix, Laravel, FastAPI, Hono and supertest, all opt-in (an
application that doesn't use them runs exactly the same code per request):

- Problem Details (RFC 9457, `vitesse::problem`): `app.problem_details()`
  sends every error as `application/problem+json` (`type`, `title`,
  `status`, `detail`, `instance`, and the error's `details`),
  `app.problem_details_with(…)` completes each problem, and a handler can
  answer with a `Problem` of its own.
- Middleware stacks and named groups (Laravel's `web` and `api`):
  `middleware::Stack`, `app.middleware_group(name, stack)` and
  `middleware::group(name)`.
- Async hooks: `app.on_request_async` and `app.pre_handler_async`; hooks per
  router or group (Fastify's encapsulation): `router.on_request`,
  `router.on_response` and `router.on_request_async`.
- HTTP caching: `middleware::etag()` (`ETag` and `304 Not Modified` for the
  responses in memory) and `middleware::cache(ttl)`, a server-side cache of
  whole responses with a time to live, `X-Cache`, `Age`, a custom key and
  invalidation (`invalidate`, `invalidate_prefix`, `clear`).
- PubSub (`vitesse::pubsub`, Phoenix's PubSub and channels): topics shared
  by WebSocket connections, Server-Sent Events and tasks, with
  `subscribe`, `publish`, `broadcast_from`-style `Subscription::publish`,
  subscriber counts, `Subscription::sse(…)`, and `pubsub.relay(socket,
  topic)` for chat rooms over WebSocket. New guide page "PubSub and
  channels".
- A background job queue (`vitesse::queue`, Laravel's queues): workers,
  retries with exponential backoff, a rate limit, delayed jobs and
  `queue.idle()` for tests; and a scheduler (`vitesse::schedule`, Laravel's
  scheduler): `every`, `every_minute`, `hourly_at`, `daily_at`, without
  overlapping runs. New guide page "Background jobs and scheduled tasks".
- Typed routes: `TypedRoute<(String, u32)>` declares a pattern once, with
  the types of its parameters, to register the route, read them
  (`params(&req)`) and build links (`path`, `try_path`, `url`).
- OpenAPI: schemas derived from the types with the new `schemars` feature
  (`Operation::body_type::<T>()`, `form_type`, `validated_body` with its
  `422`, `response_type`, `query_type` and `OpenApi::schema_type`), Scalar
  and Redoc pages (`.scalar(path)`, `.redoc(path)`), and Swagger UI served
  from your own files (`.swagger_ui_assets(base)`).
- Testing: `client.agent()`, a client with a cookie jar (supertest's
  `agent`); fluent assertions on `TestResponse` (`assert_status`,
  `assert_ok`, `assert_header`, `assert_see`, `assert_json`,
  `assert_json_path`, `assert_json_includes`, `assert_json_count`,
  `assert_validation_errors`, `assert_cookie`, `assert_redirect`…),
  `res.json_path(…)`, `res.cookie(…)`; and `.cookie(…)`, `.bearer(…)` and
  `.query(…)` on test requests.

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
- The website (https://vitesse-f16135654c3d.herokuapp.com/) is served by Vitesse
  itself and written with [Active](https://active-c1883a966650.herokuapp.com/):
  pages rendered on the server with complete SEO tags, interactive parts
  hydrated as WebAssembly islands, real addresses per language (`/en/`,
  `/fr/docs/routing/`…, the old `#/docs/…` links lead to them), the
  documentation rendered on the server with its search, and a static export
  for GitHub Pages. It is a Progressive Web App: installable on a phone's
  home screen, and readable offline.
- Automatic releases: once CI is green on `master`, a new `version` in
  `Cargo.toml` is tagged (`v0.1.0` for `0.1.0`), released on GitHub with its
  section of `CHANGELOG.md` as notes, and published on crates.io.

[0.1.0]: https://crates.io/crates/vitesse/0.1.0
