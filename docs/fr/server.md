# Configuration du serveur

Cette page couvre tout ce qui touche au serveur lui-même : les trois façons de le démarrer, les adresses d'écoute, les threads, la taille des corps, les limites intégrées au moteur HTTP, l'arrêt propre et les protocoles qu'il parle. Les valeurs par défaut sont pensées pour la production : la plupart des applications n'ont besoin que de `app.run(port)`.

## Trois façons de démarrer le serveur

| | `app.run(addr)` | `app.listen(addr).await` | `app.bind(addr).await?` + `Server` |
|---|---|---|---|
| Nécessite `#[tokio::main]` | Non | Oui | Oui |
| Threads | Un par cœur, voir `workers` et `thread_per_core` | Ceux de votre runtime | Ceux de votre runtime |
| S'arrête proprement sur `Ctrl+C` / `SIGTERM` | Oui | Oui | Oui avec `run()`, ou sur votre propre signal avec `with_graceful_shutdown` |
| Connaît le port avant de servir | Non | Non | Oui, avec `local_addr()` |
| Usage typique | La plupart des applications, la production | Une application tokio existante | Tests, arrêt personnalisé, port `0` |

### `app.run` : le choix par défaut

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Hello World!" });

    app.run(3000) // bloque jusqu'à Ctrl+C / SIGTERM
}
```

`run` crée son propre runtime tokio, démarre un thread par cœur et bloque jusqu'à l'arrêt du serveur. Elle renvoie un `io::Result<()>` : si le port est déjà pris, vous obtenez l'erreur immédiatement. C'est aussi le mode le plus rapide (voir [Performances](performance.md)).

### `app.listen` : dans votre propre runtime

Si vous avez déjà un runtime tokio, par exemple pour une initialisation asynchrone avant le démarrage, utilisez `listen` :

```rust
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Initialisation asynchrone avant le démarrage (base de données, config…).
    let greeting = tokio::fs::read_to_string("greeting.txt")
        .await
        .unwrap_or_else(|_| "Hello World!".to_string());

    let mut app = App::new();
    app.state(greeting);
    app.get("/", |req: Request| async move { req.state::<String>().clone() });

    app.listen(3000).await // jusqu'à Ctrl+C / SIGTERM, sur le runtime courant
}
```

Il faut alors tokio dans votre `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`).

> [!NOTE]
> Comme `run`, `listen` s'arrête proprement sur `Ctrl+C` ou `SIGTERM` et laisse les requêtes en cours se terminer (voir [Arrêt propre](#arrêt-propre)). En revanche, `app.workers` et `app.thread_per_core` n'ont pas d'effet : les threads sont ceux de votre runtime.

### `app.bind` et `Server` : le contrôle total

`bind` ouvre le port sans encore servir, et renvoie un `Server` :

```rust
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });

    let server = app.bind("127.0.0.1:0").await?; // port 0 : l'OS choisit un port libre
    println!("Écoute sur http://{}", server.local_addr());

    server.run().await // jusqu'à Ctrl+C / SIGTERM, comme app.run
}
```

| Méthode de `Server` | Rôle |
|---|---|
| `server.local_addr()` | L'adresse réellement utilisée (pratique avec le port `0`) |
| `server.http3_addr()` | L'adresse UDP de [HTTP/3](http3.md), si `app.http3` est configuré (feature `http3`) |
| `server.run().await` | Sert jusqu'à `Ctrl+C` / `SIGTERM`, puis s'arrête proprement |
| `server.with_graceful_shutdown(signal).await` | Sert jusqu'à ce que le futur `signal` se termine, puis s'arrête proprement |

`with_graceful_shutdown` remplace `Ctrl+C` / `SIGTERM` par le futur de votre choix : un canal dans les [tests](testing.md), ou les signaux habituels suivis de votre propre code, comme dans [Mise en production](production.md#arrêt-propre).

C'est le mode utilisé pour les [tests d'intégration](testing.md) sur un vrai port.

## Adresses d'écoute

Les trois méthodes acceptent tout ce qui implémente le trait `ListenAddr` :

| Valeur | Écoute sur |
|---|---|
| `3000` (tout entier) | `0.0.0.0:3000` : toutes les interfaces IPv4 |
| `"3000"` | Idem |
| `"127.0.0.1:8080"` | Cette machine uniquement |
| `"[::]:3000"` | Toutes les interfaces IPv6 |
| `"localhost:3000"` | Le nom est résolu ; la première adresse qui fonctionne est utilisée |
| `String`, `&String` | Comme le `&str` équivalent |
| `SocketAddr` | Exactement cette adresse |
| `([127, 0, 0, 1], 8080)`, `(Ipv4Addr::LOCALHOST, 8080)` | Une paire `(IP, port)` |

Un simple port écoute sur toutes les interfaces, comme `app.listen(3000)` en Express : c'est ce qu'il faut dans un conteneur ou sur un serveur. Utilisez `127.0.0.1` pour n'accepter que les connexions venant de la machine elle-même. Un port hors de `0..=65535` renvoie une erreur au démarrage.

> [!TIP]
> Préférez une IP explicite à `localhost` : selon le système, `localhost` peut se résoudre d'abord en `::1` (IPv6), et le serveur ne répondrait alors pas sur `127.0.0.1`.

### Lire le port dans l'environnement

Les hébergeurs (Heroku et d'autres) fournissent le port dans la variable `PORT`. Comme une `String` contenant un simple port est une adresse valide, il suffit de :

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "Bonjour !" });

    // PORT=8080 → 0.0.0.0:8080 ; sans PORT → 0.0.0.0:3000
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    app.run(port)
}
```

Voir [Heroku](heroku-mobile.md) et [Docker](docker.md) pour des déploiements complets.

## Threads : `workers` et `thread_per_core`

Par défaut, `app.run` démarre un thread par processeur disponible pour le processus. La répartition du travail dépend du mode :

- **Un thread par cœur** (le défaut sous Linux) : chaque thread a sa propre boucle d'événements et son propre socket d'écoute (`SO_REUSEPORT`). Le noyau répartit les nouvelles connexions entre les threads, et une connexion ne change jamais de thread, sans aucune synchronisation entre les cœurs. C'est le mode le plus rapide.
- **Runtime multi-thread** (`thread_per_core(false)`, et toujours sur les systèmes autres que Linux, comme macOS et Windows) : un seul socket et le runtime à vol de tâches (*work stealing*) de tokio, avec `workers` threads. Les threads inactifs reprennent le travail en attente des threads occupés.

```rust
let mut app = App::new();
app.workers(4)              // 4 threads au lieu d'un par cœur
    .thread_per_core(false) // runtime multi-thread de tokio, avec vol de tâches
    .body_limit(10 * 1024 * 1024); // accepte des corps jusqu'à 10 Mio
```

Quand modifier ces réglages :

- **`workers(n)`** quand le serveur partage la machine avec d'autres services (une base de données, un processus de tâches de fond), quand vous voulez laisser des cœurs libres, ou quand le nombre de processeurs détecté dans un conteneur ne correspond pas au quota qu'il reçoit vraiment. `n` vaut au moins `1`.
- **`thread_per_core(false)`** quand des handlers font de longs calculs bloquants (en mode un thread par cœur, ils bloquent toutes les connexions de leur thread), ou quand les connexions sont peu nombreuses et très inégales : par exemple un reverse proxy qui garde une poignée de connexions keep-alive ouvertes, qui pourraient toutes tomber sur le même thread.
- Sinon, gardez les valeurs par défaut.

Ces deux réglages ne s'appliquent qu'à `app.run`. Pour les régler sans recompiler, lisez-les dans l'environnement :

```rust
let mut app = App::new();
if let Some(n) = std::env::var("WORKERS").ok().and_then(|w| w.parse().ok()) {
    app.workers(n);
}
```

> [!TIP]
> Avant de changer de mode à cause de handlers lents, déplacez le travail bloquant dans `tokio::task::spawn_blocking` : voir [Performances](performance.md).

## Taille des corps de requête : `body_limit`

```rust
app.body_limit(10 * 1024 * 1024); // 10 Mio
```

La valeur par défaut est `vitesse::DEFAULT_BODY_LIMIT`, soit 1 Mio. La limite s'applique dès qu'un handler lit le corps avec `req.bytes()`, `req.text()`, `req.json()` ou `req.form()` : au-delà, la requête échoue avec `413 Payload Too Large`. Si le `Content-Length` annoncé est déjà trop grand, la 413 est renvoyée sans rien lire.

Le corps n'est lu que si le handler le demande : les routes qui ne le lisent pas ne sont pas concernées. `req.take_body()`, qui vous donne le flux brut (upload, proxy), n'est pas limité : compter les octets est alors de votre ressort. Le réglage vaut pour toute l'application. Voir [Requêtes](requests.md) pour la lecture des corps.

## Limites intégrées au moteur

Le moteur HTTP/1.1 protège le serveur contre les requêtes malformées ou abusives. Ces valeurs sont fixes :

| Situation | Comportement |
|---|---|
| Tête de requête (ligne de requête et en-têtes) de plus de 60 Kio | `431 Request Header Fields Too Large`, connexion fermée |
| Plus de 64 en-têtes | `431`, connexion fermée |
| Requête malformée | `400 Bad Request`, connexion fermée |
| `Content-Length` et `Transfer-Encoding` à la fois, ou deux `Content-Length` différents | `400` (protection contre la contrebande de requêtes) |
| `Transfer-Encoding` dont le dernier codage n'est pas `chunked` | `501 Not Implemented` |
| Corps au-delà de `body_limit`, à la lecture | `413 Payload Too Large` |
| Connexion keep-alive sans requête pendant environ 60 s | Connexion fermée |
| Tête de requête toujours incomplète après environ 30 s | `408 Request Timeout`, connexion fermée |
| `Expect: 100-continue` | `100 Continue` envoyé automatiquement avant la lecture du corps |
| Corps de requête `chunked` | Décodé de façon transparente |
| Corps jusqu'à 64 Kio | Lu avant l'appel du handler ; les corps plus gros sont transmis au handler en flux, au fil de leur arrivée |
| Gros corps que le handler ne lit pas | Réponse envoyée avec `connection: close`, puis la fin de l'envoi est absorbée (2 s et 8 Mio au plus) pour que le client reçoive bien la réponse |
| Requêtes pipelinées | Traitées dans l'ordre ; les réponses d'un lot partent en un seul appel système |
| HTTP/1.0 | Connexion fermée après la réponse, sauf si le client demande le keep-alive |
| Panique dans un handler ou un middleware | `500`, et le serveur continue de tourner |

Côté réponse, le moteur calcule `content-length`, ajoute l'en-tête `date`, envoie les corps de taille inconnue (flux) en `chunked`, n'envoie pas de corps pour les `204`, les `304` et les requêtes `HEAD`, et ferme la connexion après la réponse si votre handler pose un en-tête `connection: close`.

> [!IMPORTANT]
> Il n'y a pas de limite de temps sur le handler lui-même : un handler qui attend indéfiniment garde sa requête ouverte indéfiniment. Ajoutez le middleware `timeout`, qui répond `503 Service Unavailable` au bout de la durée choisie.

```rust
app.middleware(middleware::timeout(Duration::from_secs(30))); // std::time::Duration
```

## Arrêt propre

Avec `app.run`, `app.listen` et `Server::run`, `Ctrl+C` (SIGINT) et, sous Unix, `SIGTERM` déclenchent un arrêt propre :

1. le serveur n'accepte plus de nouvelles connexions ;
2. les requêtes en cours ont jusqu'à **10 secondes** pour se terminer, et leurs réponses portent `connection: close` ;
3. les connexions restantes (keep-alive inactives, requêtes encore en cours après 10 s) sont fermées, et `run` (ou `listen`) renvoie `Ok(())`.

Avec [HTTP/3](http3.md), le côté UDP s'arrête en même temps : les connexions ouvertes reçoivent un `GOAWAY`, et leurs requêtes en cours disposent des mêmes 10 secondes. Les connexions [WebSocket](websocket.md) ne sont pas attendues : elles sont fermées au plus tard à la fin du délai de grâce, et les clients doivent se reconnecter.

C'est exactement ce qu'attendent Docker, Kubernetes ou Heroku : ils envoient `SIGTERM` et patientent un moment avant de forcer l'arrêt du processus. Avec `bind`, `with_graceful_shutdown(signal)` suit les mêmes étapes dès que votre futur `signal` se termine : c'est vous qui décidez ce qui déclenche l'arrêt. `Ctrl+C` et `SIGTERM` ne sont alors plus surveillés : incluez-les dans votre futur si vous en avez encore besoin. Le délai de grâce de 10 secondes n'est pas configurable.

## Protocoles : HTTP/1.1, WebSocket et HTTP/3

Vitesse parle HTTP/1.1 (et HTTP/1.0) en TCP simple, et fait passer les connexions en [WebSocket](websocket.md) (feature `ws`, activée par défaut). Avec la feature `http3`, `app.http3(...)` sert aussi l'application en [HTTP/3](http3.md), sur un port UDP à côté du port TCP ; `server.http3_addr()` renvoie son adresse.

Vitesse ne fait ni TLS sur TCP, ni HTTP/2. Pour HTTPS et HTTP/2, placez-le derrière un reverse proxy (Nginx, Caddy, ou le load balancer de votre hébergeur) qui termine le TLS et transmet les requêtes à Vitesse en HTTP/1.1, comme on le fait souvent avec Express. Le proxy peut aussi se charger de la compression.

Derrière un proxy, `req.ip()` renvoie l'adresse du proxy pour les requêtes qu'il transmet : celle du client se trouve dans l'en-tête `X-Forwarded-For`. Tout cela est détaillé dans [Production](production.md).
