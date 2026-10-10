# Déployer sur Heroku depuis un téléphone

Pas besoin d'ordinateur pour mettre une application Vitesse en ligne. Ce guide vous accompagne pas à pas depuis le navigateur de votre téléphone : d'abord un déploiement en un geste avec le bouton Deploy, puis un redéploiement automatique à chaque commit grâce à GitHub Actions.

> [!NOTE]
> Toutes les étapes ci-dessous fonctionnent dans un navigateur mobile (Safari, Chrome, Firefox…). Les interfaces de Heroku et de GitHub sont en anglais : les libellés sont donc donnés en anglais. Elles changent aussi de temps en temps : si un libellé diffère légèrement de ce que vous voyez, cherchez l'équivalent le plus proche. Sur github.com, si un menu ou un onglet semble manquer, passez votre navigateur en mode « Version pour ordinateur » (souvent dans le menu du navigateur).

## Avant de commencer

Il vous faut :

- **Un compte Heroku** : inscrivez-vous sur https://signup.heroku.com. Heroku n'a plus d'offre gratuite. Les options les moins chères sont les dynos **Eco** (environ 5 $ par mois) et **Basic** (environ 7 $ par mois), mais les tarifs évoluent : vérifiez sur https://www.heroku.com/pricing. Heroku demande un **moyen de paiement** avant de pouvoir créer une application.
- **Un compte GitHub**, seulement pour les parties 2 et 3 (votre propre copie du code et les déploiements automatiques).

### Comment ça marche

Le dépôt contient déjà tout ce dont Heroku a besoin, rien n'est donc compilé sur votre téléphone :

| Fichier | Rôle |
|---|---|
| `app.json` | Décrit l'application pour le bouton Deploy (pile « container », basée sur Docker). |
| `heroku.yml` | Indique à Heroku de construire l'image à partir du `Dockerfile`. |
| `Dockerfile` | Compile un exemple (`demo` par défaut) en mode release et l'emballe dans une petite image. |
| `examples/demo.rs` | L'application de démonstration : une page d'accueil, `/health` et une petite API JSON. |
| `.github/workflows/heroku.yml` | Le workflow **Deploy to Heroku**, pour les déploiements automatiques. |

Heroku construit l'image Docker sur ses propres serveurs, puis la démarre en indiquant à l'application le port sur lequel écouter.

## 1. Le plus rapide : le bouton Deploy

### Déployer l'application de démonstration

[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse)

1. Touchez le bouton ci-dessus (il figure aussi dans le README du projet). Connectez-vous à Heroku si on vous le demande.
2. Heroku affiche un formulaire de création d'application :
   - **App name** : par exemple `ma-demo-vitesse`. Les noms sont uniques sur tout Heroku et ne contiennent que des minuscules, des chiffres et des tirets. Vous pouvez aussi laisser le champ vide : Heroku en choisira un.
   - **Choose a region** : États-Unis ou Europe.
3. Touchez **Deploy app**. Heroku récupère le code et construit l'image Docker ; vous pouvez suivre le journal de construction sur la page. Comptez quelques minutes : le code Rust est compilé avec toutes les optimisations.
4. Une fois la construction terminée, touchez **View** pour ouvrir votre application. Vous devriez voir « ⚡ Vitesse is running. ». L'adresse exacte (`https://….herokuapp.com`) figure dans l'onglet **Settings** de l'application sur Heroku.

Essayez ensuite ces chemins, en les ajoutant après l'adresse de votre application dans la barre du navigateur :

| Chemin | Réponse |
|---|---|
| `/health` | `ok` |
| `/api/hello/Ada` | `{"message":"Hello, Ada!"}` |
| `/api/todos` | `[]` (la liste de tâches, vide pour l'instant) |

> [!NOTE]
> La démo garde sa liste de tâches en mémoire : elle est effacée à chaque redémarrage de l'application (nouveau déploiement, changement de réglage, et le redémarrage automatique qu'Heroku effectue environ une fois par jour). Pour des données qui doivent durer, utilisez une base de données.

### Déployer votre propre copie (fork)

Pour modifier le code, il vous faut votre propre copie du dépôt sur GitHub :

1. Ouvrez https://github.com/maxlestage/Vitesse, touchez **Fork**, puis **Create fork**.
2. Ouvrez le lien de déploiement avec votre nom d'utilisateur GitHub à la place de `maxlestage`, en le tapant dans la barre d'adresse :

   ```text
   https://www.heroku.com/deploy?template=https://github.com/<vous>/Vitesse
   ```

   Vous pouvez aussi modifier le bouton dans le `README.md` de votre fork pour qu'il pointe vers celui-ci (la partie 3 explique comment modifier un fichier depuis votre téléphone).
3. Suivez les mêmes étapes que ci-dessus.

> [!IMPORTANT]
> Le bouton déploie **une seule fois**. Quand vous modifiez votre fork ensuite, l'application Heroku n'est pas mise à jour automatiquement. Pour redéployer à chaque commit, configurez le workflow de la partie suivante en réutilisant le même nom d'application : le workflow peut mettre à jour l'application que vous venez de créer.

## 2. Déploiement continu avec GitHub Actions

Objectif : chaque commit sur la branche `master` de votre fork reconstruit et redéploie l'application, sans rien faire de plus. Le dépôt contient un workflow nommé **Deploy to Heroku** ([`.github/workflows/heroku.yml`](https://github.com/maxlestage/Vitesse/blob/master/.github/workflows/heroku.yml)) qui dialogue directement avec l'API Platform de Heroku : ni CLI Heroku, ni ordinateur.

À chaque exécution, il :

1. vérifie qu'il est configuré (sinon il s'arrête, sans échec lors d'un push) ;
2. crée l'application Heroku si elle n'existe pas encore (avec la pile Docker « container ») ;
3. envoie votre code à Heroku, qui construit l'image à partir de `heroku.yml` et la met en ligne ; le journal de construction s'affiche en direct dans GitHub ;
4. écrit l'adresse de l'application dans le résumé de l'exécution.

Il vous faut votre propre fork (voir « Déployer votre propre copie » plus haut).

### Étape 1 : obtenir un jeton d'API Heroku

1. Dans le navigateur de votre téléphone, allez sur https://dashboard.heroku.com et connectez-vous.
2. Ouvrez **Account settings** (depuis le menu ou votre avatar, en haut à droite).
3. Choisissez l'une de ces deux options :
   - **Rapide** : dans la section **API Key**, touchez **Reveal** et copiez la clé.
   - **Recommandé** : ouvrez l'onglet **Applications** puis, sous **Authorizations**, touchez **Create authorization**. Donnez-lui une description comme « GitHub Actions » (laisser l'expiration vide donne en général un jeton de longue durée), puis copiez le jeton. Un jeton dédié peut être révoqué à lui seul, sans rien casser d'autre.

> [!WARNING]
> Ce jeton agit en votre nom sur tout votre compte Heroku. Ne le collez jamais dans un fichier, un commit, une issue ou un message : uniquement dans les secrets de GitHub, comme décrit à l'étape suivante.

### Étape 2 : ajouter le secret et la variable sur GitHub

1. Sur github.com, ouvrez **votre fork** et touchez **Settings** (dans la barre d'onglets du dépôt ; faites-la défiler sur le côté, ou passez en « Version pour ordinateur » si vous ne le voyez pas).
2. Dans le menu, ouvrez **Secrets and variables**, puis **Actions**.
3. Dans l'onglet **Secrets**, touchez **New repository secret** :
   - **Name** : `HEROKU_API_KEY`
   - **Secret** : collez le jeton
   - touchez **Add secret**.
4. Ouvrez l'onglet **Variables** et touchez **New repository variable** :
   - **Name** : `HEROKU_APP_NAME`
   - **Value** : le nom de votre application (celle créée avec le bouton, ou un nouveau nom : le workflow crée l'application si besoin)
   - touchez **Add variable**.
5. Facultatif : ajoutez une variable `HEROKU_REGION` avec la valeur `eu` pour créer l'application en Europe. Elle ne sert que lorsque le workflow crée l'application.

| Nom | Type | Obligatoire | Valeur |
|---|---|---|---|
| `HEROKU_API_KEY` | Secret | Oui | Votre jeton Heroku |
| `HEROKU_APP_NAME` | Variable | Oui | Par exemple `ma-demo-vitesse` |
| `HEROKU_REGION` | Variable | Non | `us` (par défaut) ou `eu` |

### Étape 3 : activer Actions sur votre fork

GitHub désactive les workflows d'un fork tout juste créé. Ouvrez l'onglet **Actions** de votre fork : si un message indique que les workflows ne s'exécutent pas sur ce dépôt forké, touchez le bouton qui les active.

### Étape 4 : lancer le premier déploiement

1. Dans l'onglet **Actions**, choisissez **Deploy to Heroku** dans la liste des workflows.
2. Touchez **Run workflow**, gardez la branche `master` (le champ « Heroku app name » est facultatif : il remplace `HEROKU_APP_NAME` pour cette exécution seulement), puis confirmez avec le bouton vert **Run workflow**.
3. Touchez l'exécution qui apparaît pour la suivre. La première construction prend quelques minutes.
4. Quand elle passe au vert, l'adresse de l'application s'affiche dans le résumé de l'exécution.

Désormais, **chaque commit sur `master` qui touche au code** redéploie l'application : `src/`, `examples/`, `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `heroku.yml` ou le workflow lui-même. Modifier le README ou la documentation ne déclenche pas de déploiement.

> [!NOTE]
> Tant que le secret ou la variable manque, les pushs sautent simplement le déploiement (l'exécution affiche une notification, pas un échec). Une exécution manuelle, en revanche, échoue avec un message qui indique ce qui manque.

## 3. Personnaliser votre application

### Modifier le code depuis votre téléphone

1. Sur github.com, dans votre fork, ouvrez `examples/demo.rs`.
2. Touchez l'icône **crayon** (✏️, « Edit this file »).
3. Modifiez quelque chose, par exemple le message de la route `/api/hello/:name` :

   ```rust
   app.get("/api/hello/:name", |req: Request| async move {
       let name = req.param("name").unwrap_or("world");
       Json(json!({ "message": format!("Bonjour, {name} !") }))
   });
   ```

4. Touchez **Commit changes…**, écrivez un court message, gardez « Commit directly to the `master` branch » et confirmez avec **Commit changes**.
5. Le workflow démarre tout seul (vous pouvez le suivre dans l'onglet **Actions**). Quelques minutes plus tard, la nouvelle version est en ligne.

Ajouter une route est tout aussi simple. Insérez cette ligne à côté des autres routes :

```rust
app.get("/api/ping", |_| async { json!({ "pong": true }) });
```

Voyez [Routage](routing.md) et [Répondre](responses.md) pour tout ce qu'une route sait faire.

> [!TIP]
> Une faute de frappe dans le code Rust fait échouer la construction, et la version précédente reste simplement en ligne : Heroku ne met en ligne que les constructions réussies. Ouvrez l'exécution en échec dans l'onglet **Actions** pour lire le message du compilateur (il indique le fichier, la ligne et la colonne), corrigez le code et refaites un commit.

### Utiliser votre propre exemple

Vous pouvez laisser `demo.rs` tranquille et écrire votre application dans un nouveau fichier :

1. Dans votre fork, ouvrez le dossier `examples`, touchez **Add file** → **Create new file** et nommez-le, par exemple, `mon_app.rs`.
2. Collez une application minimale :

   ```rust
   use vitesse::prelude::*;

   fn main() -> std::io::Result<()> {
       let mut app = App::new();
       app.middleware(middleware::logger());

       app.get("/", |_| async { "Bonjour depuis mon téléphone !" });
       app.get("/health", |_| async { "ok" });

       // Heroku choisit le port : écoutez toujours sur $PORT.
       let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
       app.run(port)
   }
   ```

3. Faites un commit du fichier.
4. Ouvrez le `Dockerfile`, touchez le crayon et remplacez la ligne `ARG EXAMPLE=demo` par `ARG EXAMPLE=mon_app` (le nom du fichier sans `.rs`). Faites un commit : le workflow construit et déploie votre exemple.

L'argument de construction `EXAMPLE` choisit le fichier de `examples/` à compiler ; c'est en modifiant sa valeur par défaut dans le `Dockerfile` que Heroku utilise votre exemple. Votre code peut utiliser les crates déjà déclarées dans `Cargo.toml` : `vitesse`, `serde` (avec `derive`) et `tokio`.

> [!IMPORTANT]
> Votre application doit écouter sur **`0.0.0.0:$PORT`**. Heroku choisit le port au démarrage et le transmet dans la variable d'environnement `PORT`. `app.run(port)` avec un simple numéro de port (ou une chaîne qui ne contient que des chiffres) écoute déjà sur toutes les interfaces. N'écrivez jamais `3000` en dur et n'écoutez jamais uniquement sur `127.0.0.1`, sinon Heroku ne peut pas joindre votre application.

> [!TIP]
> Besoin d'une autre crate ? Ouvrez `Cargo.toml` avec l'icône crayon, ajoutez une ligne sous `[dev-dependencies]` (la section que les exemples peuvent utiliser sans changer les dépendances de la bibliothèque elle-même), par exemple `chrono = "0.4"`, et faites un commit. La construction résout la nouvelle crate toute seule, tandis que les autres gardent les versions fixées dans `Cargo.lock`. Plus tard, committer un `Cargo.lock` à jour depuis un ordinateur (n'importe quel `cargo build` le rafraîchit) reste une bonne pratique pour des constructions reproductibles.

### Réglages : variables d'environnement

Sur Heroku, ouvrez votre application, puis **Settings** → **Reveal Config Vars**. Chaque « config var » devient une variable d'environnement, que votre code lit avec `std::env::var("NOM")`. Enregistrer une modification redémarre l'application. Ne définissez pas `PORT` vous-même : Heroku s'en charge.

## 4. Au quotidien

### Journaux

Tableau de bord Heroku → votre application → **More** (en haut à droite) → **View logs**. Vous y verrez les messages d'Heroku lui-même (démarrage, arrêt, plantages, une ligne par requête venant de son routeur) et tout ce qu'affiche votre application. Avec `middleware::logger()`, l'application écrit une ligne par requête, comme `GET /api/todos 200 0.084 ms`.

L'onglet **Activity** liste les constructions et les mises en ligne, avec un lien vers le journal de chaque construction.

### Redémarrer, dimensionner, arrêter

- **Redémarrer** : **More** → **Restart all dynos**.
- **Type de dyno** : dans l'onglet **Resources**, modifiez la ligne `web` (icône crayon) pour changer de type (Eco, Basic, Standard…) ; le prix change en conséquence. Un dyno Eco s'endort après environ 30 minutes sans trafic, et la visite suivante le réveille en quelques secondes ; un dyno Basic ne dort jamais.
- **Plusieurs dynos** : faire tourner plusieurs dynos pour le même processus demande des dynos Standard ou supérieurs.
- **Arrêter** : dans le même mode d'édition, éteignez le dyno `web`, ou supprimez l'application (**Settings** → **Delete app**).

### Nom de domaine personnalisé

Dans **Settings** → **Domains**, touchez **Add domain** et saisissez, par exemple, `www.exemple.fr`. Heroku vous donne une **cible DNS** (« DNS target ») : créez chez votre registraire un enregistrement `CNAME` qui pointe vers elle. Un domaine racine (`exemple.fr`) nécessite un registraire qui gère les enregistrements `ALIAS`/`ANAME` ou l'aplatissement de CNAME.

Les certificats HTTPS des domaines personnalisés sont gérés par l'Automated Certificate Management (ACM) d'Heroku, qui n'est pas disponible sur tous les types de dynos : vérifiez sur https://devcenter.heroku.com/articles/automated-certificate-management. Plus de détails : https://devcenter.heroku.com/articles/custom-domains.

## 5. Dépannage

### La construction échoue

Ouvrez le journal de construction : dans GitHub (onglet **Actions**, exécution en échec) ou sur Heroku (onglet **Activity** → journal de construction). Cherchez la première ligne qui commence par `error` :

| Message | Cause | Solution |
|---|---|---|
| `error[E…]` avec un fichier et une ligne | Erreur de compilation Rust | Corrigez le code à cette ligne et refaites un commit. |
| `no example target named …` | `ARG EXAMPLE` ne correspond à aucun fichier de `examples/` | Utilisez le nom du fichier sans `.rs`. |
| `no matching package named …` ou `failed to select a version …` | Un nom ou une version de crate est faux dans `Cargo.toml` | Vérifiez le nom exact et une version existante sur https://crates.io, puis refaites un commit. |

### Le workflow échoue avant la construction

| Message | Solution |
|---|---|
| `Heroku deployment is not configured` | Ajoutez le secret `HEROKU_API_KEY` et la variable `HEROKU_APP_NAME` (étape 2). |
| `Heroku rejected the API key` | Le jeton est faux, expiré ou révoqué : créez-en un nouveau et mettez à jour le secret (**Settings** → **Secrets and variables** → **Actions** → `HEROKU_API_KEY`). |
| `This Heroku account cannot access the app` | Le nom est déjà pris par un autre compte Heroku : choisissez un autre nom dans `HEROKU_APP_NAME`. |

Si la création de l'application échoue, vérifiez que votre compte Heroku a un moyen de paiement, ou créez d'abord l'application avec le bouton Deploy (ou le tableau de bord Heroku) et mettez son nom dans `HEROKU_APP_NAME`.

### « Application error » dans le navigateur

La construction a réussi, mais l'application ne répond pas. Ouvrez les journaux et cherchez un code d'erreur :

| Code | Signification | Que faire |
|---|---|---|
| `R10` Boot timeout | L'application n'écoutait pas sur `$PORT` dans les 60 secondes suivant son démarrage. | Écoutez sur `$PORT` (voir plus haut), jamais sur un port fixe ni sur `127.0.0.1`. |
| `H10` App crashed | Le processus s'est arrêté. | Lisez les lignes juste au-dessus dans les journaux : une panique dans `main`, une route invalide ou en double (Vitesse panique au démarrage en nommant la route fautive), un port fixe… |
| `H14` No web dynos running | Aucun dyno ne tourne pour le processus `web`. | Dans **Resources**, vérifiez que le dyno `web` est allumé. |
| `H12` Request timeout | Une requête a pris plus de 30 secondes. | Heroku coupe les requêtes au bout de 30 secondes : accélérez le handler, ou utilisez `middleware::timeout(...)` pour répondre plus tôt par une `503` propre. |

Une panique dans un handler ne fait pas planter l'application : Vitesse la transforme en réponse `500`. Tous les codes d'erreur d'Heroku sont listés sur https://devcenter.heroku.com/articles/error-codes.

### Autres surprises

- **La première requête est lente** : un dyno Eco dormait ; il se réveille à la première visite.
- **Mes données ont disparu** : les données en mémoire sont perdues à chaque redémarrage. Utilisez une base de données.
- **`PORT=8080` dans le `Dockerfile`** : ce n'est que la valeur par défaut pour les autres plateformes. Sur Heroku, la variable `PORT` définie au démarrage l'emporte.
- **WebSocket fonctionne** : le routeur de Heroku prend en charge les connexions [WebSocket](websocket.md) (utilisez `wss://` avec l'adresse HTTPS de votre application). Il ferme une connexion qui reste silencieuse pendant environ 55 secondes : faites envoyer régulièrement un message ou un `Message::Ping` par le serveur (toutes les 30 secondes, par exemple).
- **HTTP/3, non** : le routeur de Heroku ne transmet pas l'UDP aux dynos, [HTTP/3](http3.md) ne peut donc pas atteindre votre application sur Heroku. Laissez-y la feature `http3` désactivée.

## Pour aller plus loin

- [Docker](docker.md) : construire et lancer la même image sur votre ordinateur ou sur d'autres plateformes.
- [Mise en production](production.md) : limites, journaux, arrêt propre et sécurité.
- [Configuration du serveur](server.md) : `app.run`, adresses d'écoute, workers.
