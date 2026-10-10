# Docker

A Vitesse app compiles to a single native binary, which makes it easy to ship as a small Docker image. The repository includes a ready-to-use multi-stage `Dockerfile` (the one Heroku uses) that you can run on your computer or deploy to any container platform.

## Build and run locally

You need Docker (Docker Desktop on macOS and Windows, Docker Engine on Linux). From the root of the repository:

```sh
docker build -t my-app .
docker run --rm -p 8080:8080 my-app
```

Then open http://localhost:8080. The image runs the demo app ([`examples/demo.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/demo.rs)):

```sh
curl localhost:8080/health
curl localhost:8080/api/hello/Ada
curl -X POST localhost:8080/api/todos -H 'content-type: application/json' \
     -d '{"title":"Ship it"}'
curl localhost:8080/api/todos
```

Press `Ctrl+C` to stop it. Vitesse handles `SIGINT` and `SIGTERM` itself, so it shuts down gracefully even though it runs as the container's main process (PID 1): no extra init process is needed.

> [!TIP]
> `docker stop` sends `SIGTERM`, then kills the container after 10 seconds. Vitesse also gives in-flight requests up to 10 seconds to finish, so leave Docker a little more time: `docker run --stop-timeout 15 …` or `docker stop -t 15 <container>`.

## What the Dockerfile does

The [`Dockerfile`](https://github.com/maxlestage/Vitesse/blob/master/Dockerfile) has two stages:

1. **Build** (`rust:1-slim-bookworm`): copies `Cargo.toml`, `Cargo.lock`, `src/` and `examples/`, then compiles one example with `cargo build --release --example $EXAMPLE`.
2. **Runtime** (`debian:bookworm-slim`): creates an unprivileged `vitesse` user, copies **only the binary** to `/usr/local/bin/server`, sets `ENV PORT=8080` and `EXPOSE 8080`, and starts it with `CMD ["server"]`.

The [`.dockerignore`](https://github.com/maxlestage/Vitesse/blob/master/.dockerignore) file keeps `target/`, `site/`, `docs/`, `bench/`, `.git/` and other unneeded folders out of the build context, so builds are faster and never depend on what happens to be on your disk.

### Why two stages?

The build stage contains the whole Rust toolchain and all intermediate files: several hundred megabytes that are useless at runtime. The final image only keeps Debian slim plus your binary: around 115 MB for the demo, most of it being the Debian base. Smaller images download faster, start faster and contain fewer things that could be attacked. Check the size with:

```sh
docker image ls my-app
```

The runtime stage uses the same Debian release as the build stage (Bookworm) because the binary is linked against the system's C library (glibc). Going even smaller is possible, for example with a "distroless" base image or a fully static binary built for the `musl` target, but those setups are up to you.

> [!NOTE]
> The build compiles everything from scratch with the release profile (`lto = "fat"`, `codegen-units = 1`), so count a few minutes. The build doesn't use `--locked`: the crates listed in `Cargo.lock` keep their locked versions, and a dependency added to `Cargo.toml` without updating `Cargo.lock` (from a phone, for instance) is resolved during the build. Committing an up-to-date `Cargo.lock` is still the way to get reproducible builds.

## Choosing the example

The `EXAMPLE` build argument selects the file of `examples/` to compile (its name without `.rs`). It defaults to `demo`:

```sh
docker build --build-arg EXAMPLE=rest_api -t api .
```

The image sets `PORT=8080`, but only an app that reads `PORT` uses it; `demo` does. If an example listens on a fixed port instead (`rest_api`, for instance, calls `app.run(3000)`), publish that port:

```sh
docker run --rm -p 8080:3000 api
```

Platforms like Heroku build the image without passing build arguments. To change the example they use, edit the default value in the `Dockerfile`:

```dockerfile
ARG EXAMPLE=my_app
```

To make any app work everywhere, read the port from the environment:

```rust
let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
app.run(port) // a bare port listens on every interface (0.0.0.0)
```

## Your own project

If your app is a regular crate (with a `src/main.rs`) rather than an example, the same pattern applies. Replace `my-app` with the name of your package:

```dockerfile
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked && cp target/release/my-app /server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home app
COPY --from=build /server /usr/local/bin/server
USER app
ENV PORT=8080
EXPOSE 8080
CMD ["server"]
```

Add a `.dockerignore` file containing at least `target/` and `.git/`. Here `--locked` makes the build fail if `Cargo.lock` is out of date, which guarantees reproducible builds; remove it if you edit dependencies without regenerating the lock file.

### Static files and other assets

The runtime image only contains the binary. If your app serves a folder (`app.static_dir("/assets", "public")`, see [Static files](static-files.md)) or reads files at runtime, copy them into the runtime stage and set the working directory, because relative paths are resolved from the current directory:

```dockerfile
WORKDIR /app
COPY public ./public
```

Put these lines in the runtime stage, before `USER`, and make sure `.dockerignore` doesn't exclude the folder.

## Environment variables

- **`PORT`**: the listening port. The image defaults to `8080`, and most platforms override it. Locally: `docker run -e PORT=9000 -p 9000:9000 my-app`.
- **Your own settings**: pass them with `-e NAME=value` or `--env-file .env`, and read them with `std::env::var("NAME")`.

> [!WARNING]
> Never write secrets in the `Dockerfile` (`ENV API_TOKEN=…`): anyone who has the image can read them, for example with `docker history`. Pass them when the container starts.

## HTTP/3: publish the UDP port

`EXPOSE` and `-p` mean TCP unless told otherwise. If your app serves [HTTP/3](http3.md) (the `http3` feature) on UDP port 443, declare and publish that port too, and give the container its certificate:

```dockerfile
EXPOSE 8080
EXPOSE 443/udp
```

```sh
docker run --rm -p 127.0.0.1:8080:8080 -p 443:443/udp \
  -v /etc/my-app/tls:/etc/my-app/tls:ro my-app
```

Here, TCP port 8080 is only reachable from the host, for the reverse proxy that handles HTTPS, while UDP port 443 is public. The mounted files must be readable by the container's user (Let's Encrypt keys are only readable by root by default). If the container listens on another UDP port, for example `-p 443:8443/udp`, announce the public port with `.alt_svc_port(443)`.

HTTP/3 needs UDP all the way to the container: Heroku doesn't route UDP, and platforms such as Cloud Run or Render terminate HTTP/3 at their edge when they offer it. On those platforms, leave the feature off.

## Logs and health checks

- **Logs**: `middleware::logger()` writes one line per request on standard output, without colour codes when it isn't attached to a terminal. Read them with `docker logs -f <container>`. The causes of `5xx` errors go to standard error.
- **Health checks**: the demo exposes `GET /health`. The slim image doesn't include `curl`, so a `HEALTHCHECK` instruction based on `curl` won't work as is: prefer your platform's HTTP health check (Kubernetes probes, Fly.io, Render, Cloud Run…) pointed at `/health`.

## Deploying the same image elsewhere

Most container platforms give the port in the `PORT` variable, so the image usually works unchanged. Interfaces change often: check each platform's documentation.

| Platform | In short |
|---|---|
| Heroku | See [Deploy to Heroku from your phone](heroku-mobile.md). Heroku builds the image itself from `heroku.yml`. |
| Render | Create a web service from your repository with the Docker runtime; Render builds the `Dockerfile` and sets `PORT`. |
| Railway | Detects the `Dockerfile` and provides `PORT`. |
| Fly.io | `fly launch` detects the `Dockerfile`; check that `internal_port` in `fly.toml` matches the port your app listens on (`8080` with this image). |
| Google Cloud Run | `gcloud run deploy --source .` builds the `Dockerfile`; Cloud Run sets `PORT` (8080 by default). |

To push the image to a registry yourself:

```sh
docker tag my-app ghcr.io/<you>/my-app:latest
docker push ghcr.io/<you>/my-app:latest
```

## Multi-architecture images

An image is built for the processor architecture of the machine that builds it. On an Apple Silicon Mac (arm64), a locally built image won't run on an x86-64 (amd64) server such as Heroku's. Ask for the target platform explicitly:

```sh
docker build --platform linux/amd64 -t my-app .
```

To publish a single image for both architectures, use `buildx`:

```sh
docker buildx build --platform linux/amd64,linux/arm64 \
  -t ghcr.io/<you>/my-app:latest --push .
```

The `rust` and `debian` base images exist for both architectures. Building for a foreign architecture goes through emulation, which can make Rust compilation much slower.

## Learn more

- [Going to production](production.md): reverse proxy, graceful shutdown, limits, security.
- [Server configuration](server.md): listening addresses, workers, threads.
