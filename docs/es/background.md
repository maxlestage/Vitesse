# Tareas en segundo plano y programadas

Algunos trabajos no pertenecen a la petición: enviar un correo, redimensionar una imagen, llamar a una API lenta, purgar las sesiones viejas cada noche. Vitesse tiene una cola de tareas, `vitesse::queue` (las colas de Laravel, BullMQ, Sidekiq), y un planificador, `vitesse::schedule` (el scheduler de Laravel, cron), ambos en memoria y ambos opcionales.

## La cola de tareas

Una `Queue` ejecuta tareas en segundo plano, con un tope de tareas simultáneas, un límite de ritmo opcional y reintentos:

```rust
use std::time::Duration;
use vitesse::queue::Queue;

let queue = Queue::builder()
    .workers(4) // 4 tareas a la vez como máximo (por defecto)
    .rate_limit(10, Duration::from_secs(1)) // 10 arranques por segundo como máximo
    .retries(3) // una tarea fallida se reintenta hasta 3 veces…
    .backoff(Duration::from_secs(1)) // …tras 1 s, luego 2 s, luego 4 s
    .start();

let mut app = App::new();
app.state(queue);
```

`Queue::new()` arranca una cola con los ajustes por defecto: 4 workers, sin reintentos ni límite de ritmo.

## Enviar tareas

Una tarea es una función que devuelve un future, a la que se vuelve a llamar en cada reintento. No devuelve nada, o un `Result` cuyo error se registra y se reintenta:

```rust
async fn send_welcome_email(to: String) -> Result<(), std::io::Error> {
    // … hablar con el servidor de correo
    Ok(())
}

app.post("/signup", |req: Request| async move {
    let email = req.text().await?;
    req.state::<Queue>().dispatch(move || send_welcome_email(email.clone()));
    Ok::<_, Error>((202, "¡Bienvenido! Revisa tu bandeja de entrada."))
});
```

- `dispatch_after(retraso, tarea)` arranca la tarea tras un retraso como pronto.
- Una tarea que entra en pánico cuenta como fallida: se reintenta como un error.
- `queue.pending()` cuenta las tareas en espera o en curso, y `queue.idle().await` espera a que no quede ninguna, útil en las pruebas antes de comprobar lo que hicieron.

## Tareas programadas

Un `Scheduler` ejecuta funciones a intervalos regulares, en segundo plano del servidor:

```rust
use std::time::Duration;
use vitesse::schedule::Scheduler;

fn main() -> std::io::Result<()> {
    let scheduler = Scheduler::new()
        .every(Duration::from_secs(30), "heartbeat", || async { println!("vivo") })
        .every_minute("refresh rates", refresh_exchange_rates)
        .hourly_at(5, "purge sessions", purge_sessions) // a las hh:05 cada hora
        .daily_at(3, 30, "nightly report", send_report) // a las 03:30 UTC cada día
        .start();

    let mut app = App::new();
    app.get("/", |_| async { "¡Hola!" });
    app.run(3000)?;
    scheduler.stop();
    Ok(())
}
```

- Como las tareas de la cola, una tarea programada no devuelve nada o un `Result`; los errores y los pánicos se registran, y la tarea vuelve a ejecutarse la siguiente vez.
- Una tarea que sigue en curso cuando llega su siguiente ejecución no se lanza dos veces (el `withoutOverlapping` de Laravel).
- Las horas están en UTC. `every(intervalo, …)` se ejecuta por primera vez un intervalo después del arranque.
- `scheduler.stop()` detiene las tareas; soltar el manejador las deja funcionando.

## Dónde se ejecutan

La cola y el planificador funcionan en el runtime de tokio actual cuando lo hay (`#[tokio::main]`, `app.listen`, las pruebas), y si no en un hilo propio con su propio runtime: créalos antes de `app.run(…)` y funcionan junto al servidor.

## Límites

Las tareas y las programaciones viven en la memoria del proceso: las tareas aún en espera se pierden si se detiene, y cada instancia de la aplicación tiene su propio planificador. Para tareas que deben sobrevivir a un reinicio, usa una cola respaldada por una base de datos o un broker; con varias instancias, ejecuta el planificador en una sola.
