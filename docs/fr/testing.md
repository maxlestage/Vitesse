# Tests

Vitesse fournit un client de test en mémoire, `vitesse::test::TestClient`, qui envoie les requêtes directement à votre application, sans ouvrir de port. Il joue le rôle de [supertest](https://github.com/ladjs/supertest) dans le monde Express : les tests sont rapides, s'exécutent en parallèle et n'ont besoin d'aucun serveur lancé.

## Mise en place

`TestClient` fait partie de Vitesse : il vous faut seulement un runtime asynchrone pour vos tests. Ajoutez tokio en dépendance de développement (inutile s'il figure déjà dans vos `[dependencies]` avec les fonctionnalités `macros` et `rt`) :

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
```

Chaque test est une `async fn` annotée avec `#[tokio::test]`, qui démarre un petit runtime rien que pour ce test.

## Rendre l'application testable

Les tests doivent construire la même application que `main`, sans démarrer le serveur. Le plus simple : une fonction qui renvoie l'`App`, dans `src/lib.rs`.

```rust
// src/lib.rs
use vitesse::prelude::*;

pub fn app() -> App {
    let mut app = App::new();
    app.get("/hello/:name", |req: Request| async move {
        format!("Bonjour {} !", req.param("name").unwrap_or("inconnu"))
    });
    app
}
```

```rust
// src/main.rs
fn main() -> std::io::Result<()> {
    my_api::app().run(3000) // `my_api` est le nom de votre paquet
}
```

Les tests d'intégration du dossier `tests/` peuvent alors appeler `my_api::app()`.

> [!TIP]
> C'est le même principe que `module.exports = app` dans `app.js` avec `app.listen()` dans `server.js` : définir l'application et démarrer le serveur sont deux étapes distinctes.

## Votre premier test

```rust
// tests/hello.rs
use vitesse::test::TestClient;

#[tokio::test]
async fn dit_bonjour() {
    let client = TestClient::new(my_api::app());

    let res = client.get("/hello/Ada").await;

    assert_eq!(res.status(), 200);
    assert_eq!(res.header("content-type"), Some("text/plain; charset=utf-8"));
    assert_eq!(res.text(), "Bonjour Ada !");
}
```

Lancez-le avec `cargo test`. `TestClient::new` prend l'`App` et la fige, exactement comme le ferait `app.run`. Chaque requête traverse ensuite tout ce que traverse une vraie requête : middlewares globaux, routage, middlewares de route, handler, `app.on_error`, `app.body_limit`, et paniques transformées en réponses `500`.

## Construire les requêtes

On démarre une requête depuis le client :

| Méthode | Requête |
|---|---|
| `client.get(uri)` | `GET` |
| `client.post(uri)` | `POST` |
| `client.put(uri)` | `PUT` |
| `client.patch(uri)` | `PATCH` |
| `client.delete(uri)` | `DELETE` |
| `client.request(Method::HEAD, uri)` | toute autre méthode (`HEAD`, `OPTIONS`…) |

Puis on enchaîne ce dont on a besoin, et on l'envoie avec `.await` :

| Méthode | Effet |
|---|---|
| `.header(nom, valeur)` | Ajoute un en-tête (panique si le nom ou la valeur est invalide) |
| `.body(données)` | Corps brut : `&'static str`, `String`, `Vec<u8>`, `Bytes`… |
| `.json(&valeur)` | Sérialise `valeur` avec serde et pose `content-type: application/json` |
| `.form(&valeur)` | Formulaire encodé, avec `content-type: application/x-www-form-urlencoded` |
| `.await` ou `.send().await` | Envoie la requête et renvoie une `TestResponse` |

```rust
let res = client
    .post("/items")
    .header("authorization", "Bearer secret")
    .json(&json!({ "name": "Ada" }))
    .await;

// La query string se met dans l'URI, déjà encodée.
let res = client.get("/search?q=caf%C3%A9+cr%C3%A8me").await;

let res = client.post("/login").form(&[("user", "ada"), ("remember", "true")]).await;
let res = client.request(Method::OPTIONS, "/items").await;
```

> [!NOTE]
> Une requête de test est paresseuse : rien n'est envoyé tant que vous ne faites pas `.await`. Si vous oubliez le `.await`, le compilateur vous prévient.

## Lire la réponse

Une `TestResponse` contient déjà tout son corps en mémoire :

| Méthode | Renvoie |
|---|---|
| `res.status()` | Le `StatusCode`, comparable à un simple nombre : `assert_eq!(res.status(), 404)` |
| `res.header(nom)` | Un en-tête, en `Option<&str>` |
| `res.headers()` | Tous les en-têtes (`&HeaderMap`) |
| `res.text()` | Le corps en `String` |
| `res.bytes()` | Le corps brut (`&Bytes`) |
| `res.json::<T>()` | Le corps désérialisé depuis du JSON (panique en affichant le corps s'il est invalide) |

```rust
#[derive(serde::Deserialize, Debug, PartialEq)]
struct User {
    id: u64,
    name: String,
}

let user: User = res.json();                        // typé
let body: vitesse::serde_json::Value = res.json();  // non typé
assert_eq!(body["name"], "Ada");
```

> [!NOTE]
> Aucun socket n'intervient : les en-têtes que le moteur HTTP ajoute en écrivant sur le réseau (`content-length`, `date`, `connection`, `transfer-encoding`) sont donc absents d'une `TestResponse`. Pour les vérifier, testez contre un vrai serveur (voir plus bas).

## Un exemple complet

L'application `hello` du début, complétée par une petite API d'utilisateurs avec un état partagé, et les tests qui vont avec :

```rust
// src/lib.rs
use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

/// Notre « base de données » : une table en mémoire partagée par toutes les requêtes.
#[derive(Default)]
struct Users(Mutex<HashMap<u64, User>>);

pub fn app() -> App {
    let mut app = App::new();
    app.state(Users::default());

    app.get("/hello/:name", |req: Request| async move {
        format!("Bonjour {} !", req.param("name").unwrap_or("inconnu"))
    });

    app.post("/users", |req: Request| async move {
        let new: NewUser = req.json().await?;
        if new.name.trim().is_empty() {
            return Err(Error::unprocessable("le nom est obligatoire"));
        }
        let mut users = req.state::<Users>().0.lock().unwrap();
        let user = User { id: users.len() as u64 + 1, name: new.name };
        users.insert(user.id, user.clone());
        Ok((201, Json(user)))
    });

    app.get("/users/:id", |req: Request| async move {
        let id: u64 = req.param_as("id")?;
        let users = req.state::<Users>().0.lock().unwrap();
        users
            .get(&id)
            .cloned()
            .map(Json)
            .ok_or_else(|| Error::not_found("utilisateur introuvable"))
    });

    app
}
```

```rust
// tests/api.rs
use my_api::{User, app};
use vitesse::json;
use vitesse::serde_json::Value;
use vitesse::test::TestClient;

#[tokio::test]
async fn cree_puis_lit_un_utilisateur() {
    let client = TestClient::new(app());

    let res = client.post("/users").json(&json!({ "name": "Ada" })).await;
    assert_eq!(res.status(), 201);
    let created: User = res.json();
    assert_eq!(created.name, "Ada");

    let res = client.get(&format!("/users/{}", created.id)).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.json::<User>(), created);
}

#[tokio::test]
async fn refuse_les_mauvaises_entrees() {
    let client = TestClient::new(app());

    // Pas du JSON du tout : 400, produit par `req.json()`.
    let res = client.post("/users").body("{oups").await;
    assert_eq!(res.status(), 400);

    // JSON valide, mais notre propre validation échoue : 422 avec notre message.
    let res = client.post("/users").json(&json!({ "name": "" })).await;
    assert_eq!(res.status(), 422);
    let body: Value = res.json();
    assert_eq!(body["error"], "le nom est obligatoire");

    // Pas un nombre : 400. Identifiant inconnu : 404.
    assert_eq!(client.get("/users/abc").await.status(), 400);
    assert_eq!(client.get("/users/999").await.status(), 404);
}
```

Chaque appel à `app()` crée un état neuf : chaque test part d'une « base » vide, et les tests ne se gênent jamais, même lancés en parallèle.

## Tester les erreurs

Les erreurs sont des réponses comme les autres : vérifiez le statut et le corps `{"error": "..."}`. Les paniques et les erreurs internes méritent aussi un test, pour s'assurer que rien ne fuit vers le client :

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

async fn boom(_req: Request) -> &'static str {
    panic!("quelque chose s'est mal passé")
}

async fn parse(_req: Request) -> vitesse::Result<String> {
    let n: u32 = "pas un nombre".parse()?; // une erreur standard : devient une 500
    Ok(n.to_string())
}

#[tokio::test]
async fn les_erreurs_deviennent_des_reponses() {
    let mut app = App::new();
    app.get("/boom", boom);
    app.get("/parse", parse);
    let client = TestClient::new(app);

    // Une panique ne fait rien tomber : elle devient une 500.
    assert_eq!(client.get("/boom").await.status(), 500);

    // Le détail interne n'est jamais envoyé au client.
    let res = client.get("/parse").await;
    assert_eq!(res.status(), 500);
    assert_eq!(res.text(), r#"{"error":"Internal Server Error"}"#);

    // La 404 par défaut.
    assert_eq!(client.get("/nope").await.text(), r#"{"error":"Cannot GET /nope"}"#);
}
```

Un gestionnaire `app.on_error` personnalisé se vérifie de la même façon :

```rust
#[tokio::test]
async fn pages_d_erreur_personnalisees() {
    let mut app = App::new();
    app.on_error(|err: Error| {
        res::status(err.status()).html(format!("<h1>{}</h1>", err.message()))
    });
    let client = TestClient::new(app);

    let res = client.get("/nope").await;
    assert_eq!(res.status(), 404);
    assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(res.text(), "<h1>Cannot GET /nope</h1>");
}
```

> [!TIP]
> Vérifiez les statuts et vos propres messages. La formulation des messages générés par Vitesse lui-même (JSON invalide, paramètre invalide…) peut changer d'une version à l'autre.

Le message de la panique reste affiché dans la sortie des tests par le *panic hook* de Rust : c'est normal. Tout sur `Error` et `on_error` dans [Erreurs](errors.md).

## Tester les middlewares

Placez le middleware dans une mini-application avec une route factice, puis vérifiez les deux chemins : la requête qui passe et celle qui est arrêtée.

```rust
use vitesse::prelude::*;
use vitesse::test::TestClient;

async fn require_key(req: Request, next: Next) -> Response {
    match req.header("x-api-key") {
        Some("secret") => next.run(req).await,
        _ => Error::unauthorized("clé d'API manquante").into_response(),
    }
}

#[tokio::test]
async fn require_key_bloque_ou_laisse_passer() {
    let mut app = App::new();
    app.middleware(require_key);
    app.get("/", |_| async { "ok" });
    let client = TestClient::new(app);

    let res = client.get("/").await;
    assert_eq!(res.status(), 401);
    assert_eq!(res.text(), r#"{"error":"clé d'API manquante"}"#);

    let res = client.get("/").header("x-api-key", "secret").await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.text(), "ok");
}
```

Pour vérifier ce qu'un middleware attache à la requête (avec `req.set`), faites-le renvoyer par le handler factice :

```rust
#[derive(Clone)]
struct CurrentUser(String);

async fn fake_auth(mut req: Request, next: Next) -> Response {
    req.set(CurrentUser("ada".into()));
    next.run(req).await
}

async fn me(req: Request) -> String {
    req.get::<CurrentUser>().map(|u| u.0.clone()).unwrap_or_default()
}

#[tokio::test]
async fn le_handler_voit_ce_que_le_middleware_a_attache() {
    let mut app = App::new();
    app.get("/me", me.with(fake_auth));
    let client = TestClient::new(app);

    assert_eq!(client.get("/me").await.text(), "ada");
}
```

Les middlewares fournis se testent de la même manière, par exemple `middleware::cors()` en envoyant un en-tête `origin` et en vérifiant `access-control-allow-origin`. Pour écrire vos middlewares, voir [Middlewares](middleware.md).

## Tests d'intégration sur un vrai port

`TestClient` court-circuite le moteur HTTP/1.1. Pour ce qui n'existe que sur une vraie connexion (`content-length`, keep-alive, pipelining, corps `chunked`, `Expect: 100-continue`, limites de taille des en-têtes), ou pour utiliser un vrai client HTTP, démarrez le serveur sur un vrai port :

- `app.bind("127.0.0.1:0")` ouvre le socket ; le port `0` laisse l'OS choisir un port libre, si bien que des tests lancés en parallèle n'entrent jamais en collision ;
- `server.local_addr()` donne l'adresse réellement utilisée ;
- `tokio::spawn(server.run())` sert les requêtes en tâche de fond pendant toute la durée du test.

Avec [reqwest](https://docs.rs/reqwest) comme client :

```toml
[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
reqwest = { version = "0.12", default-features = false } # le HTTP simple suffit ici
```

```rust
// tests/server.rs
#[tokio::test]
async fn sert_en_tcp() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr(); // par ex. 127.0.0.1:49213
    tokio::spawn(server.run());

    let res = reqwest::get(format!("http://{addr}/hello/Ada")).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-length"], "13");
    assert_eq!(res.text().await.unwrap(), "Bonjour Ada !");
}
```

Pour tester aussi l'arrêt, utilisez `with_graceful_shutdown` avec un canal :

```rust
#[tokio::test]
async fn s_arrete_proprement() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(server.with_graceful_shutdown(async {
        stopped.await.ok();
    }));

    let res = reqwest::get(format!("http://{addr}/hello/Ada")).await.unwrap();
    assert_eq!(res.status(), 200);

    stop.send(()).unwrap();
    handle.await.unwrap().unwrap(); // le serveur a renvoyé Ok(())
}
```

Pour le comportement bas niveau, écrivez du HTTP brut sur un `TcpStream`, sans aucune dépendance supplémentaire :

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn repond_dans_l_ordre_aux_requetes_pipelinees() {
    let server = my_api::app().bind("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr();
    tokio::spawn(server.run());

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"GET /hello/A HTTP/1.1\r\nHost: test\r\n\r\n\
              GET /hello/B HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    let a = response.find("Bonjour A !").unwrap();
    let b = response.find("Bonjour B !").unwrap();
    assert!(a < b);
}
```

Voir [Configuration du serveur](server.md) pour `bind`, `Server` et l'arrêt propre.

## Organiser ses tests

- **Le dossier `tests/`** : un fichier par domaine (`tests/users.rs`, `tests/auth.rs`…), chacun passant par `TestClient`. Chaque fichier est compilé comme un crate à part ; partagez les utilitaires dans `tests/common/mod.rs` :

  ```rust
  // tests/common/mod.rs
  use vitesse::test::TestClient;

  pub fn client() -> TestClient {
      TestClient::new(my_api::app())
  }
  ```

  ```rust
  // tests/users.rs
  mod common;

  #[tokio::test]
  async fn utilisateur_inconnu_404() {
      let client = common::client();
      assert_eq!(client.get("/users/42").await.status(), 404);
  }
  ```

- **Les tests unitaires** dans un bloc `#[cfg(test)] mod tests` à côté de votre code, pour les fonctions privées. `Request` n'a pas de constructeur public : testez les handlers via `TestClient`, et gardez la logique métier dans des fonctions ordinaires que vous pouvez appeler directement.
- **Un état neuf** : construisez une nouvelle application par test (`TestClient::new(app())`). L'application figée vit jusqu'à la fin du programme de test, ce qui est négligeable. `TestClient` est `Copy` : passez-le librement à vos fonctions utilitaires.
- **Commandes utiles** : `cargo test` lance tout, `cargo test users` seulement les tests dont le nom contient `users`, et `cargo test -- --nocapture` affiche les `println!` et les lignes de `middleware::logger()`.
