# Vitesse documentation

Everything you need to build fast web apps with Vitesse, the Express.js-style framework for Rust: from your first route to a production deployment. You can also read these pages on the website: https://maxlestage.github.io/Vitesse/en/docs/

## Getting started

- [Introduction](introduction.md): what Vitesse is, its philosophy and what it offers.
- [Installation](installation.md): add Vitesse to a project with Cargo.
- [Your first app](first-app.md): build and run a small app, step by step.

## Essentials

- [Routing](routing.md): routes, HTTP methods, path parameters, wildcards and the matched route.
- [Reading requests](requests.md): parameters, query strings, headers, cookies, content negotiation, bodies (JSON, forms) and file uploads.
- [Sending responses](responses.md): text, JSON, HTML, status codes, headers, cookies, redirects, files, streaming and Server-Sent Events.
- [Middleware](middleware.md): global and per-route middleware, `next`, the built-in middleware (logging, CORS, authentication, rate limiting, compression…) and tower layers.
- [Routers](routers.md): group routes with `Router`, mount them under a prefix, with their own middleware and fallback.
- [Shared state](state.md): share configuration, counters or connection pools between requests.
- [Error handling](errors.md): `vitesse::Error`, the `?` operator, custom error responses and panics.
- [Static files](static-files.md): serve a folder of files with caching, ranges and built-in safety checks.
- [WebSocket](websocket.md): real-time, two-way connections with `app.ws`, from an echo server to a chat room.

## Going further

- [Testing](testing.md): test your app in memory with `TestClient`, without any network.
- [Server configuration](server.md): `run`, `listen` and `bind`, listening addresses and Unix sockets, workers, timeouts, limits and shutdown.
- [HTTPS and HTTP/2](https.md): serve HTTPS and HTTP/2 yourself with rustls, certificates, redirects and h2c.
- [HTTP/3 and QUIC](http3.md): serve your app over HTTP/3 next to HTTP/1.1, certificates and deployment.
- [Performance](performance.md): why Vitesse is fast, the benchmarks, and tuning tips.
- [Coming from Express](from-express.md): the Express → Vitesse equivalence table and migration tips.
- [Coming from axum or actix-web](from-axum-actix.md): the equivalents, feature by feature.

## Deployment

- [Deploy to Heroku from your phone](heroku-mobile.md): the one-tap Deploy button and automatic deployments with GitHub Actions, all from a phone.
- [Docker](docker.md): build and run the multi-stage image, then deploy it to container platforms.
- [Going to production](production.md): release builds, HTTPS and reverse proxies, the client's real address, systemd, graceful shutdown, limits, security and monitoring.

## Reference

- [API cheat sheet](cheatsheet.md): the whole API at a glance.
- [FAQ & limitations](faq.md): common questions, and what Vitesse doesn't do (yet).

## More resources

- API reference generated from the source code: https://docs.rs/vitesse
- Source code, issues and examples: https://maxlestage.github.io/Vitesse/
- Also available in [French](https://maxlestage.github.io/Vitesse/fr/docs/README/) and [Spanish](https://maxlestage.github.io/Vitesse/es/docs/README/).
