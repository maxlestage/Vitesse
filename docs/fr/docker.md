# Docker

Une application Vitesse se compile en un seul binaire natif, ce qui la rend facile à livrer dans une petite image Docker. Le dépôt contient un `Dockerfile` multi-étapes prêt à l'emploi (celui qu'utilise Heroku), que vous pouvez lancer sur votre ordinateur ou déployer sur n'importe quelle plateforme de conteneurs.

## Construire et lancer en local

Il vous faut Docker (Docker Desktop sous macOS et Windows, Docker Engine sous Linux). À la racine du dépôt :

```sh
docker build -t mon-app .
docker run --rm -p 8080:8080 mon-app
```

Ouvrez ensuite http://localhost:8080. L'image lance l'application de démonstration ([`examples/demo.rs`](https://github.com/maxlestage/Vitesse/blob/master/examples/demo.rs)) :

```sh
curl localhost:8080/health
curl localhost:8080/api/hello/Ada
curl -X POST localhost:8080/api/todos -H 'content-type: application/json' \
     -d '{"title":"Livrer"}'
curl localhost:8080/api/todos
```

Arrêtez-la avec `Ctrl+C`. Vitesse gère lui-même `SIGINT` et `SIGTERM` : il s'arrête proprement même en tant que processus principal du conteneur (PID 1), sans processus d'init supplémentaire.

> [!TIP]
> `docker stop` envoie `SIGTERM`, puis tue le conteneur au bout de 10 secondes. Vitesse laisse lui aussi jusqu'à 10 secondes aux requêtes en cours pour se terminer : laissez donc un peu plus de temps à Docker, avec `docker run --stop-timeout 15 …` ou `docker stop -t 15 <conteneur>`.

## Ce que fait le Dockerfile

Le [`Dockerfile`](https://github.com/maxlestage/Vitesse/blob/master/Dockerfile) comporte deux étapes :

1. **Construction** (`rust:1-slim-bookworm`) : copie `Cargo.toml`, `Cargo.lock`, `src/` et `examples/`, puis compile un exemple avec `cargo build --release --example $EXAMPLE`.
2. **Exécution** (`debian:bookworm-slim`) : crée un utilisateur non privilégié `vitesse`, copie **uniquement le binaire** dans `/usr/local/bin/server`, définit `ENV PORT=8080` et `EXPOSE 8080`, et le lance avec `CMD ["server"]`.

Le fichier [`.dockerignore`](https://github.com/maxlestage/Vitesse/blob/master/.dockerignore) écarte du contexte de construction `target/`, `site/`, `docs/`, `bench/`, `.git/` et d'autres dossiers inutiles : les constructions sont plus rapides et ne dépendent jamais de ce qui traîne sur votre disque.

### Pourquoi deux étapes ?

L'étape de construction contient toute la chaîne d'outils Rust et les fichiers intermédiaires : plusieurs centaines de mégaoctets inutiles à l'exécution. L'image finale ne garde que Debian slim et votre binaire : environ 115 Mo pour la démo, dont l'essentiel pour la base Debian. Une image plus petite se télécharge plus vite, démarre plus vite et offre moins de prise aux attaques. Vérifiez sa taille avec :

```sh
docker image ls mon-app
```

L'étape d'exécution utilise la même version de Debian que l'étape de construction (Bookworm), car le binaire est lié à la bibliothèque C du système (glibc). On peut aller encore plus loin, par exemple avec une image de base « distroless » ou un binaire entièrement statique compilé pour la cible `musl`, mais ces montages sont à votre charge.

> [!NOTE]
> La construction compile tout depuis zéro avec le profil release (`lto = "fat"`, `codegen-units = 1`) : comptez quelques minutes. La construction n'utilise pas `--locked` : les crates listées dans `Cargo.lock` gardent leurs versions verrouillées, et une dépendance ajoutée à `Cargo.toml` sans mettre à jour `Cargo.lock` (depuis un téléphone, par exemple) est résolue pendant la construction. Committer un `Cargo.lock` à jour reste le moyen d'obtenir des constructions reproductibles.

## Choisir l'exemple

L'argument de construction `EXAMPLE` choisit le fichier de `examples/` à compiler (son nom sans `.rs`). Sa valeur par défaut est `demo` :

```sh
docker build --build-arg EXAMPLE=rest_api -t api .
```

L'image définit `PORT=8080`, mais seule une application qui lit `PORT` s'en sert ; c'est le cas de `demo`. Si un exemple écoute plutôt sur un port fixe (`rest_api`, par exemple, appelle `app.run(3000)`), publiez ce port :

```sh
docker run --rm -p 8080:3000 api
```

Les plateformes comme Heroku construisent l'image sans passer d'argument de construction. Pour changer l'exemple qu'elles utilisent, modifiez la valeur par défaut dans le `Dockerfile` :

```dockerfile
ARG EXAMPLE=mon_app
```

Pour qu'une application fonctionne partout, lisez le port dans l'environnement :

```rust
let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
app.run(port) // un simple port écoute sur toutes les interfaces (0.0.0.0)
```

## Votre propre projet

Si votre application est une crate classique (avec un `src/main.rs`) plutôt qu'un exemple, le principe est le même. Remplacez `mon-app` par le nom de votre paquet :

```dockerfile
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked && cp target/release/mon-app /server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home app
COPY --from=build /server /usr/local/bin/server
USER app
ENV PORT=8080
EXPOSE 8080
CMD ["server"]
```

Ajoutez un fichier `.dockerignore` contenant au moins `target/` et `.git/`. Ici, `--locked` fait échouer la construction si `Cargo.lock` n'est pas à jour, ce qui garantit des constructions reproductibles ; retirez-le si vous modifiez les dépendances sans régénérer le fichier de verrouillage.

### Fichiers statiques et autres ressources

L'image d'exécution ne contient que le binaire. Si votre application sert un dossier (`app.static_dir("/assets", "public")`, voir [Fichiers statiques](static-files.md)) ou lit des fichiers à l'exécution, copiez-les dans l'étape d'exécution et définissez le répertoire de travail, car les chemins relatifs partent du répertoire courant :

```dockerfile
WORKDIR /app
COPY public ./public
```

Placez ces lignes dans l'étape d'exécution, avant `USER`, et vérifiez que `.dockerignore` n'exclut pas le dossier.

## Variables d'environnement

- **`PORT`** : le port d'écoute. L'image utilise `8080` par défaut, et la plupart des plateformes le remplacent. En local : `docker run -e PORT=9000 -p 9000:9000 mon-app`.
- **Vos propres réglages** : passez-les avec `-e NOM=valeur` ou `--env-file .env`, et lisez-les avec `std::env::var("NOM")`.

> [!WARNING]
> N'écrivez jamais de secrets dans le `Dockerfile` (`ENV API_TOKEN=…`) : toute personne qui a l'image peut les lire, par exemple avec `docker history`. Passez-les au démarrage du conteneur.

## HTTP/3 : publier le port UDP

`EXPOSE` et `-p` désignent du TCP, sauf indication contraire. Si votre application sert [HTTP/3](http3.md) (la feature `http3`) sur le port UDP 443, déclarez et publiez aussi ce port, et donnez son certificat au conteneur :

```dockerfile
EXPOSE 8080
EXPOSE 443/udp
```

```sh
docker run --rm -p 127.0.0.1:8080:8080 -p 443:443/udp \
  -v /etc/mon-app/tls:/etc/mon-app/tls:ro mon-app
```

Ici, le port TCP 8080 n'est joignable que depuis l'hôte, pour le reverse proxy qui gère HTTPS, tandis que le port UDP 443 est public. Les fichiers montés doivent être lisibles par l'utilisateur du conteneur (par défaut, les clés de Let's Encrypt ne sont lisibles que par root). Si le conteneur écoute sur un autre port UDP, par exemple avec `-p 443:8443/udp`, annoncez le port public avec `.alt_svc_port(443)`.

HTTP/3 a besoin que l'UDP arrive jusqu'au conteneur : Heroku ne route pas l'UDP, et des plateformes comme Cloud Run ou Render terminent HTTP/3 à leur périphérie quand elles le proposent. Sur ces plateformes, laissez la feature désactivée.

## Journaux et vérifications de santé

- **Journaux** : `middleware::logger()` écrit une ligne par requête sur la sortie standard, sans codes de couleur quand elle n'est pas reliée à un terminal. Lisez-les avec `docker logs -f <conteneur>`. Les causes des erreurs `5xx` partent sur la sortie d'erreur.
- **Vérifications de santé** : la démo expose `GET /health`. L'image slim ne contient pas `curl` : une instruction `HEALTHCHECK` basée sur `curl` ne fonctionnera donc pas telle quelle. Préférez la vérification HTTP de votre plateforme (sondes Kubernetes, Fly.io, Render, Cloud Run…), dirigée vers `/health`.

## Déployer la même image ailleurs

La plupart des plateformes de conteneurs donnent le port dans la variable `PORT` : l'image fonctionne donc en général sans modification. Les interfaces changent souvent : vérifiez la documentation de chaque plateforme.

| Plateforme | En bref |
|---|---|
| Heroku | Voir [Déployer sur Heroku depuis un téléphone](heroku-mobile.md). Heroku construit lui-même l'image à partir de `heroku.yml`. |
| Render | Créez un service web à partir de votre dépôt avec l'environnement Docker ; Render construit le `Dockerfile` et définit `PORT`. |
| Railway | Détecte le `Dockerfile` et fournit `PORT`. |
| Fly.io | `fly launch` détecte le `Dockerfile` ; vérifiez que `internal_port` dans `fly.toml` correspond au port écouté par votre application (`8080` avec cette image). |
| Google Cloud Run | `gcloud run deploy --source .` construit le `Dockerfile` ; Cloud Run définit `PORT` (8080 par défaut). |

Pour pousser vous-même l'image vers un registre :

```sh
docker tag mon-app ghcr.io/<vous>/mon-app:latest
docker push ghcr.io/<vous>/mon-app:latest
```

## Images multi-architectures

Une image est construite pour l'architecture du processeur de la machine qui la construit. Sur un Mac Apple Silicon (arm64), une image construite en local ne tournera pas sur un serveur x86-64 (amd64) comme ceux d'Heroku. Demandez explicitement la plateforme cible :

```sh
docker build --platform linux/amd64 -t mon-app .
```

Pour publier une seule image pour les deux architectures, utilisez `buildx` :

```sh
docker buildx build --platform linux/amd64,linux/arm64 \
  -t ghcr.io/<vous>/mon-app:latest --push .
```

Les images de base `rust` et `debian` existent pour les deux architectures. Construire pour une architecture étrangère passe par de l'émulation, ce qui peut beaucoup ralentir la compilation Rust.

## Pour aller plus loin

- [Mise en production](production.md) : reverse proxy, arrêt propre, limites, sécurité.
- [Configuration du serveur](server.md) : adresses d'écoute, workers, threads.
