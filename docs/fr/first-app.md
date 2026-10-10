# Votre première application

Dans ce tutoriel, vous allez construire une petite API JSON étape par étape : un Hello World, une route JSON, une route avec un paramètre, une route `POST` qui lit un corps JSON, et la journalisation des requêtes. Comptez une dizaine de minutes.

Il vous faut d'abord Rust. Voir [Installation](installation.md).

## 1. Créer le projet

```sh
cargo new hello-vitesse
cd hello-vitesse
cargo add vitesse
cargo add serde --features derive
```

Si Vitesse n'est pas encore sur crates.io, utilisez plutôt `cargo add vitesse --git https://github.com/maxlestage/Vitesse` (voir [Installation](installation.md#ajouter-vitesse)).

## 2. Hello World

Remplacez `src/main.rs` par :

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    println!("Vitesse écoute sur http://localhost:3000");
    app.run(3000)
}
```

Lancez-le avec `cargo run`, puis dans un autre terminal :

```sh
curl http://localhost:3000
```

```text
Hello World!
```

Ce que fait chaque ligne :

- `use vitesse::prelude::*;` importe les types dont vous vous servirez tout le temps : `App`, `Request`, `Json`, `Error`, `StatusCode`, etc.
- `App::new()` est l'équivalent de `express()`. `app` est déclarée `mut` parce qu'ajouter des routes la modifie.
- `app.get(chemin, handler)` enregistre une route. Le handler est une closure asynchrone. Elle reçoit la requête (ignorée ici avec `_`) et renvoie n'importe quoi qui puisse devenir une réponse. Un `&str` devient une `200` avec `Content-Type: text/plain`.
- `app.run(3000)` démarre le serveur sur le port 3000, sur toutes les interfaces réseau et avec tous les cœurs du processeur. Il bloque jusqu'à ce que vous appuyiez sur `Ctrl+C`. Il renvoie un `std::io::Result<()>`, d'où le type de retour de `main`. Si le port est déjà pris, le programme s'arrête avec l'erreur.

Avec Express, la même chose s'écrirait :

```js
const app = express();
app.get('/', (req, res) => res.send('Hello World!'));
app.listen(3000);
```

Avant chacune des étapes suivantes, arrêtez le serveur avec `Ctrl+C`, puis relancez-le avec `cargo run`.

## 3. Une route JSON

Ajoutez cette route sous la première :

```rust
app.get("/api/status", |_| async {
    json!({ "status": "ok", "framework": "Vitesse" })
});
```

`json!` construit une valeur JSON avec une syntaxe proche de JavaScript. La renvoyer envoie une `200` avec `Content-Type: application/json`.

```sh
curl http://localhost:3000/api/status
```

```json
{"framework":"Vitesse","status":"ok"}
```

> [!NOTE]
> Les clés reviennent triées par ordre alphabétique, car `json!` range les objets dans une table triée. Quand vous sérialisez votre propre structure avec `Json(...)` (étape 5), les champs gardent leur ordre de déclaration.

## 4. Une route avec un paramètre

```rust
app.get("/hello/:name", |req: Request| async move {
    let name = req.param("name").unwrap_or("inconnu");
    format!("Bonjour, {name} !")
});
```

- `:name` est un paramètre de chemin, comme dans Express. `req.param("name")` renvoie une `Option<&str>`, qui vaut `Some("Ada")` pour `/hello/Ada`. La valeur est déjà décodée : `/hello/Fran%C3%A7ois` donne `François`.
- Quand le handler utilise la requête, écrivez `|req: Request| async move { ... }`. `move` déplace la requête dans le bloc asynchrone, qui peut alors s'en servir. Sinon, `|_| async { ... }` suffit.
- `format!` construit une `String`, qui est elle aussi une réponse `text/plain` valide.

```sh
curl http://localhost:3000/hello/Ada
```

```text
Bonjour, Ada !
```

Besoin d'un nombre ? `req.param_as::<u64>("id")?` convertit le paramètre et répond de lui-même `400 Bad Request` si ce n'est pas un nombre. Voir [Routage](routing.md).

## 5. Lire un corps JSON

Ajoutez maintenant une route `POST /users`. Elle reçoit `{"name": "Ada"}` et répond `201 Created` avec le nouvel utilisateur. Au-dessus de `main`, ajoutez les types et le handler :

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn create_user(req: Request) -> vitesse::Result<(StatusCode, Json<User>)> {
    let input: NewUser = req.json().await?;
    if input.name.trim().is_empty() {
        return Err(Error::bad_request("le nom ne doit pas être vide"));
    }
    let user = User { id: 1, name: input.name };
    Ok((StatusCode::CREATED, Json(user)))
}
```

Puis enregistrez-le dans `main` :

```rust
app.post("/users", create_user);
```

- Un handler peut être une `async fn` nommée qui prend une `Request`. C'est plus lisible dès qu'il dépasse quelques lignes.
- `req.json().await` lit le corps et le désérialise dans le type demandé (`NewUser`). Rien à installer, pas d'équivalent de `express.json()` : le corps est analysé au moment où vous le demandez. Si le JSON est invalide ou qu'un champ manque, `?` renvoie une `400 Bad Request`. Si le corps dépasse la limite (1 Mio par défaut), il renvoie `413 Payload Too Large`.
- `vitesse::Result<T>` est un raccourci pour `Result<T, vitesse::Error>`. `Error::bad_request(...)` crée une erreur `400`, dont le message est envoyé au client sous la forme `{"error": "..."}`.
- `(StatusCode::CREATED, Json(user))` envoie la structure en JSON avec le statut `201`. C'est l'équivalent de `res.status(201).json(user)`.

Essayez une requête valide, un nom vide et un champ manquant :

```sh
curl -i -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"name":"Ada"}'
```

```http
HTTP/1.1 201 Created
content-type: application/json
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 21

{"id":1,"name":"Ada"}
```

```sh
curl -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"name":""}'
# {"error":"le nom ne doit pas être vide"}

curl -X POST http://localhost:3000/users \
  -H 'content-type: application/json' -d '{"nom":"Ada"}'
# {"error":"invalid JSON: missing field `name` at line 1 column 13"}
```

> [!TIP]
> Vous pouvez aussi écrire ce handler sous forme de closure. Une closure n'a pas de type de retour déclaré, il faut donc préciser le type d'erreur sur le `Ok`, avec `Ok::<_, Error>((StatusCode::CREATED, Json(user)))`. Une fonction nommée évite cela.

## 6. Journaliser chaque requête

Ajoutez cette ligne juste après `App::new()` :

```rust
app.middleware(middleware::logger());
```

`app.middleware(...)` est le `app.use(fn)` d'Express. Un middleware global s'exécute pour chaque requête, 404 comprises. `middleware::logger()` fonctionne comme `morgan('dev')` et affiche une ligne par requête :

```text
GET /hello/Ada 200 0.007 ms
POST /users 201 0.008 ms
```

Les middlewares globaux s'exécutent dans l'ordre où vous les ajoutez. Écrire le vôtre prend quelques lignes, voir [Middlewares](middleware.md).

## 7. Lire le port dans l'environnement

Beaucoup d'hébergeurs (Heroku, Render, Railway, etc.) indiquent à votre application le port à écouter via la variable d'environnement `PORT`. Remplacez les deux dernières lignes de `main` par :

```rust
let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
println!("Vitesse écoute sur http://localhost:{port}");
app.run(port)
```

C'est l'équivalent de `app.listen(process.env.PORT || 3000)`. `app.run` accepte un numéro de port, une chaîne comme `"3000"` ou `"127.0.0.1:8080"`, une `SocketAddr`, etc. Vous pouvez donc lui passer telle quelle la `String` lue dans l'environnement.

## Le programme complet

```rust
use serde::{Deserialize, Serialize};
use vitesse::prelude::*;

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn create_user(req: Request) -> vitesse::Result<(StatusCode, Json<User>)> {
    let input: NewUser = req.json().await?;
    if input.name.trim().is_empty() {
        return Err(Error::bad_request("le nom ne doit pas être vide"));
    }
    let user = User { id: 1, name: input.name };
    Ok((StatusCode::CREATED, Json(user)))
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.middleware(middleware::logger());

    app.get("/", |_| async { "Hello World!" });

    app.get("/api/status", |_| async {
        json!({ "status": "ok", "framework": "Vitesse" })
    });

    app.get("/hello/:name", |req: Request| async move {
        let name = req.param("name").unwrap_or("inconnu");
        format!("Bonjour, {name} !")
    });

    app.post("/users", create_user);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    println!("Vitesse écoute sur http://localhost:{port}");
    app.run(port)
}
```

## Lancer en mode release

```sh
cargo run --release
```

Le premier build release prend un peu plus de temps, mais le serveur est ensuite beaucoup plus rapide (ajoutez aussi le profil release recommandé dans [Installation](installation.md)). Pour utiliser un autre port :

```sh
PORT=8080 cargo run --release
```

Vitesse gère aussi pour vous les routes inconnues et les mauvaises méthodes :

```sh
curl -i http://localhost:3000/nope
```

```http
HTTP/1.1 404 Not Found
content-type: application/json
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 28

{"error":"Cannot GET /nope"}
```

```sh
curl -i -X DELETE http://localhost:3000/users
```

```http
HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: POST, OPTIONS
date: Sat, 10 Oct 2026 09:00:00 GMT
content-length: 30

{"error":"Method Not Allowed"}
```

## Et ensuite ?

- [Routage](routing.md) : jokers, paramètres typés, priorité des routes, 404 et 405.
- [Lire la requête](requests.md) : query string, en-têtes, cookies, formulaires et uploads.
- [Envoyer la réponse](responses.md) : statuts, en-têtes, cookies, redirections et fichiers.
- [État partagé](state.md) : remplacez le `id: 1` écrit en dur par un stockage en mémoire ou un pool de base de données.
- [Gestion des erreurs](errors.md), [Tests](testing.md), puis déployez avec [Docker](docker.md).
