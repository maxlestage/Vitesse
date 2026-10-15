# Validation des données

Désérialiser un corps avec serde vérifie sa *forme* : `{"age": "dix"}` n'est pas un `u8`, et `req.json()` répond `400`. La validation vérifie son *contenu* : une adresse e-mail qui en a l'air, un nom non vide, un âge d'au moins 13 ans. Le module `validate` de Vitesse s'y prend comme le `$request->validate()` de Laravel ou les schémas de Fastify : chaque règle qui échoue est signalée, champ par champ, dans une `422 Unprocessable Entity` que le client peut afficher à côté de son formulaire.

## Valider un corps

Implémentez `Validate` pour le type reçu, puis lisez-le avec `req.valid_json()` au lieu de `req.json()` :

```rust
use serde::Deserialize;
use vitesse::prelude::*;
use vitesse::validate::{Validate, Validator};

#[derive(Deserialize)]
struct Inscription {
    email: String,
    nom: String,
    age: u8,
    site: Option<String>,
}

impl Validate for Inscription {
    fn validate(&self, v: &mut Validator) {
        v.field("email", &self.email).email();
        v.field("nom", &self.nom).not_empty().max_length(50);
        v.field("age", &self.age).min(13);
        v.optional("site", &self.site).url();
    }
}

let mut app = App::new();
app.post("/utilisateurs", |req: Request| async move {
    let inscription: Inscription = req.valid_json().await?;
    Ok::<_, Error>((201, Json(json!({ "email": inscription.email }))))
});
```

`valid_json` désérialise le corps, puis applique les règles :

```text
POST /utilisateurs {"email":"ada@example.com","nom":"Ada","age":36}
→ 201 {"email":"ada@example.com"}

POST /utilisateurs {"email":"non","nom":"","age":9}
→ 422 {"error":"Validation failed","details":{"age":["must be at least 13"],"email":["must be a valid email address"],"nom":["must not be empty"]}}

POST /utilisateurs {"email":
→ 400 {"error":"invalid JSON: EOF while parsing a value at line 1 column 9"}
```

Il en va de même pour les formulaires et les query strings :

| Méthode | Lit | Puis |
|---|---|---|
| `req.valid_json::<T>().await?` | Un corps JSON, comme `req.json()` | Le vérifie : `422` si une règle échoue |
| `req.valid_form::<T>().await?` | Un formulaire URL-encodé, comme `req.form()` | Idem |
| `req.valid_query::<T>()?` | La query string, comme `req.query_as()` | Idem |
| `vitesse::validate::validate(&valeur)?` | Une valeur que vous avez déjà | Idem |

La validation ne s'exécute que là où vous l'appelez : les routes qui ne s'en servent pas n'en paient pas le prix.

## Les règles

Un champ commence par `v.field(nom, &valeur)` et enchaîne ses règles. Il s'arrête à sa **première** règle en échec : le client reçoit un message par problème, et non « est vide » *et* « est trop court ».

| Règle | Pour | Échoue si |
|---|---|---|
| `.not_empty()` | chaînes, `Vec`, tranches, maps, ensembles | la valeur est vide |
| `.min_length(n)`, `.max_length(n)` | les mêmes | elle a moins / plus de `n` caractères (caractères Unicode, pas octets) ou éléments |
| `.min(x)`, `.max(x)` | nombres, et tout ce qui est `PartialOrd + Display` | la valeur est inférieure / supérieure à `x` |
| `.between(a, b)` | les mêmes | la valeur est hors de `a..=b` |
| `.one_of(&["S", "M", "L"])` | tout ce qui se compare aux choix | la valeur n'en fait pas partie |
| `.email()` | chaînes | elle ne ressemble pas à `nom@domaine.tld` |
| `.url()` | chaînes | ce n'est pas une URL `http://` ou `https://` avec un hôte |
| `.alphanumeric()` | chaînes | elle contient autre chose que des lettres et chiffres ASCII |
| `.check(\|v\| …, "message")` | tout | votre fonction renvoie `false` |

### Champs optionnels et obligatoires

Pour une `Option`, `v.optional(nom, &valeur)` saute les règles quand la valeur vaut `None`, et `v.required(nom, &valeur)` signale alors « is required » :

```rust
v.optional("site", &self.site).url();                     // absent : accepté
v.required("mot_de_passe", &self.mot_de_passe).min_length(12); // absent : erreur
```

> [!TIP]
> serde rejette déjà un champ absent qui n'est pas une `Option`, avec une `400` (`missing field 'email'`). Déclarez le champ en `Option` et utilisez `v.required(…)` si vous voulez que le champ manquant figure dans la `422`, avec les autres.

### Vos propres règles et messages

`.check` ajoute une règle à vous, et `.message` remplace le message de la règle juste avant, si elle a échoué : c'est aussi ainsi qu'on les traduit.

```rust
impl Validate for Compte {
    fn validate(&self, v: &mut Validator) {
        v.field("pseudo", &self.pseudo)
            .min_length(3)
            .message("au moins 3 caractères")
            .check(|p| !p.contains(' '), "sans espaces");
        if self.mot_de_passe != self.confirmation {
            v.error("confirmation", "ne correspond pas au mot de passe");
        }
    }
}
```

`v.error(nom, message)` signale un problème directement : une règle qui porte sur plusieurs champs, ou une vérification faite avant (un pseudo déjà pris dans votre base, par exemple).

### Valeurs imbriquées et listes

`v.nested` valide une valeur qui implémente elle-même `Validate`, et `v.each` chaque élément d'une liste. Leurs champs sont signalés avec un chemin :

```rust
impl Validate for Commande {
    fn validate(&self, v: &mut Validator) {
        v.nested("adresse", &self.adresse); // "adresse.ville"
        v.field("articles", &self.articles).not_empty();
        v.each("articles", &self.articles); // "articles[2].quantite"
    }
}
```

## La réponse

Une validation qui échoue est une `vitesse::Error` ordinaire : une `422` dont le message est `Validation failed`, et dont les `details` sont les messages de chaque champ, triés par nom. Elle passe par `app.on_error` comme toute erreur, qui peut lire `err.details()` pour la présenter autrement : un formulaire HTML avec ses erreurs, ou le format `application/problem+json`, par exemple. Voir [Gestion des erreurs](errors.md#détails-structurés--with_details).

Pour valider sans passer par une requête (une valeur lue dans un fichier, par exemple), appelez `validate::validate(&valeur)`, ou construisez vous-même un `Validator` : `is_valid()`, `errors()` et `into_result()` donnent le résultat.

## Paramètres de route typés

Les paramètres de chemin se valident par leur type et leurs contraintes plutôt que par des règles : une contrainte dans le motif (`r"/users/:id(\d+)"`) empêche la route de correspondre, et `req.param_as::<u64>("id")?` ou `req.params_as::<T>()?` répondent `400` quand une valeur ne se convertit pas. Voir [Paramètres typés](routing.md#paramètres-typés).
