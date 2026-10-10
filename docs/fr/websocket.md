# WebSocket

Un WebSocket garde une connexion ouverte dans les deux sens : le serveur peut envoyer des messages au client à tout moment. C'est ce qu'il faut pour un chat, des notifications, un tableau de bord en direct ou un jeu multijoueur. Vitesse le prend en charge d'emblée, dans le style d'`express-ws` : `app.ws(chemin, handler)`, puis une boucle qui lit et écrit les messages.

## Mise en place

La prise en charge de WebSocket est la feature Cargo `ws`, **activée par défaut** : `vitesse = "0.1"` suffit. Elle repose sur [tokio-tungstenite](https://github.com/snapview/tokio-tungstenite), l'implémentation Rust validée par la suite de tests Autobahn. La poignée de main (RFC 6455) et la remise de la connexion TCP après la réponse `101 Switching Protocols` sont assurées par le moteur HTTP/1.1 de Vitesse lui-même.

Si vous n'avez pas besoin de WebSocket, désactivez les features par défaut pour compiler un peu moins :

```toml
[dependencies]
vitesse = { version = "0.1", default-features = false }
```

## Une première route WebSocket

Un serveur d'écho, qui renvoie chaque message texte :

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.ws("/echo", |_req, mut socket| async move {
        while let Some(Ok(message)) = socket.recv().await {
            if let ws::Message::Text(text) = message {
                if socket.send(format!("écho : {text}")).await.is_err() {
                    break; // le client est parti
                }
            }
        }
        // La boucle se termine quand le client ferme la connexion.
    });

    app.run(3000)
}
```

Le handler reçoit la requête et le socket ouvert, et la connexion vit aussi longtemps que le handler s'exécute. `ws` fait partie du prélude, d'où `ws::Message`. Pour l'essayer, utilisez [websocat](https://github.com/vi/websocat), un client WebSocket en ligne de commande écrit en Rust (`cargo install websocat`) :

```sh
websocat ws://localhost:3000/echo
```

Tapez une ligne, et le serveur répond `écho : …`.

### Ce que fait `app.ws`

`app.ws(chemin, handler)` déclare une route `GET`, avec les mêmes motifs que n'importe quelle route (`/rooms/:room`, `/files/*path`…). Les requêtes qui ne sont pas une poignée de main WebSocket valide reçoivent une erreur, qui passe par [`app.on_error`](errors.md) comme les autres :

| Requête | Réponse |
|---|---|
| Une poignée de main WebSocket (`GET` avec `Upgrade: websocket`, `Sec-WebSocket-Version: 13` et une clé valide) | `101 Switching Protocols`, puis le handler prend la main sur la connexion |
| Une requête HTTP ordinaire (un navigateur qui ouvre l'URL, `curl`) | `426 Upgrade Required` : « this route only accepts WebSocket connections » |
| Une autre version du protocole (`Sec-WebSocket-Version` différent de `13`) | `426 Upgrade Required` |
| Une `Sec-WebSocket-Key` invalide | `400 Bad Request` |
| Une autre méthode, `HEAD` compris | `405 Method Not Allowed` |

`ws` existe aussi sur `Router` : une route WebSocket peut donc vivre dans un routeur, avec ses middlewares et son préfixe (voir [Middlewares et authentification](#middlewares-et-authentification)).

## Lire et écrire des messages

| Méthode de `WebSocket` | Effet |
|---|---|
| `socket.recv().await` | Le message suivant : `Some(Ok(message))`, `Some(Err(erreur))`, ou `None` une fois la connexion fermée |
| `socket.send(valeur).await` | Envoie un message : une `String` ou un `&str` en texte, un `Vec<u8>`, des `Bytes` ou un `&[u8]` en binaire, ou un `ws::Message` |
| `socket.close(code, raison).await` | Lance la fermeture (`1000` : fermeture normale) |
| `socket.protocol()` | Le sous-protocole choisi avec [`Upgrade::protocols`](#plus-de-contrôle--wsupgrade), s'il y en a un |
| `socket.split()` | Deux moitiés, pour envoyer depuis une tâche pendant qu'une autre reçoit (voir [plus bas](#envoyer-et-recevoir-en-même-temps--split)) |

Un message est un `ws::Message` :

| Variante | Contenu |
|---|---|
| `Message::Text(String)` | Un message texte (UTF-8 valide) |
| `Message::Binary(Bytes)` | Un message binaire |
| `Message::Ping(Bytes)`, `Message::Pong(Bytes)` | Des messages de contrôle : les pings reçoivent une réponse automatique |
| `Message::Close(Option<CloseFrame>)` | L'autre côté ferme la connexion ; `CloseFrame { code: u16, reason: String }` |

Trois raccourcis évitent un `match` : `message.as_text()` (un `Option<&str>`), `message.as_bytes()` (le contenu d'un message texte ou binaire) et `message.is_close()`.

`recv` renvoie `Some(Err(…))` quand la connexion est coupée, quand le client enfreint le protocole ou envoie un message trop gros ; il n'y a alors plus rien à lire, si bien que `while let Some(Ok(message))` s'arrête simplement à la première erreur. `send` échoue une fois la connexion fermée : `erreur.is_closed()` vous l'indique.

### Des messages JSON

Les messages sont souvent du JSON. Analysez-les avec serde et répondez avec `json!` :

```rust
use serde::Deserialize;
use vitesse::prelude::*;

#[derive(Deserialize)]
struct Move {
    x: i32,
    y: i32,
}

let mut app = App::new();
app.ws("/game", |_req, mut socket| async move {
    while let Some(Ok(message)) = socket.recv().await {
        let Some(text) = message.as_text() else {
            continue; // binaire, ping, pong...
        };
        let reply = match vitesse::serde_json::from_str::<Move>(text) {
            Ok(m) => json!({ "ok": true, "x": m.x, "y": m.y }),
            Err(e) => json!({ "ok": false, "error": e.to_string() }),
        };
        if socket.send(reply.to_string()).await.is_err() {
            break;
        }
    }
});
```

## Paramètres, en-têtes et état

Le handler reçoit d'abord la requête, et la garde pendant toute la connexion : paramètres de route, query string, en-têtes, cookies et [état partagé](state.md) se lisent comme dans n'importe quel handler.

```rust
app.ws("/rooms/:room", |req, mut socket| async move {
    let room = req.param("room").unwrap_or("accueil").to_owned();
    let nickname = req.query("nickname").unwrap_or("anonyme".into()).into_owned();

    let _ = socket.send(format!("{nickname} a rejoint {room}")).await;
    while let Some(Ok(message)) = socket.recv().await {
        if message.is_close() {
            break;
        }
    }
});
```

> [!NOTE]
> Les navigateurs ne peuvent pas ajouter d'en-têtes comme `Authorization` à une connexion WebSocket. En revanche, ils envoient les cookies du site avec la poignée de main : authentifiez avec un cookie de session, ou avec un jeton de courte durée dans la query string (`ws://…/live?token=…`).

## Middlewares et authentification

Les middlewares globaux et ceux des routeurs s'exécutent sur la requête de poignée de main, comme sur n'importe quelle requête : journalisation, authentification, limitation de débit… Un middleware qui répond `401` refuse la connexion avant qu'elle ne s'ouvre, et les en-têtes qu'un middleware ajoute figurent dans la réponse `101`.

```rust
async fn require_session(req: Request, next: Next) -> Response {
    match req.cookie("session") {
        Some(_) => next.run(req).await,
        None => Error::unauthorized("veuillez vous connecter").into_response(),
    }
}

let mut live = Router::new();
live.middleware(require_session);
live.ws("/notifications", |_req, mut socket| async move {
    let _ = socket.send("vous êtes connecté").await;
});

let mut app = App::new();
app.mount("/live", live); // ws://example.com/live/notifications
```

Un middleware voit la poignée de main, pas la conversation : `middleware::logger()` écrit une ligne, `GET /live/notifications 101`, dès que la connexion est acceptée, et `middleware::timeout(…)` ne limite que le temps de réponse à la poignée de main, jamais la durée de vie de la connexion.

## Plus de contrôle : `ws::Upgrade`

Pour des sous-protocoles, une limite de taille ou une vérification avant d'accepter la connexion, utilisez `vitesse::ws::Upgrade` dans une route `GET` ordinaire :

```rust
use vitesse::prelude::*;
use vitesse::ws::Upgrade;

let mut app = App::new();
app.get("/chat", |req: Request| async move {
    // Refuse les connexions ouvertes par les pages d'autres sites.
    if req.header("origin") != Some("https://chat.example.com") {
        return Err(Error::forbidden("origine inconnue"));
    }
    let upgrade = Upgrade::new(&req)? // 426, 400 ou 405 si ce n'est pas une poignée de main
        .protocols(["chat.v2", "chat.v1"])
        .max_message_size(64 * 1024); // 64 Kio au lieu de 16 Mio
    Ok(upgrade.on_upgrade(req, |_req, mut socket| async move {
        let protocol = socket.protocol().unwrap_or("aucun").to_owned();
        let _ = socket.send(format!("bienvenue, protocole : {protocol}")).await;
        // ... la conversation, comme avec app.ws
    }))
});
```

| Usage | Effet |
|---|---|
| `Upgrade::new(&req)?` | Vérifie que la requête est une poignée de main WebSocket ; sinon `426`, `400` ou `405`, comme avec `app.ws` |
| `.offered_protocols()` | Les sous-protocoles proposés par le client (`Sec-WebSocket-Protocol`), en `&[String]` |
| `.protocols([...])` | Choisit le premier sous-protocole proposé par le client que le serveur prend en charge ; `socket.protocol()` le renvoie |
| `.max_message_size(octets)` | Taille maximale d'un message reçu, et d'une trame (par défaut `ws::DEFAULT_MAX_MESSAGE_SIZE`, 16 Mio) ; au-delà, la connexion est fermée |
| `.on_upgrade(req, handler)` | Renvoie la réponse `101 Switching Protocols` ; `handler(req, socket)` prend ensuite la main sur la connexion |

Quand aucun sous-protocole ne correspond, la connexion est acceptée sans sous-protocole. Pour la refuser à la place, examinez `offered_protocols()` et renvoyez une erreur avant `on_upgrade`.

> [!IMPORTANT]
> Vérifiez l'en-tête `Origin` avant d'accepter une connexion si vous authentifiez par cookie. Les navigateurs envoient les cookies avec la poignée de main, mais la politique de même origine et CORS ne s'appliquent pas à WebSocket : sans cette vérification, une page de n'importe quel autre site pourrait ouvrir une connexion au nom de vos utilisateurs connectés (*cross-site WebSocket hijacking*). Les clients hors navigateur (applications mobiles, `websocat`, autres serveurs) n'envoient en général pas d'`Origin`.

## Envoyer et recevoir en même temps : `split`

`recv` et `send` empruntent tous deux le socket : une seule boucle ne peut donc pas attendre le client tout en envoyant ses propres messages. `socket.split()` renvoie deux moitiés : un `WebSocketSender` (`send`, `close`) et un `WebSocketReceiver` (`recv`), qui peuvent vivre dans deux tâches.

```rust
use std::time::Duration;

app.ws("/clock", |_req, socket| async move {
    let (mut tx, mut rx) = socket.split();

    // Une tâche envoie un message chaque seconde...
    let ticker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        for n in 1.. {
            interval.tick().await;
            if tx.send(format!("tic {n}")).await.is_err() {
                break; // le client est parti
            }
        }
    });

    // ...pendant que celle-ci lit jusqu'au départ du client.
    while let Some(Ok(message)) = rx.recv().await {
        if message.is_close() {
            break;
        }
    }
    ticker.abort();
});
```

Cet exemple utilise tokio directement : ajoutez-le à vos dépendances, ou passez par la réexportation, `vitesse::tokio::spawn`.

### `Stream` et `Sink`

`WebSocket` implémente `futures::Stream<Item = Result<Message, ws::Error>>` et `Sink<Message>`, et `WebSocketReceiver` implémente `Stream`. Tous les combinateurs de [futures-util](https://docs.rs/futures-util) (`StreamExt`, `SinkExt`) fonctionnent donc, par exemple pour envoyer tout un flux de messages :

```rust
use futures_util::{StreamExt, stream};

app.ws("/countdown", |_req, socket| async move {
    let messages = ["3", "2", "1", "décollage !"].map(|text| Ok(ws::Message::from(text)));
    // Envoie chaque message, puis ferme la connexion.
    let _ = stream::iter(messages).forward(socket).await;
});
```

## Un salon de discussion

L'exemple [`examples/chat.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/chat.rs) du dépôt est un salon de discussion complet : chaque message envoyé par un client est diffusé à tous les autres. Voici sa route WebSocket :

```rust
use tokio::sync::broadcast;
use vitesse::prelude::*;
use vitesse::ws::Message;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    // Chaque client connecté écoute ce canal.
    let (room, _) = broadcast::channel::<String>(256);
    app.state(room);

    app.ws("/chat/:name", |req, socket| async move {
        let name = req.param("name").unwrap_or("anonyme").to_owned();
        let room = req.state::<broadcast::Sender<String>>();
        let mut inbox = room.subscribe();
        let (mut tx, mut rx) = socket.split();

        let _ = room.send(format!("* {name} est arrivé"));
        // Transmet les messages du salon à ce client...
        let forward = tokio::spawn(async move {
            while let Ok(line) = inbox.recv().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
        // ...et diffuse ce que dit ce client.
        while let Some(Ok(message)) = rx.recv().await {
            match message {
                Message::Text(text) => {
                    let _ = room.send(format!("{name} : {}", text.trim_end()));
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        forward.abort();
        let _ = room.send(format!("* {name} est parti"));
    });

    app.run(3000)
}
```

Le principe :

- un canal [`broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) de tokio sert de salon : il est enregistré comme [état](state.md), et chaque message qu'on y envoie parvient à tous ses abonnés ;
- chaque connexion s'abonne (`room.subscribe()`) et sépare son socket en deux : une tâche transmet au client les messages du salon, pendant que la boucle du handler diffuse ce que dit le client ;
- quand le client s'en va, la boucle se termine : le handler arrête la tâche de transmission et annonce le départ.

Essayez-le depuis un clone du dépôt, avec un terminal par participant :

```sh
cargo run --release --example chat

# Dans deux autres terminaux :
websocat ws://localhost:3000/chat/ada
websocat ws://localhost:3000/chat/grace
```

> [!TIP]
> Un canal `broadcast` garde les 256 derniers messages pour les abonnés lents. Un client qui prend plus de retard reçoit une erreur `Lagged`, qui met fin à sa boucle de transmission dans cet exemple. Dans une vraie application, traitez `RecvError::Lagged` (en sautant les messages perdus, ou en déconnectant le client) pour qu'un client lent ne cesse jamais de recevoir sans le savoir.

## Tester une route WebSocket

[`TestClient`](testing.md) n'a pas de vrai socket : pour une route WebSocket, il ne voit que la réponse à la poignée de main. Cela suffit pour tester les refus (`426` pour du HTTP ordinaire, `401` venant de votre middleware, `403` pour une origine inconnue). Pour tester une conversation, démarrez le serveur sur un vrai port et connectez-vous avec le client de tokio-tungstenite :

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
tokio-tungstenite = { version = "0.30", features = ["connect"] }
futures-util = "0.3"
```

```rust
// tests/ws.rs
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn echoes_text_messages() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let (mut socket, response) = connect_async(format!("ws://{addr}/echo")).await.unwrap();
    assert_eq!(response.status(), 101);

    socket.send(Message::text("salut")).await.unwrap();
    let reply = socket.next().await.unwrap().unwrap();
    assert_eq!(reply, Message::text("écho : salut"));

    socket.close(None).await.unwrap();
}
```

D'autres exemples dans [Tests](testing.md#tester-les-websockets).

## Derrière un reverse proxy

Le proxy doit laisser passer l'`Upgrade`, puis garder la connexion ouverte :

- **Nginx** a besoin qu'on le lui dise, dans la `location` qui sert vos routes WebSocket :

  ```nginx
  location /chat/ {
      proxy_pass http://127.0.0.1:3000;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
      proxy_read_timeout 1h;   # par défaut, Nginx ferme une connexion silencieuse au bout de 60 s
  }
  ```

- **Caddy** gère WebSocket automatiquement avec `reverse_proxy` : rien à ajouter.
- **Heroku** : son routeur prend en charge WebSocket (voir [Heroku](heroku-mobile.md#autres-surprises)).

Les proxys et les hébergeurs ferment les connexions qui restent trop longtemps silencieuses. Pour une connexion qui peut rester inactive (des notifications), envoyez un `Message::Ping` depuis le serveur toutes les 30 secondes environ : les clients, navigateurs compris, répondent automatiquement par un pong. La configuration Nginx complète se trouve dans [Mise en production](production.md#websocket-derrière-nginx).

## Limites

- **HTTP/1.1 uniquement.** WebSocket n'est pas disponible en [HTTP/3](http3.md) : les clients ouvrent leurs connexions WebSocket sur une connexion TCP ordinaire, même quand le reste du site passe en HTTP/3.
- **Arrêt propre.** Sur `Ctrl+C` ou `SIGTERM`, le serveur n'attend pas les connexions WebSocket ouvertes : elles sont coupées dès que les requêtes HTTP en cours sont terminées, et au plus tard à la fin du délai de grâce de 10 secondes (voir [Configuration du serveur](server.md#arrêt-propre)). Elles sont coupées sans fermeture en bonne et due forme, et les clients constatent une fermeture anormale (code `1006`). Faites en sorte que vos clients se reconnectent automatiquement, ce qu'il leur faut de toute façon pour les coupures réseau.
- **Taille des messages.** 16 Mio par message par défaut ; abaissez-la avec `Upgrade::max_message_size` pour les points d'accès publics.
- **Threads.** En mode un thread par cœur, une connexion reste sur le thread qui l'a acceptée : comme dans tout handler, ne le bloquez pas (voir [Performances](performance.md)).

## Venir d'Express

Avec [express-ws](https://github.com/HenningM/express-ws) :

```js
const expressWs = require('express-ws');
expressWs(app);

app.ws('/echo', (ws, req) => {
  ws.on('message', (msg) => ws.send(msg));
  ws.on('close', () => console.log('au revoir'));
});
```

Avec Vitesse :

```rust
app.ws("/echo", |_req, mut socket| async move {
    while let Some(Ok(message)) = socket.recv().await {
        match message {
            ws::Message::Text(_) | ws::Message::Binary(_) => {
                if socket.send(message).await.is_err() {
                    break;
                }
            }
            _ => {}
        }
    }
    println!("au revoir");
});
```

| express-ws | Vitesse |
|---|---|
| `app.ws('/echo', (ws, req) => …)` | `app.ws("/echo", \|req, socket\| async move { … })` : la requête vient **en premier** |
| `ws.on('message', (msg) => …)` | `while let Some(Ok(msg)) = socket.recv().await { … }` |
| `ws.on('close', …)` | Le code après la boucle |
| `ws.send(data)` | `socket.send(data).await` |
| `ws.close(1000, 'bye')` | `socket.close(1000, "bye").await` |
| `req.params.room` | `req.param("room")` |
| `router.ws(…)` | `router.ws(…)` |
| `wss.clients.forEach(…)` (diffusion) | Un canal `broadcast` dans l'état (voir [Un salon de discussion](#un-salon-de-discussion)) |

Plutôt que des callbacks, un WebSocket est une boucle : chaque connexion tourne dans sa propre tâche, et `.await` attend le message suivant sans rien bloquer.
