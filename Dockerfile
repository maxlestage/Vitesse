# Production image for a Vitesse app (used by Heroku, and usable as is on
# Render, Fly.io, Cloud Run…).
#
#   docker build -t vitesse-demo .
#   docker run -p 8080:8080 vitesse-demo
#
# To build another example: --build-arg EXAMPLE=rest_api

FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples
ARG EXAMPLE=demo
# No --locked: a dependency added from a phone (where Cargo.lock cannot be
# regenerated) is resolved here; the others keep their locked versions.
RUN cargo build --release --example "$EXAMPLE" \
    && cp "target/release/examples/$EXAMPLE" /server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home vitesse
COPY --from=build /server /usr/local/bin/server
USER vitesse
# Heroku sets its own PORT at startup; 8080 is the default elsewhere.
ENV PORT=8080
EXPOSE 8080
CMD ["server"]
