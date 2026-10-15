# PubSub et canaux

`vitesse::pubsub` publie des messages sur des sujets, ou salons, auxquels n'importe quelle partie de l'application peut écrire, et auxquels s'abonnent des connexions WebSocket, des flux Server-Sent Events et des tâches de fond : le `PubSub` et les canaux de Phoenix, les salons de Socket.IO, le broadcasting de Laravel, intégrés.

## Créer un PubSub

Un `PubSub<T>` transporte des messages de type `T` (`String` par défaut ; `serde_json::Value`, `Bytes` ou un enum de vos événements conviennent aussi). Il se clone pour presque rien, et tous ses clones partagent les sujets : mettez-le dans l'[état partagé](state.md) de l'application.

```rust
use vitesse::pubsub::PubSub;

let mut app = App::new();
app.state(PubSub::<String>::new());
```

Un sujet n'est qu'un nom (`"room:rust"`, `"user:42"`, `"news"`). Il existe tant qu'il a des abonnés, et ne coûte plus rien une fois qu'ils sont partis.

## Publier et s'abonner

```rust
let pubsub = PubSub::<String>::new();

let mut subscription = pubsub.subscribe("news");
let reached = pubsub.publish("news", "Vitesse 1.0 est sortie"); // 1 abonné
let message = subscription.recv().await; // Some("Vitesse 1.0 est sortie")
```

- `publish(sujet, message)` renvoie le nombre d'abonnés atteints ; sans abonné, le message est perdu.
- `subscription.recv()` attend le message suivant. Un abonné trop lent pour suivre manque les plus anciens messages, puis continue avec les suivants : 256 messages peuvent attendre chaque abonné (`PubSub::with_capacity(n)` pour changer).
- `subscription.publish(message)` envoie aux **autres** abonnés du sujet, pas à lui-même (le `broadcast_from` de Phoenix).
- `pubsub.subscribers(sujet)` compte les abonnés (qui est dans le salon), et `pubsub.topics()` liste les sujets.
- Abandonner l'abonnement (`drop`) désabonne.

## Des salons de discussion en WebSocket

`pubsub.relay(socket, sujet)` relie un WebSocket à un sujet : chaque message texte envoyé par le socket est publié sur le sujet, et chaque message du sujet est envoyé au socket, les siens compris, jusqu'à sa fermeture. Une discussion complète avec des salons tient en quelques lignes :

```rust
app.ws("/rooms/:room", |req, socket| async move {
    let pubsub = req.state::<PubSub>();
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    pubsub.publish(&room, "quelqu'un arrive");
    pubsub.relay(socket, &room).await;
    pubsub.publish(&room, "quelqu'un part");
});
```

Pour plus de contrôle (messages JSON, authentification, plusieurs sujets par connexion), [séparez le socket](websocket.md) et combinez `socket.recv()` et `subscription.recv()` dans un `tokio::select!`.

## Des mises à jour en direct avec les Server-Sent Events

`subscription.sse(…)` transforme un abonnement en réponse [Server-Sent Events](responses.md), chaque message devenant un événement. L'abonnement prend fin quand le navigateur s'en va :

```rust
use vitesse::sse::Event;

app.get("/rooms/:room/events", |req: Request| async move {
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    req.state::<PubSub>()
        .subscribe(&room)
        .sse(|message| Event::new().event("message").data(message))
});
```

`subscription.into_stream()` donne les messages sous forme de `Stream`, pour tout le reste.

## Publier de n'importe où

Les routes, les tâches de fond, les webhooks et les [tâches planifiées](background.md) publient avec le même `PubSub` :

```rust
app.post("/rooms/:room/announce", |req: Request| async move {
    let room = format!("room:{}", req.param("room").unwrap_or("lobby"));
    let text = req.text().await?;
    let reached = req.state::<PubSub>().publish(&room, text);
    Ok::<_, Error>(format!("envoyé à {reached} abonnés"))
});
```

## Limites

Les sujets et les messages vivent dans la mémoire du processus. Avec plusieurs instances de l'application derrière un répartiteur de charge, chacune a ses propres sujets : relayez les messages entre instances par un courtier (Redis, NATS, le `LISTEN/NOTIFY` de PostgreSQL) qui les publie sur chaque instance.
