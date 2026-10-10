# Static files

Vitesse serves the files of a folder (CSS, JavaScript, images, a front-end build…) with everything you'd expect from `express.static`: MIME types, `index.html`, browser caching with `ETag` and `304` responses, range requests for video, streaming of large files and protection against path traversal.

## Serving a folder under a prefix

```js
app.use('/assets', express.static('public'));
```

```rust
use vitesse::prelude::*;

let mut app = App::new();
app.static_dir("/assets", "public");
```

`public/css/app.css` is now served at `/assets/css/app.css`, and `public/index.html` at `/assets/`. Under the hood, `static_dir` registers two routes, `GET` and `HEAD` on `/assets/*`. A file that doesn't exist answers `404 {"error":"Cannot GET /assets/nope.css"}`, which goes through [`app.on_error`](errors.md) like any other error.

> [!NOTE]
> A relative folder is resolved from the directory the server is launched from (usually the project root with `cargo run`), not from the source file. In production, launch the binary from the right directory or use an absolute path.

## At the root, before your routes

To serve files at the root of the site, use `ServeDir` as a **middleware**: if the requested file exists, it is sent; otherwise the request continues to your routes. This is exactly `app.use(express.static('public'))`:

```rust
app.middleware(ServeDir::new("public"));
app.get("/api/hello", |_| async { "Hello!" }); // still reachable
```

`middleware::serve_static("public")` is a synonym. In this mode only `GET` and `HEAD` requests are considered, and a missing or refused file doesn't produce an error: the request just carries on.

> [!TIP]
> As a global middleware, `ServeDir` looks up the file system for every `GET` request before routing. For an API with heavy traffic, prefer a prefix (`app.static_dir("/assets", ...)`), which only costs something for the requests under that prefix.

`app.static_dir("/", "public")` also works, but it's a different trade-off: it declares a `GET /*` wildcard route. Your other routes still win (a wildcard has the lowest priority), but every unknown `GET` path then gets the static `404`, and never reaches `app.fallback`.

## Options: `ServeDir`

For more control, configure a `ServeDir` and mount it with `serve_dir` (or `app.middleware(...)`):

```rust
use std::time::Duration;

app.serve_dir(
    "/assets",
    ServeDir::new("public")
        .max_age(Duration::from_secs(3600))
        .index(None)
        .dotfiles(false),
);
```

| Option | Default | Effect |
|---|---|---|
| `index(Some("home.html"))` / `index(None)` | `Some("index.html")` | file served for a directory; `None` disables it |
| `max_age(duration)` | `0` | browser cache duration: `Cache-Control: public, max-age=...` |
| `dotfiles(true)` | `false` | allows hidden files and folders (`.well-known`…) |

`static_dir`, `serve_dir` and `ServeDir` are also available on a [`Router`](routers.md), which lets you protect files with the router's middleware:

```rust
let mut private = Router::new();
private.middleware(require_login); // your authentication middleware
private.static_dir("/", "private-files");
app.mount("/private", private); // GET /private/report.pdf
```

## Directories and `index.html`

A request for a directory serves its index file: `/assets/docs/` returns `public/docs/index.html`. Like Express, a request without the trailing slash (`/assets/docs`) is redirected with a `301` to `/assets/docs/` (keeping the query string), so that relative links in the page keep working. With `index(None)`, directories aren't served.

## Browser caching: `ETag`, `Last-Modified` and `304`

Each file is sent with:

- `Content-Type`, guessed from the extension;
- `ETag`, a weak validator computed from the size and modification date (`W/"1a2b-65f0c3d1"`);
- `Last-Modified`;
- `Cache-Control: public, max-age=...` (`max-age=0` by default: the browser checks with the server before reusing its copy);
- `Accept-Ranges: bytes`.

When the browser already has the right version (`If-None-Match` or `If-Modified-Since`), Vitesse answers `304 Not Modified` without a body:

```http
GET /assets/app.css HTTP/1.1
If-None-Match: W/"1a2b-65f0c3d1"

HTTP/1.1 304 Not Modified
etag: W/"1a2b-65f0c3d1"
```

> [!TIP]
> If your front-end build puts a hash in file names (`app.3f9c2b.js`), these files never change: serve them with a long cache duration (`max_age(Duration::from_secs(31_536_000))`, one year), and keep `index.html`, which references them, on the default `max-age=0`. `max_age` applies to a whole `ServeDir`, so use two of them, one per folder.

## Range requests

Video and audio players, and download managers, ask for parts of a file with the `Range` header. Vitesse supports a single range per request:

| `Range` header | Response |
|---|---|
| `bytes=0-499`, `bytes=500-`, `bytes=-500` (the last 500 bytes) | `206 Partial Content` with `Content-Range: bytes 0-499/1234` |
| a range beyond the end of the file | `416 Range Not Satisfiable` with `Content-Range: bytes */1234` |
| several ranges (`bytes=0-1,5-6`) | the whole file, `200` |

To resume a download, a client may add `If-Range`: the range is then only served if `If-Range` equals the file's `Last-Modified` date. Otherwise the file has changed, and the whole file is sent with a `200`. Vitesse's ETags are weak, so an ETag in `If-Range` never matches and also gets the whole file.

## Large files

Files up to 256 KiB are read in one go; larger files are streamed in 64 KiB chunks, with their exact `Content-Length`. Memory use therefore stays constant, even for a video of several gigabytes. For a `HEAD` request, only the headers are sent.

## MIME types

| Extensions | `Content-Type` |
|---|---|
| `html`, `htm` | `text/html; charset=utf-8` |
| `css` | `text/css; charset=utf-8` |
| `js`, `mjs`, `cjs` | `text/javascript; charset=utf-8` |
| `json`, `map` | `application/json` |
| `txt`, `log`, `md`, `csv` | `text/plain`, `text/markdown`, `text/csv` (`charset=utf-8`) |
| `xml`, `webmanifest` | `application/xml`, `application/manifest+json` |
| `svg`, `png`, `jpg`/`jpeg`, `gif`, `webp`, `avif`, `ico`, `bmp` | `image/...` (`image/svg+xml` for SVG, `image/x-icon` for `ico`) |
| `woff`, `woff2`, `ttf`, `otf` | `font/...` |
| `pdf`, `zip`, `gz`, `tar`, `wasm` | `application/pdf`, `application/zip`, `application/gzip`, `application/x-tar`, `application/wasm` |
| `mp3`, `ogg`, `wav`, `mp4`, `webm` | `audio/mpeg`, `audio/ogg`, `audio/wav`, `video/mp4`, `video/webm` |
| anything else | `application/octet-stream` |

## Security

`ServeDir` only ever serves files located inside its folder:

- `..` segments, backslashes and null bytes are refused, including when they are percent-encoded (`%2e%2e%2f`);
- hidden files and folders (`.env`, `.git/`, `.htpasswd`…) are refused, unless you enable `dotfiles(true)`;
- only `GET` and `HEAD` are served.

A refused path is treated like a missing file: `404` with `static_dir`, next middleware or route with `app.middleware(ServeDir::new(...))`.

> [!WARNING]
> Serve a dedicated folder (`public/`, `dist/`), never the project root: your `Cargo.toml`, your sources or a configuration file would become downloadable. Symbolic links inside the folder are followed, so don't put links to sensitive locations in it.

Vitesse doesn't compress responses (gzip, brotli). In production, put a reverse proxy or a CDN in front of the server for compression and TLS (see [Production](production.md)).

## Sending a single file

To send one particular file from a handler, use `res::file(path).await`, or `res::download(path, name).await` to trigger a download (see [Sending responses](responses.md)). Like Express's `res.sendFile`, these helpers handle caching and ranges exactly as `ServeDir` does: `ETag` and `Last-Modified` headers, `304 Not Modified`, `206 Partial Content` (with `If-Range`) and `416`, as described above. `res::download` keeps its `Content-Disposition: attachment` header on partial responses too. Only the cache duration can't be configured: they always send `max-age=0`.

## Single-page applications (SPA)

A React, Vue or Svelte app handles its own routes in the browser: `/users/42/profile` must return `index.html`, while real files (`/assets/app.js`) and the API work normally. Combine the `ServeDir` middleware with a fallback:

```rust
let mut api = Router::new();
api.get("/users", |_| async { Json(vec!["ada", "grace"]) });

let mut app = App::new();
app.mount("/api", api);
app.middleware(ServeDir::new("dist"));
app.fallback(|req: Request| async move {
    // API requests and other methods keep a real 404.
    if req.method() != Method::GET || req.path().starts_with("/api/") {
        return Error::not_found(format!("Cannot {} {}", req.method(), req.path())).into_response();
    }
    res::file("dist/index.html").await
});
```

For a request, the order is: files in `dist/` first (the middleware runs before routing), then API routes, then `index.html` for everything else, so a file named `dist/api/...` would hide an API route.
