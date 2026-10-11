# Cookies signés et sessions

Un cookie garde un peu de données dans le navigateur du visiteur, qui le renvoie avec chaque requête. Avec la feature `session`, Vitesse sait faire deux choses de plus avec les cookies : les **signer**, pour que les visiteurs puissent les lire mais pas les modifier (comme `cookie-parser` avec un secret dans Express), et tenir des **sessions**, des données gardées sur le serveur pour chaque visiteur pendant que le navigateur ne garde qu'un identifiant aléatoire (comme `express-session` dans Express).

## Activer la feature `session`

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["session"] }
```

Elle ajoute `app.cookie_secret(...)`, `req.sign_cookie(...)`, `req.signed_cookie(...)`, `middleware::session(...)`, `req.session()` et le module `vitesse::session`.

## Le secret

Pour signer, il faut un secret : une longue chaîne aléatoire que seul le serveur connaît. Générez-en un, une fois pour toutes :

```sh
openssl rand -base64 48
```

Puis passez-le à l'application par une variable d'environnement :

```rust
let mut app = App::new();
app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET n'est pas défini"));
```

- Le secret doit faire au moins 32 octets : un secret plus court fait paniquer `cookie_secret` au démarrage.
- Gardez-le hors du code et hors de Git. Quiconque le connaît peut falsifier vos cookies.
- Toutes les instances de l'application ont besoin du même secret. Le changer invalide tous les cookies signés avec l'ancien : les visiteurs perdent leurs cookies signés et leurs sessions, comme s'ils s'étaient déconnectés.

## Cookies signés

`req.sign_cookie(cookie)` signe un cookie avant que vous ne l'envoyiez avec `.cookie(...)`, et `req.signed_cookie(nom)` le relit et vérifie la signature :

```rust
use std::time::Duration;
use vitesse::prelude::*;

let mut app = App::new();
app.cookie_secret(std::env::var("COOKIE_SECRET").expect("COOKIE_SECRET n'est pas défini"));

app.post("/theme/:name", |req: Request| async move {
    let theme = req.param("name").unwrap_or("light").to_owned();
    let cookie = Cookie::new("theme", theme).max_age(Duration::from_secs(365 * 24 * 3600));
    res::send_status(204).cookie(req.sign_cookie(cookie))
});

app.get("/", |req: Request| async move {
    // `None` si le cookie manque, ou si le visiteur l'a modifié.
    let theme = req.signed_cookie("theme").unwrap_or_else(|| "light".into());
    format!("thème : {theme}")
});
```

Le cookie part sous la forme `theme=dark.<signature>` : la valeur, un point, puis un HMAC-SHA256 du nom et de la valeur du cookie, calculé avec le secret. `req.signed_cookie("theme")` le calcule à nouveau : s'il correspond, vous obtenez `Some("dark")`, sans la signature ; sinon, vous obtenez `None`, comme s'il n'y avait pas de cookie. `req.cookie("theme")` renvoie toujours la valeur brute, signature comprise.

Ce que garantit la signature :

- **La valeur vient de vous.** Un visiteur qui remplace `dark` par `light` n'a pas la signature correspondante, et le cookie est ignoré.
- **La signature couvre aussi le nom** : la valeur d'un cookie ne peut donc pas se faire passer pour celle d'un autre.

Ce qu'elle ne fait pas :

- **Cacher la valeur.** Signé ne veut pas dire chiffré : n'importe qui peut lire `dark` dans les outils de développement du navigateur. Ne mettez ni secrets ni données personnelles dans un cookie, signé ou non.
- **La faire expirer ou la reprendre.** `Max-Age` n'est appliqué que par le navigateur : quelqu'un qui a gardé une copie d'un cookie signé peut le renvoyer plus tard, et il reste valide tant que le secret ne change pas. Pour tout ce qui doit pouvoir prendre fin à la demande, une connexion par exemple, utilisez une [session](#sessions).

`sign_cookie` panique (la requête reçoit donc une `500`) quand l'application n'a pas de secret, et `signed_cookie` renvoie `None`. La valeur est écrite telle quelle : limitez-vous aux lettres, aux chiffres et à `-_.`. Pour quelque chose de plus gros ou de structuré, utilisez une session.

## Sessions

Une session garde des données sur le serveur pour chaque visiteur : qui est connecté, un panier, un message à afficher sur la page suivante. Le navigateur ne garde qu'un cookie, `vitesse.sid`, qui contient un identifiant aléatoire qu'il renvoie avec chaque requête. Ajoutez le middleware `session`, puis lisez et écrivez la session avec `req.session()` :

```rust
use vitesse::prelude::*;
use vitesse::session::MemoryStore;

let mut app = App::new();
app.middleware(middleware::session(MemoryStore::new()));

app.get("/visits", |req: Request| async move {
    let session = req.session();
    let visits = session.get::<u32>("visits").unwrap_or(0) + 1;
    session.insert("visits", visits)?;
    Ok::<_, Error>(format!("visite n°{visits}"))
});
```

Chaque navigateur compte ses propres visites. Une session contient des valeurs nommées, stockées en JSON : tout type qui implémente `Serialize` peut y entrer, et en ressortir sous n'importe quel type qui implémente `Deserialize`.

| Méthode | `express-session` | Effet |
|---|---|---|
| `session.get::<T>(clé)` | `req.session.clé` | La valeur sous forme d'`Option<T>` ; `None` si elle manque ou est d'un autre type |
| `session.insert(clé, valeur)?` | `req.session.clé = valeur` | Enregistre une valeur ; un `Result`, une `Err` (`500`) si elle ne se sérialise pas |
| `session.remove(clé)` | `delete req.session.clé` | Retire une valeur et la renvoie (`Option<serde_json::Value>`) |
| `session.contains(clé)` | `'clé' in req.session` | Indique si la session contient cette clé |
| `session.clear()` | | Retire toutes les valeurs, en gardant la session |
| `session.is_empty()` | | Indique si la session ne contient aucune valeur |
| `session.regenerate()` | `req.session.regenerate()` | Un nouvel identifiant, les mêmes données : à appeler à la connexion |
| `session.destroy()` | `req.session.destroy()` | Supprime la session, dans le stockage et dans le navigateur : la déconnexion |
| `session.id()` | `req.sessionID` | L'identifiant, une `Option<String>` : `None` tant que la session n'a jamais été enregistrée |

- Ajoutez le middleware avec `app.middleware(...)`, pour que chaque route ait une session. `req.session()` panique (et la requête reçoit une `500`) sur une route que le middleware ne couvre pas.
- `get` renvoie une valeur possédée : demandez une `String`, pas une `&str`.
- `Session` est `Clone`, et un clone désigne la même session.
- Dans un handler [WebSocket](websocket.md#paramètres-en-têtes-et-état) (`app.ws`), `req.session()` contient la session telle qu'elle était quand la connexion a été acceptée. Les modifications faites là ne sont pas enregistrées, car le middleware a déjà envoyé sa réponse (la `101`) : modifiez la session dans une route ordinaire.

### Connexion et déconnexion

```rust
use serde::Deserialize;
use vitesse::prelude::*;
use vitesse::session::MemoryStore;

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}

async fn login(req: Request) -> vitesse::Result<Redirect> {
    let form: Login = req.form().await?;
    let Some(user_id) = check_password(&form.email, &form.password).await else {
        return Err(Error::unauthorized("e-mail ou mot de passe incorrect"));
    };
    let session = req.session();
    session.regenerate(); // un nouvel identifiant : celui que le visiteur avait jusqu'ici ne vaut plus rien
    session.insert("user_id", user_id)?;
    Ok(Redirect::see_other("/account"))
}

async fn account(req: Request) -> vitesse::Result<String> {
    let user_id: u64 = req
        .session()
        .get("user_id")
        .ok_or_else(|| Error::unauthorized("connectez-vous"))?;
    Ok(format!("Bonjour, utilisateur n°{user_id}"))
}

async fn logout(req: Request) -> Redirect {
    req.session().destroy();
    Redirect::see_other("/")
}

let mut app = App::new();
app.middleware(middleware::session(MemoryStore::new()));
app.post("/login", login);
app.get("/account", account);
app.post("/logout", logout);
```

`check_password` est votre propre fonction : elle cherche l'utilisateur et vérifie le hachage du mot de passe (avec argon2 ou bcrypt, par exemple), et renvoie une `Option<u64>`.

- **`regenerate()` à la connexion** déjoue la fixation de session : un attaquant qui a amené la victime à utiliser un identifiant de session qu'il connaît (sur un ordinateur partagé, par un sous-domaine) ne peut plus s'en servir une fois la victime connectée. Les données sont gardées, un panier par exemple.
- **`destroy()` à la déconnexion** supprime la session du stockage, pour qu'une copie du cookie ne vaille plus rien, et demande au navigateur de supprimer le cookie.
- La déconnexion est un `POST` : un `GET` peut être déclenché par n'importe quel lien ou image d'un autre site.
- Après `destroy()`, la session reste détruite jusqu'à la fin de la requête : une valeur insérée ensuite n'est pas enregistrée. Pour afficher un message après la déconnexion, mettez-le dans l'URL de la redirection (`/?bye`) ou dans un cookie ordinaire.

Pour protéger un groupe de routes, vérifiez la session dans un [middleware](middleware.md) et transmettez l'utilisateur aux handlers :

```rust
#[derive(Clone)]
struct UserId(u64);

async fn require_login(mut req: Request, next: Next) -> Response {
    match req.session().get::<u64>("user_id") {
        Some(id) => {
            req.set(UserId(id));
            next.run(req).await
        }
        None => Redirect::to("/login").into_response(),
    }
}

let mut account = Router::new();
account.middleware(require_login);
account.get("/", |req: Request| async move {
    let user = req.get::<UserId>().unwrap();
    format!("Bonjour, utilisateur n°{}", user.0)
});
app.mount("/account", account);
```

Les middlewares globaux s'exécutent avant ceux des routeurs : `require_login` trouve donc la session ouverte par `app.middleware(middleware::session(...))`.

### Options

`middleware::session(stockage)` renvoie un builder :

```rust
use std::time::Duration;
use vitesse::SameSite;

app.middleware(
    middleware::session(MemoryStore::new())
        .cookie_name("sid")
        .max_age(Duration::from_secs(30 * 24 * 3600)) // 30 jours
        .rolling(true)
        .same_site(SameSite::Strict),
);
```

| Option | Par défaut | Effet |
|---|---|---|
| `.cookie_name(nom)` | `"vitesse.sid"` | Le nom du cookie (`name` dans express-session) |
| `.max_age(durée)` | 1 jour | La durée de vie d'une session après sa dernière modification, dans le stockage et dans le navigateur (`cookie.maxAge`) |
| `.rolling(true)` | `false` | Prolonge la session à chaque requête, pour qu'elle se termine après `max_age` d'inactivité ; cela coûte une écriture dans le stockage par requête (`rolling`) |
| `.secure(bool)` | `Secure` quand la requête est arrivée en HTTPS | Force l'attribut `Secure`, ou le retire (`cookie.secure`) |
| `.same_site(SameSite::Strict)` | `SameSite::Lax` | L'attribut `SameSite` (`cookie.sameSite`) |
| `.domain("example.com")` | aucun : cet hôte seulement | L'attribut `Domain`, pour partager la session avec les sous-domaines (`cookie.domain`) |
| `.path("/app")` | `/` | L'attribut `Path` (`cookie.path`) |

### Quand le cookie est envoyé

Une session n'est enregistrée, et son cookie envoyé, que lorsqu'un handler la modifie : `insert`, `remove` d'une valeur présente, `clear` d'une session qui n'était pas vide, `regenerate`. Avec `rolling(true)`, une session existante est aussi enregistrée à chaque requête.

- Un visiteur qui ne fait que lire des pages ne reçoit pas de cookie et ne prend aucune place dans le stockage, comme avec `saveUninitialized: false` dans express-session.
- Lire la session ne renvoie pas le cookie, et ne prolonge pas non plus la session (sauf avec `rolling(true)`).
- La session est enregistrée quand la réponse sort du middleware, une fois que le handler (et les middlewares qui le suivent) ont rendu la main. Une modification faite plus tard, dans une tâche lancée avec `tokio::spawn`, n'est pas enregistrée.
- Un identifiant que le stockage ne connaît pas (expiré, supprimé, inventé par le client) ouvre une session vide, qui reçoit un nouvel identifiant si quelque chose y est écrit : Vitesse n'adopte jamais un identifiant choisi par le client.
- Deux requêtes simultanées du même visiteur travaillent chacune sur leur propre copie de la session, et la dernière à se terminer l'emporte, comme avec express-session. Gardez des sessions légères, et gardez dans votre base de données les données qui changent souvent.

### Attributs du cookie

Le cookie de session est :

- **`HttpOnly`** : le JavaScript de la page ne peut pas le lire, et une faille XSS ne peut donc pas voler la session.
- **`SameSite=Lax`** par défaut : le navigateur ne l'ajoute pas aux requêtes que déclenchent d'autres sites, sauf les simples liens (voir [CSRF](#csrf)).
- **`Secure` quand la requête est arrivée en HTTPS** : le navigateur ne le renvoie alors qu'en HTTPS. Derrière un reverse proxy qui termine TLS, l'application reçoit du HTTP simple : déclarez le proxy avec [`app.trust_proxy`](requests.md#derrière-un-proxy--trust_proxy), pour que son `X-Forwarded-Proto: https` soit pris en compte, ou forcez l'attribut avec `.secure(true)`.
- **Signé** quand l'application a un [`cookie_secret`](#le-secret). L'identifiant fait déjà 128 bits aléatoires venus du système d'exploitation, impossibles à deviner ; la signature permet à Vitesse de rejeter un identifiant falsifié sans interroger le stockage.

> [!IMPORTANT]
> En HTTP simple, n'importe qui sur le même réseau (un Wi-Fi public) peut lire le cookie de session et s'en servir. En production, servez l'application en HTTPS : voir [HTTPS et HTTP/2](https.md) et [Mise en production](production.md).

## Stockages

### `MemoryStore`

`MemoryStore::new()` garde les sessions dans la mémoire du processus. Il n'a besoin de rien, ce qui le rend idéal pour le développement, les tests et les petites applications, mais :

- les sessions sont perdues quand le serveur redémarre : chaque déploiement déconnecte tout le monde ;
- elles ne sont pas partagées entre les instances : derrière un répartiteur de charge, un visiteur connecté sur un serveur ne l'est pas sur les autres ;
- les sessions expirées sont retirées de temps en temps, au fil de l'arrivée des nouvelles.

### Votre propre stockage

Pour Redis, PostgreSQL ou n'importe quelle autre base de données, implémentez les trois méthodes du trait `SessionStore`. Les données d'une session sont un `Record`, une `serde_json::Map`, facile à enregistrer sous forme de texte JSON. Voici la forme d'un stockage SQL, avec une `HashMap` qui tient lieu de table pour que l'exemple fonctionne tel quel ; les commentaires donnent les requêtes correspondantes :

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use vitesse::session::{Record, SessionStore};
use vitesse::tokio::sync::RwLock;
use vitesse::{BoxError, BoxFuture, serde_json};

/// Tient lieu de table `sessions (id TEXT PRIMARY KEY, data TEXT, expires_at TIMESTAMP)`.
/// Un vrai stockage garde ici un pool de connexions (`sqlx::PgPool`, `deadpool_redis::Pool`…).
#[derive(Clone, Default)]
struct SqlStore {
    table: Arc<RwLock<HashMap<String, (String, SystemTime)>>>,
}

impl SessionStore for SqlStore {
    fn load(&self, id: &str) -> BoxFuture<Result<Option<Record>, BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        Box::pin(async move {
            // SELECT data FROM sessions WHERE id = $1 AND expires_at > now()
            let table = table.read().await;
            match table.get(&id) {
                Some((data, expires_at)) if *expires_at > SystemTime::now() => {
                    Ok(Some(serde_json::from_str(data)?))
                }
                _ => Ok(None),
            }
        })
    }

    fn save(&self, id: &str, record: &Record, ttl: Duration) -> BoxFuture<Result<(), BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        let data = serde_json::to_string(record);
        Box::pin(async move {
            // INSERT INTO sessions (id, data, expires_at) VALUES ($1, $2, now() + $3)
            // ON CONFLICT (id) DO UPDATE SET data = $2, expires_at = now() + $3
            table.write().await.insert(id, (data?, SystemTime::now() + ttl));
            Ok(())
        })
    }

    fn delete(&self, id: &str) -> BoxFuture<Result<(), BoxError>> {
        let (table, id) = (self.table.clone(), id.to_owned());
        Box::pin(async move {
            // DELETE FROM sessions WHERE id = $1
            table.write().await.remove(&id);
            Ok(())
        })
    }
}

let mut app = App::new();
app.middleware(middleware::session(SqlStore::default()));
```

- Chaque méthode renvoie une `BoxFuture` qui doit posséder ses données (`'static`) : clonez l'identifiant et le pool, et sérialisez le record, **avant** le bloc `async`.
- `load` s'exécute pour chaque requête qui porte un cookie de session, `save` quand la session a changé, et `delete` sur `destroy()` et `regenerate()`.
- `?` transforme n'importe quelle erreur en `BoxError`. Une erreur de `load` ou de `save` donne une `500` à la requête (la cause est journalisée) ; une erreur de `delete` est seulement journalisée.
- Avec Redis, `SET id data EX ttl`, `GET id` et `DEL id` suffisent, et Redis oublie de lui-même les sessions expirées. Dans une table SQL, supprimez de temps en temps les lignes expirées, dans une tâche lancée avec l'application.

### tower-sessions

Si vous utilisez déjà [tower-sessions](https://crates.io/crates/tower-sessions) et ses stockages, cela fonctionne aussi, grâce à la feature `tower` : voir [Sessions avec tower-sessions](from-axum-actix.md#sessions-avec-tower-sessions). Son API est asynchrone (`session.get(...).await?`), et il ignore `app.cookie_secret`.

## CSRF

Les navigateurs joignent automatiquement les cookies aux requêtes, y compris aux requêtes déclenchées par un autre site : un formulaire d'un autre site qui envoie un `POST` à votre route `/transfer` porterait la session de votre visiteur. C'est la falsification de requête intersite (CSRF).

`SameSite=Lax`, le réglage par défaut du cookie de session, en bloque l'essentiel : les navigateurs n'envoient pas le cookie avec un `POST`, un `PUT`, un `PATCH` ou un `DELETE` venant d'un autre site, ni avec ses appels `fetch` ou ses images, seulement avec les simples navigations (les liens), qui sont des requêtes `GET`. Votre part du travail : faire en sorte qu'un `GET` ne modifie jamais rien.

Ajoutez un jeton CSRF quand cela ne suffit pas : quand vous passez à `SameSite::None` (l'application tourne dans une iframe sur un autre site, ou un prestataire de paiement lui renvoie des formulaires), quand vos sous-domaines ne se font pas tous confiance (ils sont le « même site »), ou quand les anciens navigateurs comptent. Gardez un jeton aléatoire dans la session, placez-le dans un champ caché de chaque formulaire, et comparez les deux à chaque `POST` :

```rust
use serde::Deserialize;
use vitesse::prelude::*;

/// Le jeton CSRF de cette session, créé à sa première utilisation.
fn csrf_token(req: &Request) -> vitesse::Result<String> {
    let session = req.session();
    if let Some(token) = session.get::<String>("csrf") {
        return Ok(token);
    }
    let token = format!("{:032x}", rand::random::<u128>()); // cargo add rand
    session.insert("csrf", &token)?;
    Ok(token)
}

app.get("/transfer", |req: Request| async move {
    let token = csrf_token(&req)?;
    Ok::<_, Error>(Html(format!(
        r#"<form method="post"><input type="hidden" name="csrf" value="{token}">…</form>"#
    )))
});

#[derive(Deserialize)]
struct Transfer {
    csrf: String,
    amount: u64,
}

app.post("/transfer", |req: Request| async move {
    let form: Transfer = req.form().await?;
    let expected = req.session().get::<String>("csrf").unwrap_or_default();
    if expected.is_empty() || !middleware::constant_time_eq(form.csrf.as_bytes(), expected.as_bytes()) {
        return Err(Error::forbidden("jeton CSRF invalide"));
    }
    Ok(format!("{} transférés", form.amount))
});
```

Avec des [gabarits](templates.md), passez le jeton dans le contexte : `req.render("transfer.html", json!({ "csrf": csrf_token(&req)? }))`.

## Tests

`TestClient` ne garde pas les cookies d'une requête à l'autre : lisez l'en-tête `Set-Cookie` d'une réponse, et renvoyez sa partie `nom=valeur` dans un en-tête `cookie`, comme le ferait un navigateur :

```rust
use vitesse::test::{TestClient, TestResponse};

/// La partie `nom=valeur` de l'en-tête `Set-Cookie` de la réponse.
fn cookie_of(res: &TestResponse) -> String {
    let set_cookie = res.header("set-cookie").expect("pas d'en-tête Set-Cookie");
    set_cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn counts_visits_per_visitor() {
    let client = TestClient::new(my_api::app());

    let first = client.get("/visits").await;
    assert_eq!(first.text(), "visite n°1");
    let cookie = cookie_of(&first); // "vitesse.sid=…"

    let second = client.get("/visits").header("cookie", &cookie).await;
    assert_eq!(second.text(), "visite n°2");

    // Un autre visiteur, sans le cookie, repart de zéro.
    assert_eq!(client.get("/visits").await.text(), "visite n°1");
}
```

Il en va de même pour les cookies signés, et modifier la valeur est un bon moyen de vérifier qu'un cookie falsifié est refusé : `cookie.replacen("dark", "light", 1)`. Plus de détails dans [Tests](testing.md#tester-les-sessions).
