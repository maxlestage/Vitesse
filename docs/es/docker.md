# Docker

Una aplicación Vitesse se compila en un único binario nativo, lo que facilita distribuirla en una imagen Docker pequeña. El repositorio incluye un `Dockerfile` multietapa listo para usar (el mismo que usa Heroku), que puedes ejecutar en tu computadora o desplegar en cualquier plataforma de contenedores.

## Construir y ejecutar en local

Necesitas Docker (Docker Desktop en macOS y Windows, Docker Engine en Linux). Desde la raíz del repositorio:

```sh
docker build -t mi-app .
docker run --rm -p 8080:8080 mi-app
```

Después abre http://localhost:8080. La imagen ejecuta la aplicación de demostración ([`examples/demo.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/demo.rs)):

```sh
curl localhost:8080/health
curl localhost:8080/api/hello/Ada
curl -X POST localhost:8080/api/todos -H 'content-type: application/json' \
     -d '{"title":"Publicarlo"}'
curl localhost:8080/api/todos
```

Detenla con `Ctrl+C`. Vitesse gestiona por sí mismo `SIGINT` y `SIGTERM`, así que se apaga de forma ordenada aunque sea el proceso principal del contenedor (PID 1): no hace falta ningún proceso init adicional.

> [!TIP]
> `docker stop` envía `SIGTERM` y mata el contenedor a los 10 segundos. Vitesse también da hasta 10 segundos a las peticiones en curso para terminar, así que deja a Docker un poco más de margen: `docker run --stop-timeout 15 …` o `docker stop -t 15 <contenedor>`.

## Qué hace el Dockerfile

El [`Dockerfile`](https://github.com/maxlestage/Vitesse/blob/master/Dockerfile) tiene dos etapas:

1. **Construcción** (`rust:1-slim-bookworm`): copia `Cargo.toml`, `Cargo.lock`, `src/` y `examples/`, y compila un ejemplo con `cargo build --release --example $EXAMPLE`.
2. **Ejecución** (`debian:bookworm-slim`): crea un usuario sin privilegios `vitesse`, copia **solo el binario** en `/usr/local/bin/server`, define `ENV PORT=8080` y `EXPOSE 8080`, y lo arranca con `CMD ["server"]`.

El archivo [`.dockerignore`](https://github.com/maxlestage/Vitesse/blob/master/.dockerignore) deja fuera del contexto de construcción `target/`, `site/`, `docs/`, `bench/`, `.git/` y otras carpetas innecesarias: las construcciones son más rápidas y nunca dependen de lo que tengas en el disco.

### ¿Por qué dos etapas?

La etapa de construcción contiene toda la cadena de herramientas de Rust y los archivos intermedios: varios cientos de megabytes que no sirven de nada en ejecución. La imagen final solo conserva Debian slim y tu binario: unos 115 MB para la demo, la mayor parte por la base Debian. Una imagen más pequeña se descarga antes, arranca antes y ofrece menos superficie de ataque. Comprueba su tamaño con:

```sh
docker image ls mi-app
```

La etapa de ejecución usa la misma versión de Debian que la de construcción (Bookworm) porque el binario está enlazado con la biblioteca C del sistema (glibc). Se puede ir aún más lejos, por ejemplo con una imagen base «distroless» o con un binario totalmente estático compilado para el target `musl`, pero esas configuraciones quedan en tus manos.

> [!NOTE]
> La construcción compila todo desde cero con el perfil release (`lto = "fat"`, `codegen-units = 1`), así que calcula unos minutos. La construcción no usa `--locked`: los crates de `Cargo.lock` conservan sus versiones fijadas, y una dependencia añadida a `Cargo.toml` sin actualizar `Cargo.lock` (desde el teléfono, por ejemplo) se resuelve durante la construcción. Hacer commit de un `Cargo.lock` al día sigue siendo la forma de tener construcciones reproducibles.

## Elegir el ejemplo

El argumento de construcción `EXAMPLE` elige el archivo de `examples/` que se compila (su nombre sin `.rs`). Su valor por defecto es `demo`:

```sh
docker build --build-arg EXAMPLE=rest_api -t api .
```

La imagen define `PORT=8080`, pero solo la usa una aplicación que lea `PORT`; `demo` lo hace. Si un ejemplo escucha en un puerto fijo (`rest_api`, por ejemplo, llama a `app.run(3000)`), publica ese puerto:

```sh
docker run --rm -p 8080:3000 api
```

Las plataformas como Heroku construyen la imagen sin pasar argumentos de construcción. Para cambiar el ejemplo que usan, edita el valor por defecto en el `Dockerfile`:

```dockerfile
ARG EXAMPLE=mi_app
```

Para que una aplicación funcione en cualquier sitio, lee el puerto del entorno:

```rust
let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
app.run(port) // un puerto a secas escucha en todas las interfaces (0.0.0.0)
```

## Tu propio proyecto

Si tu aplicación es un crate normal (con un `src/main.rs`) y no un ejemplo, el esquema es el mismo. Sustituye `mi-app` por el nombre de tu paquete:

```dockerfile
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked && cp target/release/mi-app /server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home app
COPY --from=build /server /usr/local/bin/server
USER app
ENV PORT=8080
EXPOSE 8080
CMD ["server"]
```

Añade un archivo `.dockerignore` que contenga al menos `target/` y `.git/`. Aquí `--locked` hace fallar la construcción si `Cargo.lock` no está al día, lo que garantiza construcciones reproducibles; quítalo si modificas las dependencias sin regenerar el archivo de bloqueo.

### Archivos estáticos y otros recursos

La imagen de ejecución solo contiene el binario. Si tu aplicación sirve una carpeta (`app.static_dir("/assets", "public")`, consulta [Archivos estáticos](static-files.md)) o lee archivos en ejecución, cópialos en la etapa de ejecución y define el directorio de trabajo, porque las rutas relativas parten del directorio actual:

```dockerfile
WORKDIR /app
COPY public ./public
```

Pon estas líneas en la etapa de ejecución, antes de `USER`, y comprueba que `.dockerignore` no excluye la carpeta.

## Variables de entorno

- **`PORT`**: el puerto de escucha. La imagen usa `8080` por defecto y la mayoría de las plataformas lo sustituyen. En local: `docker run -e PORT=9000 -p 9000:9000 mi-app`.
- **Tu propia configuración**: pásala con `-e NOMBRE=valor` o `--env-file .env`, y léela con `std::env::var("NOMBRE")`.

> [!WARNING]
> Nunca escribas secretos en el `Dockerfile` (`ENV API_TOKEN=…`): cualquiera que tenga la imagen puede leerlos, por ejemplo con `docker history`. Pásalos al arrancar el contenedor.

## HTTP/3: publicar el puerto UDP

`EXPOSE` y `-p` se refieren a TCP salvo que se indique otra cosa. Si tu aplicación sirve [HTTP/3](http3.md) (la feature `http3`) en el puerto UDP 443, declara y publica también ese puerto, y dale al contenedor su certificado:

```dockerfile
EXPOSE 8080
EXPOSE 443/udp
```

```sh
docker run --rm -p 127.0.0.1:8080:8080 -p 443:443/udp \
  -v /etc/mi-app/tls:/etc/mi-app/tls:ro mi-app
```

Aquí, el puerto TCP 8080 solo es accesible desde el host, para el proxy inverso que gestiona HTTPS, mientras que el puerto UDP 443 es público. El usuario del contenedor debe poder leer los archivos montados (por defecto, las claves de Let's Encrypt solo las puede leer root). Si el contenedor escucha en otro puerto UDP, por ejemplo con `-p 443:8443/udp`, anuncia el puerto público con `.alt_svc_port(443)`.

HTTP/3 necesita que el UDP llegue hasta el contenedor: Heroku no enruta UDP, y plataformas como Cloud Run o Render terminan HTTP/3 en su borde cuando lo ofrecen. En esas plataformas, deja la feature desactivada.

## Registros y comprobaciones de salud

- **Registros**: `middleware::logger()` escribe una línea por petición en la salida estándar, sin códigos de color cuando no está conectada a una terminal. Léelos con `docker logs -f <contenedor>`. Las causas de los errores `5xx` van a la salida de error.
- **Comprobaciones de salud**: la demo expone `GET /health`. La imagen slim no incluye `curl`, así que una instrucción `HEALTHCHECK` basada en `curl` no funcionará tal cual: es mejor usar la comprobación HTTP de tu plataforma (sondas de Kubernetes, Fly.io, Render, Cloud Run…) apuntando a `/health`.

## Desplegar la misma imagen en otros sitios

La mayoría de las plataformas de contenedores indican el puerto en la variable `PORT`, así que la imagen suele funcionar sin cambios. Sus interfaces cambian a menudo: consulta la documentación de cada plataforma.

| Plataforma | En resumen |
|---|---|
| Heroku | Consulta [Desplegar en Heroku desde el móvil](heroku-mobile.md). Heroku construye la imagen por sí mismo a partir de `heroku.yml`. |
| Render | Crea un servicio web a partir de tu repositorio con el entorno Docker; Render construye el `Dockerfile` y define `PORT`. |
| Railway | Detecta el `Dockerfile` y proporciona `PORT`. |
| Fly.io | `fly launch` detecta el `Dockerfile`; comprueba que `internal_port` en `fly.toml` coincide con el puerto en el que escucha tu aplicación (`8080` con esta imagen). |
| Google Cloud Run | `gcloud run deploy --source .` construye el `Dockerfile`; Cloud Run define `PORT` (8080 por defecto). |

Para subir tú mismo la imagen a un registro:

```sh
docker tag mi-app ghcr.io/<tu-usuario>/mi-app:latest
docker push ghcr.io/<tu-usuario>/mi-app:latest
```

## Imágenes multiarquitectura

Una imagen se construye para la arquitectura del procesador de la máquina que la construye. En un Mac con Apple Silicon (arm64), una imagen construida en local no funcionará en un servidor x86-64 (amd64) como los de Heroku. Indica la plataforma de destino de forma explícita:

```sh
docker build --platform linux/amd64 -t mi-app .
```

Para publicar una sola imagen para ambas arquitecturas, usa `buildx`:

```sh
docker buildx build --platform linux/amd64,linux/arm64 \
  -t ghcr.io/<tu-usuario>/mi-app:latest --push .
```

Las imágenes base `rust` y `debian` existen para ambas arquitecturas. Construir para una arquitectura distinta pasa por emulación, lo que puede ralentizar mucho la compilación de Rust.

## Para saber más

- [Puesta en producción](production.md): proxy inverso, apagado ordenado, límites, seguridad.
- [Configuración del servidor](server.md): direcciones de escucha, workers, threads.
