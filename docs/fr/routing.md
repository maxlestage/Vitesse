# Routage

Le routage choisit le handler qui répond à une requête, d'après sa méthode et son chemin. Vitesse reprend le vocabulaire d'Express (`app.get`, `:id`, `app.all`), mais son routeur est un arbre : l'ordre de déclaration des routes n'a pas d'importance, et une mauvaise méthode reçoit une vraie `405`.

## Déclarer des routes

Chaque méthode HTTP a sa fonction. Elles existent sur `App` et sur [`Router`](routers.md) :

| Vitesse | Express | Méthodes acceptées |
|---|---|---|
| `app.get(chemin, handler)` | `app.get()` | `GET`, et aussi `HEAD` (voir plus bas) |
| `app.post(chemin, handler)` | `app.post()` | `POST` |
| `app.put(chemin, handler)` | `app.put()` | `PUT` |
| `app.patch(chemin, handler)` | `app.patch()` | `PATCH` |
| `app.delete(chemin, handler)` | `app.delete()` | `DELETE` |
| `app.head(chemin, handler)` | `app.head()` | `HEAD` |
| `app.options(chemin, handler)` | `app.options()` | `OPTIONS` |
| `app.all(chemin, handler)` | `app.all()` | toutes les méthodes |
| `app.route(méthode, chemin, handler)` | `app[méthode]()` | la méthode passée, y compris non standard |

```rust
app.get("/articles", |_| async { "liste des articles" });
app.post("/articles", |_| async { (201, "article créé") });
app.put("/articles/:id", |_| async { "article remplacé" });
app.patch("/articles/:id", |_| async { "article modifié" });
app.delete("/articles/:id", |_| async { StatusCode::NO_CONTENT });
app.all("/ping", |_| async { "pong" });

// Une méthode non standard
let purge = Method::from_bytes(b"PURGE").unwrap();
app.route(purge, "/cache", |_| async { "cache vidé" });
```

> [!WARNING]
> `app.route` n'est pas le `app.route(chemin)` d'Express. Dans Vitesse, il enregistre une seule route pour la `Method` passée. Pour mettre plusieurs méthodes sur le même chemin, enchaînez plutôt les appels (voir plus bas).

> [!TIP]
> Les formulaires HTML ne savent envoyer que `GET` et `POST`. Pour atteindre une route `PUT` ou `DELETE` depuis un formulaire, changez la méthode dans un middleware global avec `req.set_method(...)`, comme le `method-override` d'Express : voir [Surcharger la méthode](requests.md#surcharger-la-méthode).

### Handlers

Un handler est une fonction ou une closure `async` qui prend une `Request` et renvoie n'importe quoi qui implémente `IntoResponse` : du texte, `Json(...)`, un tuple `(statut, corps)`, un `Result`, etc. (voir [Envoyer la réponse](responses.md)).

```rust
// Une closure qui ignore la requête
app.get("/", |_| async { "Accueil" });

// Une closure qui s'en sert : précisez son type et ajoutez `async move`
app.get("/hello/:name", |req: Request| async move {
    format!("Bonjour, {} !", req.param("name").unwrap_or("inconnu"))
});

// Une fonction nommée
async fn list_users(_req: Request) -> Json<Vec<&'static str>> {
    Json(vec!["ada", "grace"])
}
app.get("/users", list_users);
```

### Enchaîner les appels

Chaque fonction de routage renvoie `&mut Self`, vous pouvez donc enchaîner les appels. Cela ressemble un peu au `app.route('/users').get(...).post(...)` d'Express :

```rust
app.get("/users", list_users)
    .post("/users", create_user)
    .get("/users/:id", show_user)
    .delete("/users/:id", delete_user);
```

Enchaînez sur votre variable, après `let mut app = App::new();`. Enchaîner directement sur `App::new()` vous laisserait une référence vers une valeur temporaire.

## Paramètres de chemin

Un segment qui commence par `:` capture cette partie du chemin. `req.param(nom)` le lit et renvoie une `Option<&str>` :

```rust
app.get("/users/:id/posts/:post", |req: Request| async move {
    let user = req.param("id").unwrap();
    let post = req.param("post").unwrap();
    format!("article {post} de l'utilisateur {user}")
});
```

`GET /users/42/posts/7` répond `article 7 de l'utilisateur 42`. Le `unwrap()` ne peut pas échouer ici, puisque la route ne correspond que si les deux paramètres sont présents.

- `req.params()` parcourt toutes les paires `(nom, valeur)`, dans l'ordre.
- Les valeurs sont décodées : `/users/Fran%C3%A7ois` donne `François`. Un `+` reste un `+`, car il ne signifie « espace » que dans la query string. `req.path()` renvoie toujours le chemin brut.
- Un paramètre ne correspond jamais à une valeur vide ni à quoi que ce soit contenant un `/`.
- Un nom ne peut contenir que des lettres, des chiffres et `_`.

La syntaxe est celle d'Express (version 4) : quelques formes de plus fonctionnent donc, plusieurs paramètres dans un même segment, les contraintes et les paramètres optionnels.

### Plusieurs paramètres dans un segment

Un segment peut mélanger du texte et des paramètres, du moment qu'un peu de texte les sépare :

```rust
app.get("/flights/:from-:to", |req: Request| async move {
    let from = req.param("from").unwrap();
    let to = req.param("to").unwrap();
    format!("de {from} à {to}")
});
app.get("/files/:name.:ext", show_file);
app.get("/api/v:version/status", api_status);
app.get("/img/:id.png", show_png);
```

| Requête | Route | Paramètres |
|---|---|---|
| `/flights/LAX-SFO` | `/flights/:from-:to` | `from = "LAX"`, `to = "SFO"` |
| `/files/report.pdf` | `/files/:name.:ext` | `name = "report"`, `ext = "pdf"` |
| `/files/archive.tar.gz` | `/files/:name.:ext` | `name = "archive"`, `ext = "tar.gz"` |
| `/api/v2/status` | `/api/v:version/status` | `version = "2"` |
| `/img/42.png` | `/img/:id.png` | `id = "42"` |
| `/files/README` ou `/api/2/status` | aucune : `404` | |

- Chaque paramètre prend au moins un caractère, et tous sauf le dernier en prennent le moins possible, comme dans Express : voilà pourquoi `archive.tar.gz` donne `archive` et `tar.gz`.
- Deux paramètres doivent être séparés par du texte : `/:from:to` est refusé au démarrage, puisque rien n'indique où `from` se termine.

### Contraintes avec des expressions régulières

Une expression régulière entre parenthèses, juste après le nom, restreint les valeurs qu'un paramètre accepte. Une requête dont la valeur ne correspond pas ne correspond tout simplement pas à la route : une autre route peut la prendre, ou elle reçoit une `404`.

```rust
app.get(r"/users/:id(\d+)", |req: Request| async move {
    let id: u64 = req.param_as("id")?;
    Ok::<_, Error>(format!("utilisateur n°{id}"))
});
app.get("/docs/:lang(en|fr|es)/:page", show_doc);
app.get(r"/posts/:year(\d{4})-:slug", show_post);
```

| Requête | Réponse |
|---|---|
| `/users/42` | `utilisateur n°42` |
| `/users/ada` | `404` (ou une autre route, voir [plus bas](#la-route-la-plus-précise-lemporte)) |
| `/docs/fr/routing` | `show_doc`, avec `lang = "fr"` |
| `/docs/de/routing` | `404` |
| `/posts/2024-hello-world` | `show_post`, avec `year = "2024"` et `slug = "hello-world"` |

- En Rust, écrivez le chemin sous forme de chaîne brute, `r"/users/:id(\d+)"`, pour que les barres obliques inverses restent telles quelles. Dans une chaîne ordinaire, doublez-les : `"/users/:id(\\d+)"`.
- L'expression doit correspondre à la valeur entière : `:lang(en|fr)` accepte `fr`, pas `fra`. Inutile d'ajouter `^` et `$`.
- Elle ne peut pas contenir de `/`, puisqu'un paramètre n'en contient jamais.
- Elle est testée sur la valeur telle qu'elle apparaît dans l'URL, avant décodage : pour elle, `%20` fait trois caractères.
- Vitesse utilise [regex-lite](https://crates.io/crates/regex-lite) : la syntaxe habituelle (classes, `\d`, `\w`, `|`, répétitions, `(?i)` pour ignorer la casse) fonctionne, mais `\d` et `\w` ne couvrent que l'ASCII, et les classes Unicode comme `\p{L}` ne sont pas disponibles. Les expressions sont compilées une seule fois, au démarrage, et ne s'exécutent que pour les paramètres qui ont une contrainte.
- Une expression invalide arrête le programme au démarrage : `invalid route: invalid regular expression…`.

Une contrainte choisit la route, et `req.param_as` convertit toujours la valeur : ici, un `u64` trop grand pour tenir reçoit quand même une `400`.

> [!NOTE]
> Express 5 a retiré les expressions régulières des chemins, et actix-web les écrit `{id:\d+}`. Vitesse garde la syntaxe d'Express 4, `:id(\d+)`.

### Paramètres optionnels

Un `?` après un paramètre le rend optionnel. Une seule route répond alors à plusieurs chemins :

```rust
app.get("/archive/:year?/:month?", |req: Request| async move {
    match (req.param("year"), req.param("month")) {
        (Some(year), Some(month)) => format!("archives de {month}/{year}"),
        (Some(year), None) => format!("archives de {year}"),
        _ => "toutes les archives".to_owned(),
    }
});
```

| Requête | `req.param("year")` | `req.param("month")` |
|---|---|---|
| `/archive` | `None` | `None` |
| `/archive/2024` | `Some("2024")` | `None` |
| `/archive/2024/05` | `Some("2024")` | `Some("05")` |

- Seuls les derniers segments d'un chemin peuvent être optionnels (`/:lang?/about` est refusé), et seulement un paramètre qui occupe un segment entier (`/files/:name.:ext?` est refusé aussi).
- Une contrainte peut aussi être optionnelle : `r"/users/:id(\d+)?"` correspond à `/users` et à `/users/7`, pas à `/users/ada`.
- La route compte comme une route par chemin possible : `/archive/:year?` et une route `/archive` distincte pour la même méthode sont [en double](#routes-invalides-ou-en-double).

## Paramètres typés

`req.param_as::<T>(nom)` convertit un paramètre dans n'importe quel type qui implémente `FromStr` : entiers, `bool`, `f64`, `IpAddr`, vos propres types, etc. Si le paramètre manque ou ne se convertit pas, il renvoie une erreur `400 Bad Request`. Un `?` suffit donc :

```rust
async fn show_user(req: Request) -> vitesse::Result<String> {
    let id: u64 = req.param_as("id")?;
    Ok(format!("utilisateur n°{id}"))
}

app.get("/users/:id", show_user);
```

```text
GET /users/42   → 200 utilisateur n°42
GET /users/abc  → 400 {"error":"invalid parameter 'id': 'abc'"}
GET /users/-1   → 400 (un u64 ne peut pas être négatif)
```

Pour répondre `404` au lieu de `400` à `/users/abc`, ou pour laisser une autre route la prendre, ajoutez une [contrainte](#contraintes-avec-des-expressions-régulières) à la route : `r"/users/:id(\d+)"`.

Pour vos propres types, implémentez `FromStr` :

```rust
use std::str::FromStr;

enum Format {
    Json,
    Csv,
}

impl FromStr for Format {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            _ => Err(()),
        }
    }
}

app.get("/export/:format", |req: Request| async move {
    let message = match req.param_as::<Format>("format")? {
        Format::Json => "export en JSON",
        Format::Csv => "export en CSV",
    };
    Ok::<_, Error>(message)
});
```

`GET /export/csv` répond `export en CSV`, et `GET /export/xml` reçoit une `400`.

### Tous les paramètres d'un coup : `params_as`

`req.params_as::<T>()` désérialise tous les paramètres de la route dans une structure, en convertissant nombres et booléens, comme le `Path<T>` d'axum. Un paramètre optionnel absent va dans une `Option` :

```rust
#[derive(serde::Deserialize)]
struct CheminArticle {
    user: String,
    post: u64,
    draft: Option<bool>,
}

app.get("/users/:user/posts/:post/:draft?", |req: Request| async move {
    let chemin: CheminArticle = req.params_as()?; // 400 si `post` n'est pas un nombre
    Ok::<_, Error>(format!("article n°{} de {}", chemin.post, chemin.user))
});
```

## Jokers

Un segment `*nom` capture tout le reste du chemin, barres obliques comprises. Express 5 utilise la même syntaxe :

```rust
app.get("/files/*path", |req: Request| async move {
    format!("vous avez demandé {}", req.param("path").unwrap())
});
```

| Requête | `req.param("path")` |
|---|---|
| `/files/rapport.pdf` | `"rapport.pdf"` |
| `/files/2024/t1/rapport.pdf` | `"2024/t1/rapport.pdf"` |
| `/files` ou `/files/` | `""` (vide) |

- Un joker doit être le dernier segment. `/files/*path/edit` est refusé au démarrage.
- Un joker anonyme `*` fonctionne aussi. Sa valeur est rangée sous le nom `"*"`, que vous lisez avec `req.param("*")`.
- `/*` tout seul attrape tous les chemins qu'aucune route plus précise ne prend.

## Comment les routes sont choisies

### La route la plus précise l'emporte

Quand plusieurs routes pourraient correspondre, Vitesse les compare segment par segment, du type de segment le plus précis au moins précis :

1. un segment statique (`/users/new`) ;
2. un segment avec une contrainte ou avec du texte autour de ses paramètres (`/users/:id(\d+)`, `/img/:id.png`) ;
3. un simple paramètre (`/users/:name`) ;
4. un joker (`/users/*rest`).

L'ordre dans lequel vous déclarez les routes n'a pas d'importance, alors que dans Express, c'est la première route enregistrée qui gagne. Une route avec une contrainte et une route avec un simple paramètre peuvent donc occuper la même position :

```rust
app.get("/users/new", |_| async { "formulaire de création" });
app.get(r"/users/:id(\d+)", |_| async { "un utilisateur, par identifiant" });
app.get("/users/:name", |_| async { "un utilisateur, par nom" });
app.get("/users/*rest", |_| async { "tout le reste sous /users" });
```

| Requête | Route utilisée |
|---|---|
| `/users/new` | `/users/new` (le statique bat tout le reste) |
| `/users/42` | `/users/:id(\d+)` (la contrainte correspond) |
| `/users/ada` | `/users/:name` (la contrainte ne correspond pas) |
| `/users/42/settings` | `/users/*rest` |
| `/users` | `/users/*rest` (joker vide) |

Si une branche ne mène nulle part, le routeur revient en arrière et essaie l'option suivante. Avec `/a/:x/c` et `/a/b/d`, une requête vers `/a/b/c` correspond à `/a/:x/c` avec `x = "b"`.

L'ordre de déclaration ne compte qu'entre deux segments du deuxième type à la même position, par exemple `/:id(\d+)` et `/:slug([a-z-]+)` : ils sont essayés dans l'ordre où vous les avez déclarés.

### Barre oblique finale, casse et query string

- La barre oblique finale est ignorée : `/users/` correspond à `/users`. Express fait de même par défaut.
- Les chemins sont **sensibles à la casse** par défaut : `/Users` ne correspond pas à `/users`. Express, lui, ignore la casse par défaut ; pour faire de même, appelez `app.case_sensitive(false)`.
- La query string ne joue aucun rôle : `/search?q=rust` correspond à `/search`.

```rust
let mut app = App::new();
app.case_sensitive(false);
app.get("/users/:id", show_user); // répond aussi à /Users/Ada et à /USERS/Ada
```

Avec `app.case_sensitive(false)`, les segments statiques et le texte autour des paramètres (`.png` dans `/img/:id.png`) correspondent quelle que soit leur casse, préfixes de montage compris. Les paramètres gardent la casse de la requête (`req.param("id")` vaut `"Ada"`), et les contraintes ne sont pas modifiées : ajoutez `(?i)` à l'expression si elle doit ignorer la casse elle aussi. Seules les lettres ASCII sont concernées.

### Routes invalides ou en double

Vitesse détecte les erreurs de déclaration au démarrage de l'application, pas à l'arrivée d'une requête. `app.get(...)` panique avec un message explicite si :

- le chemin ne commence pas par `/` ;
- il contient un segment vide (`/a//b`) ;
- un nom de paramètre est invalide, ou deux paramètres se suivent sans texte entre eux (`/:a:b`) ;
- une contrainte n'est pas une expression régulière valide, ou sa parenthèse n'est pas fermée ;
- un paramètre optionnel n'est pas à la fin du chemin, ou n'occupe pas un segment entier ;
- un joker n'est pas le dernier segment ;
- la même méthode est enregistrée deux fois sur le même chemin.

Deux motifs qui ne diffèrent que par le nom de leurs paramètres sont la même route, donc `/users/:id` et `/users/:name` entrent en conflit :

```text
thread 'main' panicked at src/main.rs:12:9:
duplicate route: GET /users/:name is already defined
```

`/users/:id(\d+)` et `/users/:name` n'entrent pas en conflit, puisqu'un seul des deux a une contrainte.

## La route correspondante : `req.route()`

`req.route()` renvoie le motif de la route qui a correspondu, préfixe de montage compris, comme le `req.route.path` d'Express (ou le `MatchedPath` d'axum) :

```rust
app.get("/users/:id", |req: Request| async move {
    format!("route : {}", req.route().unwrap_or("?")) // "route : /users/:id"
});
```

| Requête | `req.path()` | `req.route()` |
|---|---|---|
| `GET /users/42` | `/users/42` | `Some("/users/:id")` |
| `GET /api/v1/items/a/b`, routeur monté sur `/api/:version` avec une route `/items/*path` | `/api/v1/items/a/b` | `Some("/api/:version/items/*path")` |
| `GET /archive/2024`, route `/archive/:year?/:month?` | `/archive/2024` | `Some("/archive/:year?/:month?")` |
| `DELETE /users/42`, sans route `DELETE` (`405`) | `/users/42` | `None` |
| `GET /nope` (`404`) | `/nope` | `None` |

Le motif est celui que vous avez déclaré, contraintes et `?` compris : `r"/users/:id(\d+)"` donne `/users/:id(\d+)`.

C'est ce qu'il vous faut dans les journaux et les métriques : `/users/:id` donne une seule ligne dans un tableau de bord, là où le chemin brut en donnerait une par utilisateur. Les middlewares globaux s'exécutent avant le routage : à cet endroit, `req.route()` cherche donc la route à la demande. Appelez-la avant `next.run(req)`, qui prend la requête ; le résultat est une `&'static str`, vous pouvez donc le garder ensuite :

```rust
use std::time::Instant;

app.middleware(|req: Request, next: Next| async move {
    let route = req.route().unwrap_or("aucune route");
    let start = Instant::now();
    let res = next.run(req).await;
    println!("{route} {} {:?}", res.status_code(), start.elapsed());
    res
});
```

Dans les middlewares de routeur, les middlewares de route et les handlers, la route est déjà connue et `req.route()` ne coûte rien.

## Routes nommées et génération d'URL

Écrire `/users/42` à la main dans chaque lien et chaque redirection fonctionne, jusqu'au jour où la route change. Donnez plutôt un nom à la route, avec `.name(...)` juste après elle, et laissez Vitesse construire ses chemins, comme le `url_for` d'actix-web :

```rust
async fn show_user(req: Request) -> String {
    format!("utilisateur {}", req.param("id").unwrap_or("?"))
}

async fn create_user(req: Request) -> vitesse::Result<Redirect> {
    let id = 42; // l'identifiant du nouvel utilisateur
    let link = req.path_for("user", &[&id])?; // "/users/42"
    Ok(Redirect::see_other(link))
}

app.get("/users/:id", show_user).name("user");
app.post("/users", create_user);
```

- `req.path_for(nom, &[valeurs])` renvoie le chemin de la route nommée, avec ses paramètres remplis **dans l'ordre**. Les valeurs peuvent être n'importe quoi qui implémente `Display` : `&[&42]`, `&[&"LAX", &"SFO"]`, `&[]` pour une route sans paramètres.
- `req.url_for(nom, &[valeurs])` renvoie l'URL absolue, `https://example.com/users/42` : le même chemin, précédé de `req.protocol()` et de `req.host()`. Derrière un proxy, déclarez-le avec [`app.trust_proxy`](production.md#connaître-ladresse-ip-du-client) pour qu'ils reflètent ce que voit le client.

| Route | Appel | Résultat |
|---|---|---|
| `/users/:id` | `path_for("user", &[&42])` | `/users/42` |
| `/users/:id` | `path_for("user", &[&"ada lovelace"])` | `/users/ada%20lovelace` |
| `/flights/:from-:to` | `path_for("flight", &[&"LAX", &"SFO"])` | `/flights/LAX-SFO` |
| `/files/*path` | `path_for("file", &[&"docs/report 2024.pdf"])` | `/files/docs/report%202024.pdf` |
| `/archive/:year?/:month?` | `path_for("archive", &[&2024])` | `/archive/2024` |
| `/users/:id` | `url_for("user", &[&42])` | `https://example.com/users/42` |

- Les valeurs sont encodées pour l'URL : une espace devient `%20` et un `/` devient `%2F`. Seul un joker garde ses `/`, puisqu'il représente plusieurs segments.
- Les paramètres optionnels de la fin peuvent être omis, tout comme un joker : `path_for("archive", &[])` donne `/archive`.
- Les contraintes sont vérifiées : `path_for` refuse `"ada"` pour `r"/users/:id(\d+)"`.
- Le chemin est construit au moment de l'appel : il peut donc aller n'importe où, dans un en-tête `Location`, un lien dans `Html(...)`, un champ JSON.

Les noms sont vérifiés à deux moments :

- **Au démarrage**, `name` panique si aucune route ne vient juste avant (un `mount`, par exemple), ou si le nom est déjà pris : les noms sont uniques dans toute l'application.
- **À l'appel**, `path_for` et `url_for` renvoient une erreur `500` si le nom est inconnu, si une valeur manque ou est en trop, si une valeur ne respecte pas sa contrainte, ou (`url_for` seulement) si la requête n'a pas d'hôte. Le client ne voit que `{"error":"Internal Server Error"}` ; la cause (`cannot build the URL of 'user': missing a value for ':id'`) est affichée sur la sortie d'erreur, comme pour les autres erreurs serveur.

Les routes déclarées dans un [routeur](routers.md#routes-nommées-dans-les-routeurs) peuvent aussi être nommées. Le nom appartient à toute l'application, et le chemin inclut le préfixe de montage : ses paramètres viennent en premier dans la liste des valeurs.

## HEAD, OPTIONS et 405

Vitesse se charge des comportements HTTP standard :

- **HEAD** : une requête `HEAD` utilise la route `GET` et reçoit les mêmes en-têtes sans le corps, sauf si vous enregistrez `app.head(...)`.
- **405 Method Not Allowed** : quand le chemin existe mais pas pour cette méthode, la réponse est une `405` avec un en-tête `Allow` qui liste les méthodes disponibles. Express répond `404` dans ce cas.
- **OPTIONS** : sans route `app.options(...)` explicite, une requête `OPTIONS` reçoit `204 No Content` avec le même en-tête `Allow`.

```rust
app.get("/items", list_items);
app.post("/items", create_item);
```

```http
DELETE /items HTTP/1.1

HTTP/1.1 405 Method Not Allowed
content-type: application/json
allow: GET, HEAD, POST, OPTIONS

{"error":"Method Not Allowed"}
```

Sur le même chemin, `OPTIONS /items` reçoit une `204 No Content` avec `allow: GET, HEAD, POST, OPTIONS`.

`app.all(...)` accepte toutes les méthodes sur son chemin, il ne produit donc jamais de `405`. Si le même chemin a aussi une route pour une méthode précise, c'est elle qui l'emporte sur `all`.

> [!TIP]
> Les navigateurs envoient leurs requêtes CORS préliminaires en `OPTIONS`. Pour y répondre, utilisez `middleware::cors()`, qui les traite avant le routage. Voir [Middlewares](middleware.md).

## Fallback : personnaliser la 404

Quand aucune route ne correspond au chemin, Vitesse répond `404` avec `{"error":"Cannot GET /nope"}`, le même message qu'Express. `app.fallback` remplace ce handler. Il joue le rôle du dernier `app.use((req, res) => ...)` d'une application Express :

```rust
app.fallback(|req: Request| async move {
    (404, format!("Rien ici : {}", req.path()))
});
```

Un usage courant est l'application monopage (SPA) : envoyer `index.html` pour tout chemin inconnu et laisser le routeur du front-end prendre le relais.

```rust
app.fallback(|_| async { res::file("dist/index.html").await });
```

- Le fallback s'exécute après les middlewares globaux, comme n'importe quelle route.
- Il n'est pas appelé pour une `405`, puisque le chemin existe, simplement pas pour cette méthode.
- Un [routeur](routers.md#le-fallback-dun-routeur) peut avoir son propre fallback pour les chemins sous son préfixe, avec `router.fallback(handler)` : des `404` en JSON sous `/api`, par exemple, et une page HTML partout ailleurs.
- Pour changer d'un coup le format de *toutes* les réponses d'erreur (404, 405, 400, 500, etc.), utilisez plutôt `app.on_error`. Voir [Gestion des erreurs](errors.md).

## Callbacks de paramètres et liaison de modèles

`app.param(nom, middleware)` exécute un middleware avant le handler de chaque route qui a le paramètre `nom`, après le routage : c'est le `app.param()` d'Express. Chargez ou vérifiez ce que désigne le paramètre une seule fois, pour toutes les routes qui l'utilisent :

```rust
app.param("id", |req: Request, next: Next| async move {
    match req.param_as::<u32>("id") {
        Ok(id) if id > 0 => next.run(req).await,
        _ => Error::bad_request("identifiant invalide").into_response(),
    }
});
app.get("/users/:id", afficher_utilisateur); // vérifié
app.get("/posts/:id", afficher_article);     // vérifié
app.get("/a-propos", a_propos);             // non concerné
```

`app.bind_param(nom, chargeur)` est la liaison de modèles de Laravel (route model binding), construite dessus : le chargeur transforme la valeur du paramètre en une valeur à vous, que le handler lit avec `req.get::<T>()`. Quand le chargeur renvoie une erreur, elle devient la réponse et le handler n'est pas appelé :

```rust
#[derive(Clone)]
struct Utilisateur {
    id: u32,
    nom: String,
}

app.bind_param("user", |req: &Request, id: &str| {
    let db = req.state::<Db>(); // `&'static` : le future peut la garder
    let id = id.parse::<u32>();
    async move {
        let id = id.map_err(|_| Error::bad_request("identifiant invalide"))?;
        db.trouver(id).await.ok_or_else(|| Error::not_found("utilisateur inconnu"))
    }
});

app.get("/users/:user", |req: Request| async move {
    let u = req.get::<Utilisateur>().expect("lié par bind_param");
    format!("n°{} {}", u.id, u.nom)
});
app.delete("/users/:user", supprimer); // le même chargeur, gratuitement
```

- Les callbacks s'appliquent aux routes ajoutées avant ou après eux, dans l'ordre des paramètres du chemin ; les routes sans le paramètre ne paient rien.
- Comme dans Express, ils appartiennent à l'application ou au routeur où ils sont déclarés : `router.param(…)` couvre les routes de ce routeur, et ceux de l'application ne s'appliquent pas aux routes d'un routeur monté ou d'un [groupe](routers.md#groupes-de-routes--appgroup) : déclarez-les dans le groupe, avec `groupe.param(…)`.
- Ils passent après les middlewares globaux et ceux des routeurs (l'authentification d'abord, puis le chargement), et avant les middlewares propres à la route (`.with`).

## Organiser ses routes

Quand l'application grandit, regroupez les routes liées dans un `Router` et montez-le sous un préfixe, comme avec `express.Router()` :

```rust
let mut api = Router::new();
api.get("/users", list_users).post("/users", create_user);

app.mount("/api", api); // GET /api/users, POST /api/users
```

`app.group(préfixe, |groupe| { … })` fait de même sans nommer le routeur, et les plugins (`app.register`) emballent des routes pour les réutiliser. Les routeurs peuvent avoir leurs propres middlewares et s'imbriquer, voir [Routeurs](routers.md). Pour un middleware sur une seule route (`handler.with(auth)`), voir [Middlewares](middleware.md).
