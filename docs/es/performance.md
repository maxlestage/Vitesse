# Rendimiento

Vitesse se diseñó para añadir el menor trabajo posible alrededor de lo que el kernel ya hace en cada petición. Esta página muestra los resultados del benchmark, cómo se midieron y cómo reproducirlos, explica de dónde sale la velocidad y da consejos prácticos para que tu propia aplicación siga siendo rápida.

## Benchmark

Peticiones por segundo y, entre paréntesis, el tiempo de CPU que consume el servidor por cada petición (cuanto más bajo, mejor), medidos con el nuevo motor por hilo de Vitesse:

| Escenario | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|
| `GET /` (texto) | 233.459 (8,4 µs) | **359.229 (5,4 µs)** | 347.836 (**5,1 µs**) |
| `GET /json` | 215.706 (9,1 µs) | 346.652 (5,6 µs) | **350.972 (5,1 µs)** |
| `GET /json` enviado por un navegador (12 cabeceras) | 179.989 (11,0 µs) | 256.421 (7,7 µs) | **349.424 (5,3 µs)** |
| `GET /users/:id` (parámetro + JSON) | 204.665 (9,6 µs) | 304.271 (6,4 µs) | **359.307 (5,2 µs)** |
| `POST /echo` (lee y devuelve JSON) | 164.843 (12,0 µs) | 227.272 (8,7 µs) | **365.321 (5,3 µs)** |
| `GET /` con pipelining ×16 | 366.673 (5,4 µs) | 1.612.135 (1,2 µs) | **3.288.486 (0,51 µs)** |

- **Frente a actix-web**, el framework de Rust con fama de ser el más rápido: un +1,2 % en `GET /json` y un −3 % en `GET /`, donde ambos están frenados por el kernel y el generador de carga (Vitesse consume aun así un 6 % menos de CPU), un **+36 %** con una petición real de navegador, un **+18 %** con un parámetro, un **+61 %** en el eco JSON, **el doble** con pipelining y entre un 6 y un 58 % menos de CPU por petición. Con 2.048 conexiones, con pipelining: 3,0 millones de peticiones por segundo frente a 1,27 millones (×2,4).
- **Frente a axum**: de 1,5 a 2,2 veces más peticiones por segundo, de 1,7 a 2,3 veces menos CPU por petición y 9 veces más con pipelining.
- **Frente a Drogon (C++) y Express**: se midieron en una sesión anterior, en otra máquina, antes del motor por hilo. Vitesse era entonces entre 1.4 y 3.8 veces más rápido que Drogon y unas 50 veces más rápido que Express.

> [!NOTE]
> Drogon y Express no se pudieron volver a medir en esta máquina: la comparación de arriba viene de una sesión anterior, con Drogon 1.9.13 (GCC 13, `-O3`) y Express 5.3.0 sobre Node 22.22. Desde entonces, el servidor Express se ha retirado del repositorio para que el proyecto siga siendo 100 % Rust, sin JavaScript: sigue en el historial de git (`git show 484eed3:bench/express/server.js`), y la herramienta de benchmark compara ahora Drogon, axum, actix-web y Vitesse.

### ¿Por qué la diferencia con actix es menor en `GET /`?

En la petición más simple, la mayor parte de los ~5 µs que cuesta una petición es trabajo del kernel (lectura, escritura y, en la interfaz de loopback, la recepción del lado del cliente imputada al envío del servidor), y el propio generador de carga se queda sin aliento: ninguno de los dos servidores usa sus dos núcleos por completo. El código del framework apenas pesa, y ambos chocan con el mismo suelo. En cuanto la petición se parece a una real (cabeceras de navegador, parámetros, cuerpo JSON, pipelining), ese código marca la diferencia y la brecha se amplía. En producción, a través de una red real, la parte del trabajo del kernel del lado del servidor es menor, así que la ventaja de Vitesse se nota todavía más.

## Metodología

- **Máquina**: una VM con 4 vCPU, más rápida que la de las mediciones anteriores (así que compara las columnas entre sí, no con cifras más antiguas). El servidor está fijado a 2 núcleos y wrk a los otros 2, así el generador de carga nunca le quita CPU al servidor.
- **Carga**: 128 conexiones keep-alive, 5 segundos por ejecución y la mediana de 3 ejecuciones por escenario, un servidor detrás de otro, todo en una misma sesión el 7 de noviembre de 2026.
- **Versiones**: axum 0.8, actix-web 4.18, Rust 1.97. Vitesse usa su asignador de memoria por defecto (mimalloc), y axum y actix-web el del sistema.
- **Escenarios**: el escenario «navegador» envía las 12 cabeceras de una petición real de Chrome (unos 650 bytes); `POST /echo` envía un pequeño documento JSON que el servidor analiza y devuelve; el escenario en pipeline envía 16 peticiones de golpe por cada conexión.
- **Tiempo de CPU por petición**: el tiempo de CPU (usuario + sistema) que consume el proceso del servidor durante el escenario, dividido entre el número de peticiones atendidas. Sin pipeline, los servidores más rápidos saturan al propio wrk: el tiempo de CPU medido en el lado del servidor es entonces el juez más fiable.

Los resultados varían unos pocos puntos porcentuales de una ejecución a otra.

## Reproducir el benchmark

La herramienta de benchmark es un pequeño programa en Rust, `bench/runner`, y el código de cada servidor está en `bench/` (el servidor de Vitesse es `examples/bench.rs`). Necesitas Linux (la herramienta usa `taskset` y lee `/proc`), Rust y wrk. Drogon es opcional: si está instalado, la herramienta compila su servidor desde `bench/drogon` con CMake (indica su ruta de instalación con `DROGON_PREFIX` si hace falta); si no, se omite.

```sh
cargo run --release --manifest-path bench/runner/Cargo.toml              # 10 s por escenario, 128 conexiones
cargo run --release --manifest-path bench/runner/Cargo.toml -- 30s 256   # duración y número de conexiones personalizados
SERVER_CPUS=0-3 CLIENT_CPUS=4-7 cargo run --release --manifest-path bench/runner/Cargo.toml   # en una máquina de 8 núcleos
```

La herramienta compila cada servidor en modo release, los arranca uno tras otro (Drogon, axum, actix-web, Vitesse) con el servidor y wrk fijados en núcleos distintos, ejecuta los seis escenarios (los scripts de wrk están en `bench/lua/`) y muestra los resultados como una tabla Markdown, con el tiempo de CPU del servidor por petición leído de `/proc`. `SERVER_CPUS` (por defecto `0,1`) y `CLIENT_CPUS` (por defecto `2,3`) aceptan listas en el formato de `taskset`, como `0,1`, `0-3` o `0-1,4`; el número de CPU del servidor también fija el número de hilos del servidor y de wrk. Para usar otro binario de wrk, indica `WRK=/ruta/a/wrk`.

Para probar rápidamente solo Vitesse:

```sh
cargo run --release --example bench    # escucha en el puerto 3000
wrk -t2 -c128 -d10s http://127.0.0.1:3000/json
```

El servidor del benchmark también lee `PORT`, `WORKERS` y `VITESSE_MODE=mt` para usar el runtime multihilo de tokio en lugar de un hilo por núcleo.

## Por qué es rápido

Casi todo el tiempo de una petición sencilla se pasa en el kernel (leyendo y escribiendo en el socket): un servidor rápido es el que añade lo menos posible alrededor. Vitesse hace exactamente **un `read` y un `write` por petición**, y solo uno de cada para todo un lote de peticiones en pipeline.

- **Un motor HTTP/1.1 propio** (`src/http1.rs`):
  - la cabecera de la petición se analiza con httparse (SIMD) y de las cabeceras solo se anota su posición. El `HeaderMap` y la `Uri` solo se construyen si un handler los pide: una petición de navegador con sus doce cabeceras cuesta casi lo mismo que una petición desnuda;
  - las respuestas se serializan directamente en un búfer de escritura reutilizado: las líneas de estado y los tipos de contenido habituales están precalculados, la cabecera `Date` se guarda en caché por hilo y solo se crea un `HeaderMap` si añades otras cabeceras;
  - un handler que responde sin esperar sigue un camino totalmente síncrono, sin `Future` intermedio ni copias de estructuras grandes;
  - un solo temporizador por conexión (no por petición) gestiona la inactividad.
- **Casi ninguna asignación de memoria**: las peticiones (y sus búferes) se reciclan por hilo, igual que las tablas de cabeceras de las respuestas, y el enrutador escribe los parámetros en búferes reutilizados. Solo queda el `Future` del handler (y el búfer de un cuerpo JSON).
- **Un asignador de memoria más rápido por defecto**: la feature `mimalloc`, activada por defecto, hace que todo el programa asigne memoria con mimalloc. Medido frente al asignador del sistema: +10,5 % de peticiones por segundo y −9,9 % de CPU por petición en `GET /json`, +6,9 % y −10,9 % en `POST /echo`; las rutas que apenas asignan memoria no cambian. Si tu programa define su propio `#[global_allocator]`, desactívala con `default-features = false, features = ["ws"]`.
- **Un hilo por núcleo, y un motor por hilo** (Linux): cada núcleo tiene su propio bucle de eventos y su propio socket `SO_REUSEPORT`; el kernel reparte las conexiones y una petición nunca cambia de hilo. Las conexiones HTTP/1.1 sin cifrar de un núcleo las vigila un epoll propio: una petición recibida entera, cuyo handler responde sin esperar, se lee, se sirve y se escribe sin tarea, sin waker ni temporizador por conexión. Todo lo demás (un cuerpo que aún llega, un handler que espera una base de datos, una respuesta en streaming, WebSocket, HTTP/2) pasa la conexión a una tarea, que sigue donde estaba. Tras una vuelta cargada, sigue buscando eventos durante 15 µs antes de dejar dormir al hilo: despertar a un hilo dormido cuesta más que esos vistazos. Medido frente al motor anterior: hasta +12 % de peticiones por segundo, y hasta un 13 % menos de CPU por petición.
- **Un enrutador sin expresiones regulares en el caso habitual**: un árbol de segmentos que se recorre sin ninguna asignación para las rutas estáticas; los parámetros apuntan dentro de la ruta. Las expresiones regulares solo se ejecutan para los segmentos que las necesitan ([restricciones](routing.md#restricciones-con-expresiones-regulares), texto alrededor de los parámetros), compiladas una sola vez al arrancar.
- **Ningún contador atómico compartido por petición**: la aplicación se congela al arrancar (`&'static`), así que handlers, middlewares y estado se leen sin `Arc`.
- **Pocas copias**: la petición recorre middlewares y handlers moviendo un solo puntero, una respuesta pesa solo 72 bytes y el cuerpo solo se lee si el handler lo pide.

### Y en una aplicación real

Una aplicación real añade middlewares y sirve archivos: cabeceras de seguridad (`helmet`), CORS, un registro de peticiones, compresión para los navegadores, una hoja de estilos. Medido con las peticiones de un navegador (`examples/bench_app.rs`, servidor en 2 núcleos, wrk en otros 2, registro a través de una tubería), frente a la versión anterior de Vitesse:

| Petición | Antes | Ahora |
|---|---:|---:|
| `GET /api/users` (una lista JSON de 50 usuarios, comprimida) | 4.684 req/s (393 µs) | **24.818 req/s (79 µs)**, ×5,3 |
| `GET /static/app.css` (20 KiB, comprimida) | 1.667 req/s (1.127 µs) | **62.697 req/s (31 µs)**, ×37,6 |

Tres cambios marcan la diferencia: una respuesta se comprime a un nivel rápido en su propio hilo ([`compress`](middleware.md#compress)), un archivo estático se guarda en memoria y se comprime una vez al mejor nivel ([archivos estáticos](static-files.md#guardados-en-memoria)), y el registro se escribe por lotes ([`logger`](middleware.md#logger)).

La velocidad no se consigue a costa de la robustez: consulta los [límites integrados en el motor](server.md).

## Mantener rápida tu aplicación

### Compila en modo release

Las compilaciones de depuración son mucho más lentas: nunca las midas ni las despliegues. Usa `cargo build --release` (o `cargo run --release`) y activa en tu `Cargo.toml` las mismas optimizaciones que Vitesse:

```toml
[profile.release]
lto = "fat"         # optimiza entre crates, Vitesse incluido
codegen-units = 1   # compilación más lenta, binario más rápido
```

> [!WARNING]
> No añadas `panic = "abort"`: Vitesse captura los pánicos para convertirlos en respuestas `500`, y con `abort` un solo handler que entre en pánico tumbaría todo el servidor.

### Nunca bloquees el bucle de eventos

Los handlers se ejecutan en pocos hilos. Una llamada bloqueante (un cálculo pesado, el hash de una contraseña, `std::fs`, un driver de base de datos síncrono, `std::thread::sleep`) congela todas las demás conexiones de su hilo mientras dura. Usa API asíncronas (`tokio::fs`, drivers asíncronos) y lleva el trabajo intensivo en CPU al pool de hilos bloqueantes de tokio:

```rust
app.post("/hash", |req: Request| async move {
    let password = req.text().await?;
    // Se ejecuta en el pool de hilos bloqueantes de tokio: el bucle de eventos queda libre.
    let hash = tokio::task::spawn_blocking(move || expensive_hash(&password)).await?;
    Ok::<_, Error>(hash)
});
```

Del mismo modo, nunca mantengas un `std::sync::Mutex` bloqueado a través de un `.await`. Si muchos handlers tienen que bloquear de verdad, consulta [`thread_per_core(false)`](server.md).

### Crea una sola vez los recursos costosos

Los pools de base de datos, los clientes HTTP y las plantillas compiladas deben crearse una sola vez al arrancar y compartirse mediante el [estado](state.md), en lugar de reconstruirse en cada petición:

```rust
struct Services {
    http: reqwest::Client, // mantiene un pool de conexiones que reutilizan todas las peticiones
}

app.state(Services { http: reqwest::Client::new() });
app.get("/weather", |req: Request| async move {
    let services = req.state::<Services>();
    let body = services
        .http
        .get("https://example.com/weather")
        .send()
        .await?
        .text()
        .await?;
    Ok::<_, Error>(body)
});
```

`req.state::<T>()` devuelve una simple referencia, sin bloqueos ni contadores de referencias.

### Lee solo lo que necesitas

El cuerpo solo se lee si llamas a `req.json()`, `req.text()`, `req.bytes()` o `req.form()`: una ruta que no lo necesita no paga nada. Para subidas grandes, `req.take_body()` te da el flujo para procesarlo por partes en lugar de cargarlo entero en memoria. Mantén `body_limit` tan bajo como tu caso de uso lo permita.

### Prefiere los datos estáticos y el JSON tipado

```rust
#[derive(serde::Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

// Se serializa directamente en el búfer de la respuesta: sin árbol intermedio.
app.get("/health", |_| async { Json(Health { status: "ok", version: "1.0" }) });

// Construye un `serde_json::Value` (unas cuantas asignaciones) antes de serializarlo.
app.get("/health-dyn", |_| async { json!({ "status": "ok", "version": "1.0" }) });

// Datos estáticos: nunca se copian.
app.get("/robots.txt", |_| async { "User-agent: *\nDisallow:\n" });
app.get("/pixel", |_| async { Bytes::from_static(b"GIF89a") });
```

`json!` es perfecto para prototipos y rutas poco usadas; en los caminos críticos, una estructura `#[derive(Serialize)]` envuelta en `Json` evita construir un valor intermedio. Un `&'static str` o un `Bytes::from_static` se envían sin ninguna copia (`Bytes` se reexporta como `vitesse::Bytes`).

### Mantén ligeros los middlewares globales

Los middlewares globales se ejecutan en cada petición, incluidos los 404. Haz las comprobaciones costosas en middlewares de ruta o de router, solo donde hacen falta (consulta [Middlewares](middleware.md)). `middleware::logger()` escribe sus líneas por lotes, cada 50 ms, así que cuesta poco; un middleware de registro propio que escribiera cada línea al momento costaría una llamada al sistema por petición.

### Mide

Mide la compilación release, con un generador de carga que no compita con el servidor por los mismos núcleos:

```sh
cargo run --release
wrk -t2 -c128 -d10s http://127.0.0.1:3000/
oha -z 10s -c 128 http://127.0.0.1:3000/
```

wrk y [oha](https://crates.io/crates/oha) muestran el rendimiento y la latencia. Fíjate en los percentiles de latencia además de en las peticiones por segundo, y mide con datos parecidos a los de producción: lo normal es que una aplicación se vea limitada por su base de datos mucho antes que por Vitesse.
