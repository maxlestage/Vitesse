# Mise en production

Cette page rassemble ce qui compte quand une application Vitesse quitte votre ordinateur : une compilation optimisée, la configuration, un reverse proxy pour HTTPS et HTTP/2, WebSocket et HTTP/3, un gestionnaire de services, l'arrêt propre, les limites, les journaux, la sécurité et la supervision. Pour les conteneurs, voyez aussi [Docker](docker.md) ; pour Heroku, [Déployer sur Heroku depuis un téléphone](heroku-mobile.md).

## Compiler en mode release

Déployez toujours une compilation release : une compilation de débogage est bien plus lente.

```sh
cargo build --release
```

Pour les meilleures performances, ajoutez à **votre** `Cargo.toml` le profil utilisé par les benchmarks de Vitesse :

```toml
[profile.release]
lto = "fat"
codegen-units = 1
```

Cargo ne lit les profils que dans le paquet racine (ou l'espace de travail) : les réglages du `Cargo.toml` de Vitesse ne s'appliquent pas à votre application. La compilation est plus longue, le binaire plus rapide. En option, `strip = true` réduit la taille du binaire et `debug = "line-tables-only"` garde des traces d'appels et des profils lisibles.

> [!WARNING]
> N'utilisez pas `panic = "abort"`. Vitesse rattrape les paniques des handlers et les transforme en réponses `500` : une requête défaillante ne peut pas faire tomber le serveur. Avec `abort`, la moindre panique tue tout le processus.

Le binaire ne dépend que de la bibliothèque C du système (glibc) : compilez-le sur la même distribution Linux que le serveur (ou une plus ancienne), ou compilez-le dans Docker. `RUSTFLAGS="-C target-cpu=native"` peut aider, mais seulement si le binaire tourne sur la machine qui l'a compilé (ou un processeur identique).

## Configuration par l'environnement

Lisez vos réglages dans des variables d'environnement : le même binaire tourne alors partout.

```rust
use std::time::Duration;

use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.middleware(middleware::logger()); // une ligne par requête sur stdout
    app.middleware(middleware::helmet()); // en-têtes de sécurité
    app.middleware(middleware::timeout(Duration::from_secs(15))); // 503 si trop lent
    app.body_limit(256 * 1024); // 256 Kio au lieu de 1 Mio

    app.get("/health", |_| async { "ok" });
    app.get("/", |_| async { "Bonjour depuis la production !" });

    // La configuration vient de l'environnement.
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    if let Some(n) = std::env::var("WORKERS").ok().and_then(|n| n.parse().ok()) {
        app.workers(n);
    }

    app.run(format!("{host}:{port}"))
}
```

- **`PORT`** est la convention utilisée par la plupart des plateformes (Heroku, Render, Cloud Run…).
- **`HOST`** : `0.0.0.0` dans un conteneur ou sur une plateforme ; `127.0.0.1` derrière un reverse proxy sur la même machine, pour que l'application ne soit pas joignable directement depuis l'extérieur. Utilisez `[::]` pour l'IPv6.
- Un simple numéro de port (`app.run(3000)`, ou une chaîne composée uniquement de chiffres) écoute sur toutes les interfaces IPv4.

`HOST` et `WORKERS` ne sont que des noms choisis pour cet exemple : Vitesse lui-même ne lit aucune variable d'environnement.

## Derrière un reverse proxy

Vitesse parle HTTP/1.1, sans TLS sur TCP. En production, placez un reverse proxy devant : il gère HTTPS et les certificats, HTTP/2 (et HTTP/3, sauf si Vitesse le sert lui-même : voir [HTTP/3](#http3)), la compression, et parle à Vitesse en HTTP/1.1 simple, sur des connexions keep-alive locales.

Vitesse ferme une connexion après environ 60 secondes sans requête (environ 30 secondes si un client ne finit jamais d'envoyer ses en-têtes). Laissez le proxy réutiliser ses connexions vers l'application, et faites-lui fermer les connexions inactives un peu avant.

### Caddy

[Caddy](https://caddyserver.com) est l'option la plus simple : il obtient les certificats HTTPS automatiquement et active HTTP/2 et HTTP/3 par défaut. Un `Caddyfile` complet :

```text
exemple.fr {
	encode zstd gzip

	reverse_proxy 127.0.0.1:3000 {
		header_up X-Real-IP {remote_host}
		transport http {
			keepalive 30s
		}
	}
}
```

Caddy réutilise par défaut ses connexions vers l'application et ajoute lui-même `X-Forwarded-For`, `X-Forwarded-Proto` et `X-Forwarded-Host`. Appliquez les changements avec `sudo systemctl reload caddy`.

### Nginx

Avec [Nginx](https://nginx.org), le keep-alive vers l'application demande un bloc `upstream`, `proxy_http_version 1.1` et un en-tête `Connection` vide :

```nginx
upstream vitesse {
    server 127.0.0.1:3000;
    keepalive 64;            # connexions inactives gardées ouvertes vers l'app
    keepalive_timeout 30s;   # fermées avant que Vitesse ne le fasse (~60 s)
}

server {
    listen 80;
    listen [::]:80;
    server_name exemple.fr;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;                # Nginx < 1.25.1 : « listen 443 ssl http2; » à la place
    server_name exemple.fr;

    ssl_certificate     /etc/letsencrypt/live/exemple.fr/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/exemple.fr/privkey.pem;

    client_max_body_size 1m; # cohérent avec app.body_limit

    location / {
        proxy_pass http://vitesse;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Les certificats peuvent venir de Let's Encrypt, par exemple avec `sudo certbot --nginx -d exemple.fr`. Vérifiez la configuration avec `sudo nginx -t`, puis appliquez-la avec `sudo systemctl reload nginx`.

### WebSocket derrière Nginx

Caddy laisse passer les connexions [WebSocket](websocket.md) sans aucune configuration. Nginx doit transmettre explicitement les en-têtes `Upgrade` et `Connection`, et il lui faut un `proxy_read_timeout` long : par défaut, il ferme une connexion restée silencieuse pendant 60 secondes. Ajoutez une `location` pour vos routes WebSocket (ici, tout ce qui est sous `/ws/`) au bloc `server` ci-dessus :

```nginx
location /ws/ {
    proxy_pass http://vitesse;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_read_timeout 1h;
    proxy_send_timeout 1h;
}
```

Chaque WebSocket garde sa propre connexion entre Nginx et Vitesse ouverte pendant toute sa vie, en dehors du pool `keepalive`. Pour les connexions qui peuvent rester longtemps silencieuses, faites envoyer régulièrement un ping par le serveur (voir [WebSocket](websocket.md#derrière-un-reverse-proxy)).

### Connaître l'adresse IP du client

Derrière un proxy, `req.ip()` renvoie l'adresse du proxy (`127.0.0.1`). Les deux configurations ci-dessus transmettent l'adresse réelle dans `X-Real-IP` :

```rust
app.get("/ip", |req: Request| async move {
    // Posé par le reverse proxy ; sinon, l'adresse TCP du client.
    req.header("x-real-ip")
        .map(str::to_owned)
        .or_else(|| req.ip().map(|ip| ip.to_string()))
        .unwrap_or_default()
});
```

> [!IMPORTANT]
> Ne faites confiance à cet en-tête que si l'application n'est joignable **que** par le proxy (écoute sur `127.0.0.1`). Sinon, n'importe quel client peut envoyer un faux `X-Real-IP`.

Si Vitesse sert aussi HTTP/3 (ci-dessous), ces requêtes lui arrivent directement, sans passer par le proxy : `req.ip()` est alors la vraie adresse du client, et un en-tête `X-Real-IP` ne peut venir que du client. Ne lisez l'en-tête que pour les requêtes HTTP/1.1 (`req.version()`).

## HTTP/3

Les navigateurs utilisent HTTP/3 quand le site l'annonce. Deux façons de le proposer :

- **Laisser le proxy s'en charger.** Caddy active HTTP/3 par défaut, Nginx avec `listen 443 quic` (version 1.25 ou plus récente). Rien ne change pour Vitesse.
- **Laisser Vitesse s'en charger**, avec la feature `http3` : le proxy garde le port TCP 443, et Vitesse reçoit directement le port UDP 443, avec le même certificat. Ses réponses HTTP/1.1 portent l'en-tête `alt-svc`, que le proxy transmet aux navigateurs.

```rust
use vitesse::http3::Http3;

app.http3(
    Http3::from_pem_files("/etc/mon-app/tls/fullchain.pem", "/etc/mon-app/tls/privkey.pem")?
        .port(443), // HTTP/3 sur UDP 443, annoncé aux navigateurs par alt-svc
);
app.run("0.0.0.0:3000") // TCP 3000 pour le proxy ; le socket UDP utilise la même IP
```

Dans le second cas :

- ouvrez le port **UDP** 443 dans le pare-feu (`sudo ufw allow 443/udp`), et gardez le port TCP 3000 fermé ;
- sous systemd, ajoutez `AmbientCapabilities=CAP_NET_BIND_SERVICE` pour que l'utilisateur du service puisse ouvrir le port 443 ;
- Nginx ne doit pas écouter sur `443 quic`, et Caddy a besoin de `protocols h1 h2` dans ses options globales, pour laisser le port UDP 443 à Vitesse ;
- le certificat est chargé au démarrage : redémarrez l'application après chaque renouvellement.

Tout est détaillé dans [HTTP/3 et QUIC](http3.md#en-production-derrière-caddy-ou-nginx).

## Lancer l'application comme un service avec systemd

Sur un serveur Linux, systemd démarre l'application au boot, la relance si elle plante et récupère ses journaux. Créez un utilisateur dédié et copiez le binaire :

```sh
sudo useradd --system --no-create-home vitesse
sudo mkdir -p /opt/mon-app
sudo cp target/release/mon-app /opt/mon-app/server
```

Puis créez `/etc/systemd/system/mon-app.service` :

```ini
[Unit]
Description=Mon application Vitesse
After=network.target

[Service]
User=vitesse
Group=vitesse
WorkingDirectory=/opt/mon-app
ExecStart=/opt/mon-app/server
Environment=HOST=127.0.0.1
Environment=PORT=3000
# Ou gardez les réglages dans un fichier à part :
# EnvironmentFile=/etc/mon-app.env
Restart=on-failure
RestartSec=2
# Vitesse laisse jusqu'à 10 s aux requêtes en cours après SIGTERM.
TimeoutStopSec=15
# Uniquement pour HTTP/3 sur le port UDP 443 :
# AmbientCapabilities=CAP_NET_BIND_SERVICE
# Un descripteur de fichier par connexion.
LimitNOFILE=65536
# Durcissement (ajoutez ReadWritePaths=... si l'application écrit des fichiers).
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now mon-app
systemctl status mon-app
journalctl -u mon-app -f       # suivre les journaux
```

Pour déployer une nouvelle version, remplacez le binaire (copiez-le à côté de l'ancien, puis renommez-le : copier par-dessus un binaire en cours d'exécution échoue) et redémarrez :

```sh
sudo cp target/release/mon-app /opt/mon-app/server.new
sudo mv /opt/mon-app/server.new /opt/mon-app/server
sudo systemctl restart mon-app
```

Un redémarrage prend une fraction de seconde, mais les connexions tentées pendant ce temps échouent. Pour déployer sans interruption, faites tourner deux instances sur deux ports derrière le proxy et redémarrez-les l'une après l'autre.

## Arrêt propre

`app.run`, `app.listen(port).await` et `Server::run` surveillent `Ctrl+C` (`SIGINT`) et `SIGTERM`, que systemd, Docker, Kubernetes ou Heroku envoient avant d'arrêter une application. Vitesse alors :

1. cesse d'accepter de nouvelles connexions ;
2. laisse les requêtes en cours se terminer, pendant **10 secondes** au maximum ;
3. ferme les connexions restantes et rend la main depuis `app.run` (ou `app.listen`).

Les connexions HTTP/3 s'arrêtent en même temps, après un `GOAWAY`, avec les mêmes 10 secondes pour leurs requêtes. Les connexions WebSocket sont fermées au plus tard à la fin du délai de grâce : faites en sorte que vos clients se reconnectent automatiquement.

Ce délai de grâce de 10 secondes est fixe. Vérifiez que votre plateforme attend un peu plus longtemps avant de tuer le processus : systemd attend 90 secondes par défaut (`TimeoutStopSec`), Kubernetes 30 secondes (`terminationGracePeriodSeconds`), Heroku 30 secondes, mais Docker seulement 10 secondes (utilisez `--stop-timeout 15`).

Pour exécuter votre propre code à l'arrivée du signal (une ligne de journal, vider un tampon), ou pour vous arrêter sur un autre événement, ouvrez le port avec `app.bind` et passez votre propre futur à `with_graceful_shutdown`. Il remplace `Ctrl+C` et `SIGTERM` : incluez-les si vous en avez encore besoin :

```rust
use tokio::signal::unix::{SignalKind, signal};
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });

    let server = app.bind("0.0.0.0:3000").await?;
    println!("écoute sur http://{}", server.local_addr());

    let mut sigterm = signal(SignalKind::terminate())?;
    server
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            println!("arrêt en cours…");
        })
        .await
}
```

Cet exemple (Unix uniquement) demande `tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }` dans vos dépendances.

## Vérifications de santé

Une route légère qui répond vite suffit aux répartiteurs de charge, aux sondes Kubernetes et aux services de surveillance :

```rust
app.get("/health", |_| async { "ok" });
```

Si l'application dépend d'une base de données, ajoutez une route séparée (par exemple `/ready`) qui la vérifie, et gardez `/health` indépendante : une panne de la base ne doit pas faire redémarrer toutes les instances. Les middlewares globaux, comme le logger, s'exécutent aussi pour ces requêtes.

## Journaux

- `middleware::logger()` écrit une ligne par requête sur la sortie standard, comme `GET /users/42 200 0.084 ms`. Les couleurs ne sont utilisées que si la sortie est un terminal : les journaux de journald, de Docker et des plateformes restent propres.
- Quand une erreur `5xx` a une cause (une erreur Rust convertie avec `?`, ou `Error::with_source`), cette cause est affichée sur la sortie d'erreur, préfixée par `[vitesse]` ; le client ne reçoit qu'un message générique.
- Une panique dans un handler ou un middleware devient une `500`, et Rust affiche le message de la panique sur la sortie d'erreur.

Besoin de journaux JSON pour un collecteur ? Écrivez votre propre middleware (voir [Middlewares](middleware.md)) :

```rust
app.middleware(|req: Request, next: Next| async move {
    let start = std::time::Instant::now();
    let (method, path) = (req.method().clone(), req.path().to_owned());
    let res = next.run(req).await;
    println!(
        "{}",
        json!({
            "method": method.as_str(),
            "path": path,
            "status": res.status_code().as_u16(),
            "ms": start.elapsed().as_secs_f64() * 1000.0,
        })
    );
    res
});
```

## Limites

| Quoi | Par défaut | Comment la changer |
|---|---|---|
| Corps lu en mémoire (`json`, `form`, `text`, `bytes`) | 1 Mio, puis `413` | `app.body_limit(octets)` |
| Tête de requête (ligne de requête et en-têtes) | 60 Kio et 64 en-têtes, puis `431` | Fixe |
| Tête de requête incomplète | Environ 30 secondes, puis `408` | Fixe |
| Connexion keep-alive inactive | Fermée après environ 60 secondes | Fixe |
| Temps passé dans un handler | Illimité | `middleware::timeout(durée)`, qui répond `503` |

Abaissez `body_limit` à ce dont vos routes ont vraiment besoin, et gardez la limite du proxy (`client_max_body_size` dans Nginx) cohérente.

## Workers et threads

- Par défaut, `app.run` démarre **un thread par cœur**. Sous Linux, chaque thread a sa propre boucle d'événements et son propre socket `SO_REUSEPORT` : le noyau répartit les connexions entre les threads, et une requête ne change jamais de thread. C'est le mode le plus rapide.
- `app.workers(n)` fixe explicitement le nombre de threads : utile dans un conteneur dont le quota de CPU est inférieur au nombre de cœurs qu'il voit, ou pour laisser des cœurs à d'autres processus (une base de données sur la même machine, par exemple).
- `app.thread_per_core(false)` passe au runtime multi-thread de tokio, qui rééquilibre le travail entre les threads. Préférez-le si des handlers font de longs calculs, ou si les connexions sont peu nombreuses et très inégales. Derrière un proxy, tout le trafic arrive par le groupe de connexions keep-alive du proxy : donnez-lui assez de connexions (`keepalive 64` dans l'exemple Nginx) pour qu'elles se répartissent sur tous les threads, ou désactivez ce mode si un cœur sature pendant que les autres se tournent les pouces.

Ne bloquez jamais un thread dans un handler (`std::thread::sleep`, un long calcul, un pilote de base de données synchrone…) : toutes les connexions servies par ce thread attendraient. Déplacez ce travail vers le pool de threads bloquants de tokio :

```rust
app.get("/rapport", |_| async {
    let rapport = vitesse::tokio::task::spawn_blocking(calcul_couteux)
        .await
        .map_err(|e| Error::internal("la tâche a échoué").with_source(e))?;
    Ok::<_, Error>(rapport)
});
```

Voyez [Configuration du serveur](server.md) et [Performances](performance.md) pour aller plus loin.

## Liste de contrôle de sécurité

- **HTTPS partout**, géré par le proxy, avec une redirection de HTTP vers HTTPS.
- **L'application n'est pas exposée directement** : elle écoute sur `127.0.0.1` (ou un réseau privé), et le pare-feu n'ouvre que les ports 80 et 443 (TCP, plus UDP 443 si Vitesse sert [HTTP/3](#http3) : il écoute alors sur toutes les interfaces, et le pare-feu garde son port TCP fermé).
- **Un utilisateur non privilégié** exécute le processus (`User=` dans systemd ; l'image Docker fournie utilise déjà un utilisateur `vitesse`).
- **En-têtes de sécurité** avec `middleware::helmet()` : `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Strict-Transport-Security`… Ce dernier demande aux navigateurs de n'utiliser que HTTPS pendant un an, sous-domaines compris : activez-le une fois que HTTPS fonctionne pour le domaine et ses sous-domaines.
- **CORS** avec une liste explicite d'origines autorisées, et **cookies** marqués `http_only`, `secure` et `same_site` (voir ci-dessous).
- **Limites** : un `body_limit` adapté à vos routes, un `timeout`, et une limitation de débit au niveau du proxy si besoin (par exemple `limit_req` dans Nginx) : Vitesse n'inclut pas de limiteur de débit.
- **Messages d'erreur** : le détail des erreurs `5xx` n'est jamais envoyé aux clients, mais les messages des erreurs que vous créez (`Error::bad_request("…")`) le sont : n'y mettez rien de sensible.
- **Fichiers statiques** : `ServeDir` refuse `../` et les fichiers cachés (`.env`, `.git`) par défaut ; servez malgré tout un dossier dédié, jamais la racine du projet.
- **Les secrets** vivent dans des variables d'environnement ou un gestionnaire de secrets, jamais dans le dépôt ni dans l'image.
- **Dépendances** : mettez-les à jour régulièrement (`cargo update`) et vérifiez les alertes de sécurité avec [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit).

### CORS et cookies

```rust
use std::time::Duration;

use vitesse::SameSite;
use vitesse::prelude::*;

let mut app = App::new();

// Seule cette origine peut appeler l'API avec les cookies de l'utilisateur.
app.middleware(
    middleware::cors()
        .allow_origin("https://app.exemple.fr")
        .allow_credentials(true)
        .max_age(Duration::from_secs(600)),
);

app.post("/login", |_| async {
    Response::new()
        .cookie(
            Cookie::new("session", "abc123")
                .http_only(true)
                .secure(true)
                .same_site(SameSite::Lax),
        )
        .json(json!({ "ok": true }))
});
```

> [!WARNING]
> `cors().allow_credentials(true)` **sans** `allow_origin` accepte toutes les origines : n'importe quel site pourrait alors envoyer des requêtes portant les cookies de vos utilisateurs.

## Supervision

- **Journaux** : `journalctl -u mon-app`, `docker logs`, ou la visionneuse de votre plateforme ; transférez-les vers un service de journaux si vous avez besoin de recherche et d'alertes.
- **Disponibilité** : un service de surveillance externe qui appelle `/health` toutes les minutes.
- **Système** : surveillez le CPU, la mémoire et le nombre de descripteurs de fichiers ouverts (un par connexion, limité par `LimitNOFILE` sous systemd).

Vitesse n'a pas de métriques intégrées, mais un middleware et quelques compteurs atomiques couvrent l'essentiel :

```rust
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
struct Metrics {
    requests: AtomicU64,
    server_errors: AtomicU64,
}

app.state(Metrics::default());

app.middleware(|req: Request, next: Next| async move {
    let metrics = req.state::<Metrics>();
    let res = next.run(req).await;
    metrics.requests.fetch_add(1, Relaxed);
    if res.status_code().is_server_error() {
        metrics.server_errors.fetch_add(1, Relaxed);
    }
    res
});

app.get("/metrics", |req: Request| async move {
    let m = req.state::<Metrics>();
    format!(
        "http_requests_total {}\nhttp_server_errors_total {}\n",
        m.requests.load(Relaxed),
        m.server_errors.load(Relaxed),
    )
});
```

Prometheus sait collecter ce format texte. N'exposez pas `/metrics` publiquement : bloquez-la au niveau du proxy.
