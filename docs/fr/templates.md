# Gabarits HTML

Pour générer des pages HTML sur le serveur, Express a `res.render` avec Pug, EJS ou Handlebars. Vitesse a la feature `templates`, bâtie sur [minijinja](https://docs.rs/minijinja), une implémentation en Rust de Jinja2 : la syntaxe de Jinja (Python), de Nunjucks (JavaScript) et de Twig (PHP), avec `{{ … }}` pour les valeurs et `{% … %}` pour la logique.

## Activer la feature `templates`

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["templates"] }
```

Elle ajoute `app.views(...)`, `app.templates(...)`, `req.render(...)` et le module `vitesse::templates`, qui réexporte minijinja (`vitesse::templates::minijinja`).

## Votre première page

Placez les gabarits dans un dossier, ici `templates/`, à côté de `Cargo.toml` :

```text
mon-app/
├── Cargo.toml
├── src/
│   └── main.rs
└── templates/
    ├── layout.html
    └── users.html
```

Indiquez à l'application où ils se trouvent avec `app.views(dossier)`, puis rendez-en un avec `req.render(nom, contexte)` :

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.views("templates");

    app.get("/users", |req: Request| async move {
        let users = vec!["Ada", "Grace", "Margaret"];
        req.render("users.html", json!({ "title": "Utilisateurs", "users": users }))
    });

    app.run(3000)
}
```

```jinja
{# templates/users.html #}
{% extends "layout.html" %}

{% block content %}
  <h1>{{ title }}</h1>
  <ul>
    {% for user in users %}
      <li>{{ user }}</li>
    {% endfor %}
  </ul>
{% endblock %}
```

- Le nom d'un gabarit est son chemin dans le dossier : `"users.html"`, ou `"admin/stats.html"` pour `templates/admin/stats.html`. Écrivez l'extension : il n'y a pas de `view engine` pour l'ajouter.
- Le dossier est relatif au dossier courant du serveur, comme les chemins de `res::file` : lancez `cargo run` depuis la racine du projet.
- `req.render` renvoie un `Result<Html<String>, Error>` : renvoyez-le tel quel, ou utilisez `?` dans un handler qui renvoie `vitesse::Result<Html<String>>`. La réponse est un `200` avec `content-type: text/html; charset=utf-8`.

`render` est une méthode de la requête, pas d'un objet réponse : dans Vitesse, le handler [renvoie](responses.md) la page rendue.

## Gabarits de base avec `extends` et `block`

Un gabarit de base contient ce que toutes les pages partagent, et laisse des blocs que les pages remplissent :

```jinja
{# templates/layout.html #}
<!doctype html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <title>{% block title %}{{ title }}{% endblock %} · Mon application</title>
</head>
<body>
  <main>{% block content %}{% endblock %}</main>
</body>
</html>
```

Une page commence par `{% extends "layout.html" %}`, puis redéfinit les blocs qu'elle veut ; les autres gardent le contenu du gabarit de base. Les variables de la page (`title` ici) sont aussi disponibles dans le gabarit de base. Dans un bloc, `{{ super() }}` insère la version de ce bloc définie par le gabarit de base.

## Valeurs, boucles et conditions

| Syntaxe | Effet |
|---|---|
| `{{ user.name }}`, `{{ user["name"] }}` | Une valeur, ou un champ d'une valeur (échappée dans les gabarits HTML) |
| `{{ name\|upper }}`, `{{ items\|length }}`, `{{ bio\|default("none") }}` | Un filtre, appliqué à la valeur |
| `{% for item in items %}…{% else %}…{% endfor %}` | Une boucle ; la partie `else` s'affiche quand la liste est vide |
| `{{ loop.index }}`, `{{ loop.first }}`, `{{ loop.last }}` | Dans une boucle : la position (à partir de 1), s'il s'agit du premier élément, du dernier |
| `{% if user.admin %}…{% elif user.active %}…{% else %}…{% endif %}` | Une condition |
| `{% set total = items\|length %}` | Une variable du gabarit |
| `{# un commentaire #}` | Un commentaire, absent de la page |
| `{%- … -%}` | Supprime les espaces avant ou après la balise |

```jinja
<ul>
  {% for user in users %}
    <li class="{{ "admin" if user.admin else "member" }}">{{ loop.index }}. {{ user.name|title }}</li>
  {% else %}
    <li>Aucun utilisateur pour l'instant.</li>
  {% endfor %}
</ul>
```

Les filtres intégrés comprennent `upper`, `lower`, `title`, `capitalize`, `trim`, `length`, `default`, `join`, `first`, `last`, `sort`, `reverse`, `replace`, `round` et `safe`. La [documentation de minijinja](https://docs.rs/minijinja/latest/minijinja/syntax/index.html) détaille toute la syntaxe.

## Le contexte : `json!` ou une structure

Le contexte est n'importe quoi qui implémente `Serialize`. `json!` est pratique pour quelques valeurs ; une structure avec `#[derive(Serialize)]` documente ce qu'attend le gabarit, et le compilateur la vérifie côté Rust :

```rust
use serde::Serialize;
use vitesse::prelude::*;

#[derive(Serialize)]
struct User {
    name: String,
    admin: bool,
}

#[derive(Serialize)]
struct UsersPage {
    title: &'static str,
    users: Vec<User>,
}

async fn users(req: Request) -> vitesse::Result<Html<String>> {
    let users = vec![
        User { name: "ada lovelace".into(), admin: true },
        User { name: "grace hopper".into(), admin: false },
    ]; // dans une vraie application : depuis votre base de données
    req.render("users.html", UsersPage { title: "Utilisateurs", users })
}
```

Les champs deviennent des variables (`{{ title }}`, `{% for user in users %}`), `None` devient `none` (faux dans un `{% if %}`), et une page sans variables prend `()` comme contexte.

## L'échappement et `|safe`

Dans les gabarits dont le nom se termine par `.html`, `.htm` ou `.xml`, chaque valeur est échappée pour le HTML : `<`, `>`, `&`, `"`, `'` et `/` deviennent des entités. Un commentaire qui contient `<script>` s'affiche comme du texte, et ne s'exécute pas :

```jinja
<p>{{ comment }}</p>
{# comment = "<script>alert(1)</script>" donne : #}
<p>&lt;script&gt;alert(1)&lt;&#x2f;script&gt;</p>
```

Les gabarits qui ont une autre extension (`.txt` pour le texte d'un e-mail, par exemple) ne sont pas échappés.

Pour insérer du HTML tel quel, marquez-le avec le filtre `safe` : `{{ article_html|safe }}`. Ne le faites que pour du HTML que vous avez produit vous-même, du Markdown rendu par votre serveur par exemple, et nettoyé s'il vient des utilisateurs (avec un crate comme ammonia).

> [!WARNING]
> L'échappement protège le texte et les attributs entre guillemets (`title="{{ name }}"`). Il ne rend pas une URL sûre : `<a href="{{ website }}">` avec un site web réglé sur `javascript:…` par un utilisateur exécute quand même un script au clic, vérifiez donc que ces URL commencent par `https://`. Évitez aussi de placer des valeurs dans des blocs `<script>` : mettez-les dans un attribut `data-` et lisez-le depuis JavaScript.

## Des fragments avec `include`

`{% include %}` insère un autre gabarit, qui voit les mêmes variables. C'est le moyen de partager une barre de navigation ou un pied de page entre plusieurs gabarits de base :

```jinja
{# templates/layout.html #}
<body>
  {% include "partials/nav.html" %}
  <main>{% block content %}{% endblock %}</main>
</body>
```

Pour un morceau qui prend des paramètres, un champ de formulaire par exemple, écrivez une macro et importez-la :

```jinja
{# templates/forms.html #}
{% macro field(name, label, type="text") %}
  <label>{{ label }} <input name="{{ name }}" type="{{ type }}"></label>
{% endmacro %}

{# templates/login.html #}
{% import "forms.html" as forms %}
<form method="post">
  {{ forms.field("email", "E-mail", type="email") }}
  {{ forms.field("password", "Mot de passe", type="password") }}
</form>
```

## Filtres, fonctions et gabarits intégrés : `app.templates`

`app.views(dossier)` couvre le cas courant. Pour vos propres filtres, fonctions ou variables globales, configurez un `Environment` minijinja et donnez-le à l'application avec `app.templates(env)` :

```rust
use vitesse::prelude::*;
use vitesse::templates::minijinja::{Environment, UndefinedBehavior, path_loader};

let mut env = Environment::new();
env.set_loader(path_loader("templates")); // ce que fait app.views("templates")
env.set_undefined_behavior(UndefinedBehavior::SemiStrict); // une variable mal orthographiée est une erreur
env.add_filter("price", |cents: u64| format!("{},{:02} €", cents / 100, cents % 100));
env.add_function("asset", |path: String| {
    format!("/static/{path}?v={}", env!("CARGO_PKG_VERSION"))
});
env.add_global("site_name", "Ma boutique");

let mut app = App::new();
app.templates(env);
```

```jinja
<link rel="stylesheet" href="{{ asset("app.css") }}">
<h1>{{ site_name }}</h1>
<p>{{ product.name }} : {{ product.cents|price }}</p>
```

- Un filtre ou une fonction est une closure : ses arguments sont convertis à partir des valeurs du gabarit, et ce qu'elle renvoie retourne dans la page.
- Une variable globale est disponible dans tous les gabarits, comme le `app.locals` d'Express dans les vues.
- Par défaut, une variable manquante s'affiche comme une chaîne vide. Avec `UndefinedBehavior::SemiStrict`, l'afficher ou la parcourir dans une boucle est une erreur, ce qui attrape les fautes de frappe, alors que `{% if user %}` fonctionne toujours quand `user` manque.

### Gabarits intégrés au binaire

Au lieu d'un loader, ajoutez les gabarits à l'environnement avec `include_str!` : ils sont compilés dans le programme, et le serveur n'a plus besoin du dossier `templates/` à l'exécution.

```rust
use vitesse::templates::minijinja::Environment;

let mut env = Environment::new();
env.add_template("layout.html", include_str!("../templates/layout.html")).unwrap();
env.add_template("users.html", include_str!("../templates/users.html")).unwrap();
app.templates(env);
```

`include_str!` prend un chemin relatif au fichier source (`src/main.rs` ici). `add_template` vérifie la syntaxe tout de suite : une erreur dans un gabarit arrête donc le programme au démarrage, au lieu d'échouer à la première requête. La contrepartie : modifier un gabarit demande de recompiler.

## Erreurs

Un gabarit qui n'existe pas, une erreur de syntaxe, un filtre inconnu ou un champ lu sur une valeur manquante : `req.render` renvoie une erreur, et le visiteur reçoit une `500 {"error":"Internal Server Error"}`. La cause, avec le nom du gabarit et la ligne, n'est écrite que dans les journaux :

```text
[vitesse] error 500: unknown filter: filter prices is unknown (in users.html:5)
[vitesse] error 500: template not found: template "user.html" does not exist
```

Appeler `req.render` quand l'application n'a ni `app.views` ni `app.templates` donne aussi une `500`.

Avec `app.views`, un gabarit est compilé la première fois qu'il sert : une erreur de syntaxe n'apparaît donc que lorsque quelqu'un ouvre cette page. Un test qui rend chaque page la détecte avant un déploiement : voir [Tester les gabarits](testing.md#tester-les-gabarits).

## Recharger les gabarits

Les gabarits sont lus sur le disque la première fois que chacun sert, puis gardés en mémoire : une modification n'apparaît qu'après un redémarrage. Pendant le développement, `cargo watch -x run` ([cargo-watch](https://crates.io/crates/cargo-watch)) surveille tout le projet et redémarre le serveur, y compris quand vous enregistrez un gabarit. En production, ce cache est ce que vous voulez : une page est compilée une fois, et rendue de nombreuses fois.

## Déployer

Le dossier `templates/` doit exister dans le dossier depuis lequel le serveur est lancé. Dans une image Docker, copiez-le dans l'étape d'exécution, comme pour les [fichiers statiques](docker.md#fichiers-statiques-et-autres-ressources), ou [intégrez](#gabarits-intégrés-au-binaire) les gabarits au binaire.

## Venir d'Express

| Express | Vitesse |
|---|---|
| `app.set('views', './views')` | `app.views("views")` |
| `app.set('view engine', 'pug')` | Rien à régler : le moteur est minijinja, et les noms des gabarits incluent leur extension |
| `res.render('users', { users })` | `req.render("users.html", json!({ "users": users }))` |
| `app.locals.siteName` dans toutes les vues | `env.add_global("site_name", …)` avec `app.templates(env)` |
| `res.locals.user` | Placez-le dans le contexte |
| `extends` et `block` de Pug, `include` d'EJS | `{% extends %}`, `{% block %}`, `{% include %}` |

## D'autres moteurs de gabarits

La feature `templates` est une commodité, pas une obligation. N'importe quel moteur de gabarits Rust fonctionne : [askama](https://crates.io/crates/askama) (gabarits vérifiés à la compilation), [tera](https://crates.io/crates/tera), [maud](https://crates.io/crates/maud) (du HTML écrit sous forme de macros Rust), [handlebars](https://crates.io/crates/handlebars)… Rendez la page dans une `String` avec l'API du crate, puis renvoyez `Html(page)`, qui l'envoie avec `content-type: text/html; charset=utf-8` (voir [Répondre](responses.md#ce-quun-handler-peut-renvoyer)).
