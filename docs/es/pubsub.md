# PubSub y canales

`vitesse::pubsub` publica mensajes en temas, o salas, a los que cualquier parte de la aplicación puede escribir, y a los que se suscriben conexiones WebSocket, flujos de Server-Sent Events y tareas en segundo plano: el `PubSub` y los canales de Phoenix, las salas de Socket.IO, el broadcasting de Laravel, integrados.

## Crear un PubSub

Un `PubSub<T>` transporta mensajes de tipo `T` (`String` por defecto; `serde_json::Value`, `Bytes` o un enum de tus eventos también sirven). Clonarlo apenas cuesta, y todos sus clones comparten los temas: ponlo en el [estado compartido](state.md) de la aplicación.

```rust
use vitesse::pubsub::PubSub;

let mut app = App::new();
app.state(PubSub::<String>::new());
```

Un tema no es más que un nombre (`"room:rust"`, `"user:42"`, `"news"`). Existe mientras tenga suscriptores, y no cuesta nada cuando se han ido.

## Publicar y suscribirse

```rust
let pubsub = PubSub::<String>::new();

let mut subscription = pubsub.subscribe("news");
let reached = pubsub.publish("news", "Vitesse 1.0 ya está aquí"); // 1 suscriptor
let message = subscription.recv().await; // Some("Vitesse 1.0 ya está aquí")
```

- `publish(tema, mensaje)` devuelve el número de suscriptores alcanzados; sin ninguno, el mensaje se pierde.
- `subscription.recv()` espera el siguiente mensaje. Un suscriptor demasiado lento pierde los mensajes más antiguos y sigue con los siguientes: 256 mensajes pueden esperar a cada suscriptor (`PubSub::with_capacity(n)` para cambiarlo).
- `subscription.publish(mensaje)` envía a los **demás** suscriptores del tema, no a sí mismo (el `broadcast_from` de Phoenix).
- `pubsub.subscribers(tema)` cuenta los suscriptores (quién está en la sala), y `pubsub.topics()` lista los temas.
- Soltar la suscripción (`drop`) da de baja.

## Salas de chat por WebSocket

`pubsub.relay(socket, tema)` conecta un WebSocket a un tema: cada mensaje de texto que envía el socket se publica en el tema, y cada mensaje del tema se envía al socket, los suyos incluidos, hasta que se cierra. Un chat completo con salas cabe en unas líneas:

```rust
app.ws("/rooms/:room", |req, socket| async move {
    let pubsub = req.state::<PubSub>();
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    pubsub.publish(&room, "alguien ha entrado");
    pubsub.relay(socket, &room).await;
    pubsub.publish(&room, "alguien se ha ido");
});
```

Para más control (mensajes JSON, autenticación, varios temas por conexión), [divide el socket](websocket.md) y combina `socket.recv()` con `subscription.recv()` en un `tokio::select!`.

## Actualizaciones en directo con Server-Sent Events

`subscription.sse(…)` convierte una suscripción en una respuesta de [Server-Sent Events](responses.md), en la que cada mensaje se vuelve un evento. La suscripción termina cuando el navegador se va:

```rust
use vitesse::sse::Event;

app.get("/rooms/:room/events", |req: Request| async move {
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    req.state::<PubSub>()
        .subscribe(&room)
        .sse(|message| Event::new().event("message").data(message))
});
```

`subscription.into_stream()` da los mensajes como un `Stream`, para todo lo demás.

## Publicar desde cualquier parte

Las rutas, las tareas en segundo plano, los webhooks y las [tareas programadas](background.md) publican con el mismo `PubSub`:

```rust
app.post("/rooms/:room/announce", |req: Request| async move {
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    let text = req.text().await?;
    let reached = req.state::<PubSub>().publish(&room, text);
    Ok::<_, Error>(format!("enviado a {reached} suscriptores"))
});
```

## Límites

Los temas y los mensajes viven en la memoria del proceso. Con varias instancias de la aplicación detrás de un balanceador de carga, cada una tiene sus propios temas: retransmite los mensajes entre instancias con un broker (Redis, NATS, el `LISTEN/NOTIFY` de PostgreSQL) que los publique en cada instancia.
