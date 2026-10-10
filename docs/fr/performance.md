# Performances

Vitesse a été conçu pour ajouter le moins de travail possible autour de ce que le noyau fait déjà pour chaque requête. Cette page présente les résultats du benchmark, la façon dont ils ont été mesurés et comment les reproduire, explique d'où vient la vitesse, et donne des conseils concrets pour que votre propre application reste rapide.

## Benchmark

Requêtes par seconde, et entre parenthèses le temps CPU consommé par le serveur pour chaque requête (plus c'est bas, mieux c'est) :

| Scénario | Express 5 | Drogon 1.9 (C++) | axum 0.8 | actix-web 4 | **Vitesse** |
|---|---:|---:|---:|---:|---:|
| `GET /` (texte) | 5 921 (180 µs) | 203 244 (9,7 µs) | 168 165 (11,7 µs) | 274 195 (7,1 µs) | **291 415 (6,1 µs)** |
| `GET /json` | 5 765 (184 µs) | 120 489 (16,5 µs) | 165 988 (11,9 µs) | 248 987 (7,9 µs) | **313 438 (5,9 µs)** |
| `GET /json` envoyé par un navigateur (12 en-têtes) | 5 627 (188 µs) | 92 041 (21,7 µs) | 131 710 (15,0 µs) | 179 505 (10,9 µs) | **290 478 (6,8 µs)** |
| `GET /users/:id` (paramètre + JSON) | 5 627 (187 µs) | 98 486 (20,1 µs) | 151 303 (13,1 µs) | 209 173 (9,4 µs) | **304 323 (6,2 µs)** |
| `POST /echo` (lit et renvoie du JSON) | 4 538 (235 µs) | 69 876 (28,4 µs) | 105 570 (18,8 µs) | 170 129 (11,7 µs) | **253 941 (7,6 µs)** |
| `GET /` pipeliné ×16 | 8 352 (128 µs) | 724 059 (2,7 µs) | 213 678 (9,3 µs) | 1 272 510 (1,6 µs) | **2 781 541 (0,64 µs)** |

- **Contre actix-web**, le framework Rust réputé le plus rapide : jusqu'à **+62 %** de débit avec une vraie requête de navigateur, +45 à +49 % avec des paramètres ou un corps JSON, **2,2 fois plus** en pipeline, et 15 à 59 % de CPU en moins par requête.
- **Contre axum** : de 1,7 à 2,4 fois plus de requêtes par seconde, deux fois moins de CPU par requête, et 13 fois plus en pipeline.
- **Contre Drogon (C++)** : de 1,4 à 3,8 fois plus rapide.
- **Contre Express** : environ 50 fois plus rapide (et toujours 25 fois plus face à Express en cluster sur les mêmes 2 cœurs).

### Pourquoi l'écart avec actix est-il plus faible sur `GET /` ?

Sur la requête la plus simple, tous les serveurs rapides butent sur le même plancher : environ 4,7 µs de travail du noyau par requête (lecture, écriture et, sur la boucle locale, le traitement de la réception côté client, imputé à l'envoi du serveur). Vitesse n'ajoute que ~1,4 µs par-dessus, actix ~2,4 µs et axum ~7 µs. Dès que la requête ressemble à une vraie requête (en-têtes de navigateur, paramètres, corps JSON, pipelining), c'est le code du framework qui fait la différence, et l'écart se creuse. En production, à travers un vrai réseau, la part du noyau côté serveur est plus faible : l'avantage de Vitesse n'en est que plus visible.

## Méthodologie

- **Machine** : une VM à 4 vCPU. Le serveur est épinglé sur 2 cœurs et [wrk](https://github.com/wg/wrk) sur les 2 autres : le générateur de charge ne prend jamais de temps CPU au serveur.
- **Charge** : 128 connexions keep-alive, 10 secondes par scénario, après 2 secondes d'échauffement, sur la même machine et dans la même session pour tous les serveurs.
- **Versions** : Node 22.22 / Express 5.3.0, Drogon 1.9.13 (GCC 13, `-O3`), axum 0.8, actix-web 4.15, Rust 1.97, et l'allocateur système partout.
- **Scénarios** : le scénario « navigateur » envoie les 12 en-têtes d'une vraie requête de Chrome (environ 650 octets) ; `POST /echo` envoie un petit document JSON que le serveur analyse et renvoie ; le scénario pipeliné envoie 16 requêtes d'un coup sur chaque connexion.
- **Temps CPU par requête** : le temps CPU (utilisateur + système) consommé par le processus serveur pendant le scénario, divisé par le nombre de requêtes servies. Sans pipeline, les serveurs les plus rapides saturent wrk lui-même : le temps CPU mesuré côté serveur est alors le juge le plus fiable.

Les mesures varient de quelques pourcents d'une exécution à l'autre.

## Reproduire le benchmark

Le script et le code de chaque serveur se trouvent dans [`bench/`](https://github.com/maxlestage/Vitesse/blob/master/bench/run.sh) (le serveur Vitesse est [`examples/bench.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/bench.rs)). Il vous faut Linux (le script utilise `taskset` et `/proc`), Rust, [wrk](https://github.com/wg/wrk), Node.js, `curl` et Python 3. Drogon est facultatif : s'il n'est pas installé, il est ignoré (indiquez son dossier d'installation avec `DROGON_PREFIX` si besoin).

```sh
bench/run.sh               # 10 s par scénario, 128 connexions
bench/run.sh 30s 256       # durée et nombre de connexions personnalisés
SERVER_CPUS=0,1,2,3 CLIENT_CPUS=4,5,6,7 bench/run.sh   # sur une machine à 8 cœurs
```

Le script compile tout en mode release, lance chaque serveur à tour de rôle (Express, Express en cluster, Drogon, axum, actix-web, Vitesse) et affiche les résultats sous forme de tableau Markdown. Listez les processeurs en les séparant par des virgules : leur nombre fixe aussi le nombre de threads des serveurs.

Pour essayer rapidement Vitesse seul :

```sh
cargo run --release --example bench    # écoute sur le port 3000
wrk -t2 -c128 -d10s http://127.0.0.1:3000/json
```

Le serveur de benchmark lit aussi `PORT`, `WORKERS`, et `VITESSE_MODE=mt` pour utiliser le runtime multi-thread de tokio au lieu d'un thread par cœur.

## Pourquoi c'est rapide

La quasi-totalité du temps d'une requête simple est passée dans le noyau (lecture et écriture du socket) : un serveur rapide est un serveur qui ajoute le moins possible autour. Vitesse fait exactement **un `read` et un `write` par requête**, et un seul de chaque pour tout un lot de requêtes pipelinées.

- **Un moteur HTTP/1.1 maison** ([`src/http1.rs`](https://github.com/maxlestage/Vitesse/blob/master/src/http1.rs)) :
  - la tête de la requête est analysée par [httparse](https://github.com/seanmonstar/httparse) (SIMD), et les en-têtes ne sont notés que par leur position. La `HeaderMap` et l'`Uri` ne sont construites que si un handler les demande : une requête de navigateur et ses douze en-têtes coûtent presque autant qu'une requête nue ;
  - les réponses sont sérialisées directement dans un tampon d'écriture réutilisé : lignes de statut et types de contenu courants pré-calculés, en-tête `Date` en cache par thread, `HeaderMap` créée seulement si vous ajoutez d'autres en-têtes ;
  - un handler qui répond sans attendre suit un chemin entièrement synchrone, sans `Future` intermédiaire ni copie de grosses structures ;
  - un seul minuteur par connexion (et non par requête) gère l'inactivité.
- **Presque aucune allocation** : les requêtes (et leurs tampons) sont recyclées par thread, les tables d'en-têtes des réponses aussi, et le routeur écrit les paramètres dans des tampons réutilisés. Il ne reste que le `Future` du handler (et le tampon d'un corps JSON).
- **Un thread par cœur** (Linux) : chaque cœur a sa propre boucle d'événements et son propre socket `SO_REUSEPORT` ; le noyau répartit les connexions et une requête ne change jamais de thread.
- **Un routeur sans regex** : un arbre de segments parcouru sans aucune allocation pour les routes statiques ; les paramètres pointent dans le chemin.
- **Zéro compteur atomique partagé par requête** : l'application est figée au démarrage (`&'static`), handlers, middlewares et état sont lus sans `Arc`.
- **Peu de copies** : la requête traverse middlewares et handlers en ne déplaçant qu'un pointeur, la réponse ne pèse que 72 octets, et le corps n'est lu que si le handler le demande.

La vitesse ne se fait pas au détriment de la robustesse : voir les [limites intégrées au moteur](server.md).

## Garder une application rapide

### Compilez en mode release

Les builds de debug sont beaucoup plus lents : ne les mesurez ni ne les déployez jamais. Lancez `cargo build --release` (ou `cargo run --release`), et activez les mêmes optimisations que Vitesse dans votre `Cargo.toml` :

```toml
[profile.release]
lto = "fat"         # optimise à travers les crates, Vitesse compris
codegen-units = 1   # compilation plus lente, binaire plus rapide
```

> [!WARNING]
> N'ajoutez pas `panic = "abort"` : Vitesse rattrape les paniques pour en faire des réponses `500`, et avec `abort` un seul handler qui panique ferait tomber tout le serveur.

### Ne bloquez jamais la boucle d'événements

Les handlers tournent sur un petit nombre de threads. Un appel bloquant (calcul lourd, hachage de mot de passe, `std::fs`, pilote de base de données synchrone, `std::thread::sleep`) fige toutes les autres connexions de son thread pendant qu'il s'exécute. Utilisez des API asynchrones (`tokio::fs`, pilotes asynchrones) et déplacez le travail gourmand en CPU vers le pool de threads bloquants de tokio :

```rust
app.post("/hash", |req: Request| async move {
    let password = req.text().await?;
    // S'exécute sur le pool de threads bloquants de tokio : la boucle d'événements reste libre.
    let hash = tokio::task::spawn_blocking(move || expensive_hash(&password)).await?;
    Ok::<_, Error>(hash)
});
```

De même, ne gardez jamais un verrou `std::sync::Mutex` à travers un `.await`. Si beaucoup de handlers doivent vraiment bloquer, voir [`thread_per_core(false)`](server.md).

### Créez une seule fois les ressources coûteuses

Pools de base de données, clients HTTP et templates compilés doivent être créés une seule fois au démarrage et partagés via l'[état](state.md), pas reconstruits à chaque requête :

```rust
struct Services {
    http: reqwest::Client, // garde un pool de connexions, réutilisé par chaque requête
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

`req.state::<T>()` renvoie une simple référence, sans verrou ni compteur de références.

### Ne lisez que ce dont vous avez besoin

Le corps n'est lu que si vous appelez `req.json()`, `req.text()`, `req.bytes()` ou `req.form()` : une route qui n'en a pas besoin ne paie rien. Pour les gros envois, `req.take_body()` vous donne le flux pour le traiter morceau par morceau au lieu de tout charger en mémoire. Gardez `body_limit` aussi petit que votre usage le permet.

### Préférez les données statiques et le JSON typé

```rust
#[derive(serde::Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

// Sérialisé directement dans le tampon de la réponse : aucun arbre intermédiaire.
app.get("/health", |_| async { Json(Health { status: "ok", version: "1.0" }) });

// Construit un `serde_json::Value` (quelques allocations) avant de le sérialiser.
app.get("/health-dyn", |_| async { json!({ "status": "ok", "version": "1.0" }) });

// Données statiques : jamais copiées.
app.get("/robots.txt", |_| async { "User-agent: *\nDisallow:\n" });
app.get("/pixel", |_| async { Bytes::from_static(b"GIF89a") });
```

`json!` est parfait pour prototyper et pour les routes peu sollicitées ; sur les chemins critiques, une structure `#[derive(Serialize)]` enveloppée dans `Json` évite de construire une valeur intermédiaire. Un `&'static str` ou un `Bytes::from_static` est envoyé sans aucune copie (`Bytes` est réexporté sous le nom `vitesse::Bytes`).

### Gardez les middlewares globaux légers

Les middlewares globaux s'exécutent pour chaque requête, 404 comprises. Faites les vérifications coûteuses dans des middlewares de route ou de routeur, seulement là où elles sont utiles (voir [Middlewares](middleware.md)). Par exemple, `middleware::logger()` écrit une ligne sur la sortie standard à chaque requête : très pratique, mais mesurez son coût sous forte charge.

### Mesurez

Mesurez le build release, depuis un générateur de charge qui ne dispute pas les mêmes cœurs au serveur :

```sh
cargo run --release
wrk -t2 -c128 -d10s http://127.0.0.1:3000/
oha -z 10s -c 128 http://127.0.0.1:3000/
```

[wrk](https://github.com/wg/wrk) et [oha](https://github.com/hatoo/oha) donnent tous deux le débit et la latence. Regardez les percentiles de latence autant que les requêtes par seconde, et mesurez avec des données proches de la production : une application est en général limitée par sa base de données bien avant de l'être par Vitesse.
