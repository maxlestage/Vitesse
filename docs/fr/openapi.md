# OpenAPI et Swagger UI

Vitesse génère le document [OpenAPI 3.1](https://spec.openapis.org/oas/v3.1.0) de votre application à partir de ses routes, comme FastAPI ou `@fastify/swagger`, et peut servir une page Swagger UI pour lire et essayer l'API dans un navigateur. Les générateurs de clients, Postman, les passerelles d'API et les outils de documentation lisent tous ce document.

## Servir le document

Un seul appel, n'importe où dans la construction de l'application :

```rust
use vitesse::prelude::*;
use vitesse::openapi::OpenApi;

let mut app = App::new();
app.get("/users/:id", afficher);
app.post("/users", creer);

app.openapi(
    "/openapi.json",
    OpenApi::new("API Utilisateurs", "1.0.0").swagger_ui("/docs"),
);
app.run(3000)
```

`GET /openapi.json` répond désormais le document, et `GET /docs` la page Swagger UI (elle charge Swagger UI depuis le CDN jsDelivr). Toutes les routes sont listées, routeurs montés et groupes compris, avec les paramètres de leur chemin : `/users/:id` devient `/users/{id}`, et une contrainte comme `r"/users/:id(\d+)"` devient le `pattern` du schéma du paramètre. Les paramètres optionnels donnent un chemin par variante (`/archive` et `/archive/{year}`).

Le document est construit une fois, au démarrage, et servi depuis la mémoire : il ne coûte rien par requête, et rien du tout si vous n'appelez pas `app.openapi`.

## Décrire une route

`.doc(…)` suit une route, comme `.name(…)`, et la décrit avec une `Operation` :

```rust
use vitesse::openapi::Operation;

app.get(r"/users/:id(\d+)", afficher)
    .name("user")
    .doc(
        Operation::new()
            .summary("Lire un utilisateur")
            .tag("utilisateurs")
            .path_param("id", "L'identifiant", json!({ "type": "integer" }))
            .response_json(200, "L'utilisateur", json!({ "$ref": "#/components/schemas/User" }))
            .response(404, "Utilisateur inconnu"),
    );

app.post("/users", creer).doc(
    Operation::new()
        .summary("Créer un utilisateur")
        .tag("utilisateurs")
        .body(json!({
            "type": "object",
            "required": ["email"],
            "properties": { "email": { "type": "string", "format": "email" } }
        }))
        .response(201, "Créé")
        .response(422, "Champs invalides"),
);
```

Les schémas sont du JSON Schema ordinaire, écrit avec `json!` : aucune macro à apprendre, et ce que vous écrivez est ce que lisent les clients.

| Méthode d'`Operation` | Définit |
|---|---|
| `.summary(…)`, `.description(…)` | Le résumé et la description (Markdown) |
| `.tag(…)` | Une étiquette, pour regrouper les opérations (plusieurs possibles) |
| `.operation_id(…)` | L'identifiant que les générateurs de clients prennent comme nom de fonction |
| `.path_param(nom, description, schéma)` | Décrit un paramètre de chemin (sinon listé comme chaîne) |
| `.query(nom, description, schéma)`, `.query_required(…)` | Un paramètre de query string |
| `.header(nom, description, schéma, obligatoire)` | Un en-tête de requête |
| `.body(schéma)`, `.body_with(type, schéma)` | Un corps de requête obligatoire, JSON ou d'un autre type |
| `.response(statut, description)`, `.response_json(statut, description, schéma)` | Une réponse possible (`200 OK` si aucune n'est donnée) |
| `.security(schéma)`, `.public()` | Exige un schéma de sécurité, ou aucun |
| `.deprecated()` | Marque l'opération comme dépréciée |
| `.hidden()` | Retire la route du document |
| `.extend(json!({ … }))` | Fusionne tout le reste (`externalDocs`, `callbacks`…) |

`.doc` fonctionne sur `App` comme sur `Router` : les descriptions d'un routeur suivent ses routes quand il est monté.

## Décrire l'API

`OpenApi` porte ce qui vaut pour toute l'API :

```rust
let spec = OpenApi::new("API Utilisateurs", "1.0.0")
    .description("Tout sur les utilisateurs.")
    .server("https://api.example.com")
    .tag("utilisateurs", "Comptes et profils")
    .schema("User", json!({
        "type": "object",
        "properties": { "id": { "type": "integer" }, "email": { "type": "string" } }
    }))
    .security_scheme("bearer", json!({ "type": "http", "scheme": "bearer" }))
    .security("bearer") // toutes les opérations, sauf `.public()`
    .extend(json!({ "info": { "contact": { "email": "api@example.com" } } }))
    .swagger_ui("/docs");
app.openapi("/openapi.json", spec);
```

Les schémas partagés se référencent avec `json!({ "$ref": "#/components/schemas/User" })`.

## Ce qui est listé

- Toutes les routes avec une méthode : celles de l'application, de ses routeurs et de ses groupes.
- Pas listées sans `.doc(…)` : les routes pour toutes les méthodes (`app.all`), les routes `HEAD` et `OPTIONS`, et les routes à joker comme les fichiers statiques. Une route avec `.doc(Operation::new().hidden())` n'est jamais listée.
- Ni le document lui-même, ni la page Swagger UI.

## Exporter le document

`app.openapi_document(&spec)` renvoie le document des routes déjà ajoutées sous forme de `serde_json::Value`, sans le servir : pour l'écrire dans un fichier lors du build, ou le vérifier dans un test.

```rust
let doc = app.openapi_document(&OpenApi::new("API Utilisateurs", "1.0.0"));
std::fs::write("openapi.json", serde_json::to_vec_pretty(&doc)?)?;
```

En production, vous voudrez peut-être garder le document et la page Swagger UI privés : protégez leurs chemins avec un [hook `on_request`](middleware.md#hooks-du-cycle-de-vie) ou un middleware global qui vérifie les identifiants, ou n'appelez `app.openapi` que dans les builds de développement.
