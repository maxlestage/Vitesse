# Documentation de Vitesse

Tout ce qu'il faut pour créer des applications web rapides avec Vitesse, le framework à la Express.js pour Rust : de votre première route jusqu'au déploiement en production. Vous pouvez aussi lire ces pages sur le site : https://maxlestage.github.io/Vitesse/fr/docs/

## Premiers pas

- [Introduction](introduction.md) : ce qu'est Vitesse, sa philosophie et ce qu'il propose.
- [Installation](installation.md) : ajouter Vitesse à un projet avec Cargo.
- [Votre première application](first-app.md) : construire et lancer une petite application, pas à pas.

## L'essentiel

- [Routage](routing.md) : routes, méthodes HTTP, paramètres de chemin et jokers.
- [Lire la requête](requests.md) : paramètres, query string, en-têtes, cookies et corps (JSON, formulaires).
- [Répondre](responses.md) : texte, JSON, HTML, statuts, en-têtes, cookies, redirections et fichiers.
- [Middlewares](middleware.md) : middlewares globaux et par route, `next`, et les middlewares fournis.
- [Routeurs et sous-applications](routers.md) : regrouper des routes avec `Router` et les monter sous un préfixe.
- [État partagé](state.md) : partager une configuration, des compteurs ou un pool de connexions entre les requêtes.
- [Gestion des erreurs](errors.md) : `vitesse::Error`, l'opérateur `?`, réponses d'erreur personnalisées et paniques.
- [Fichiers statiques](static-files.md) : servir un dossier de fichiers avec cache, requêtes partielles et protections intégrées.
- [WebSocket](websocket.md) : des connexions bidirectionnelles en temps réel avec `app.ws`, du serveur d'écho au salon de discussion.

## Aller plus loin

- [Tests](testing.md) : tester votre application en mémoire avec `TestClient`, sans réseau.
- [Configuration du serveur](server.md) : `run`, `listen` et `bind`, adresses d'écoute, workers, limites et arrêt.
- [HTTP/3 et QUIC](http3.md) : servir votre application en HTTP/3 à côté de HTTP/1.1, certificats et déploiement.
- [Performances](performance.md) : pourquoi Vitesse est rapide, les benchmarks et des conseils de réglage.
- [Venir d'Express](from-express.md) : la table de correspondance Express → Vitesse et des conseils de migration.

## Déploiement

- [Déployer sur Heroku depuis un téléphone](heroku-mobile.md) : le bouton Deploy en un geste et les déploiements automatiques avec GitHub Actions, le tout depuis un téléphone.
- [Docker](docker.md) : construire et lancer l'image multi-étapes, puis la déployer sur des plateformes de conteneurs.
- [Mise en production](production.md) : compilation release, reverse proxy, systemd, arrêt propre, limites, sécurité et supervision.

## Référence

- [Aide-mémoire de l'API](cheatsheet.md) : toute l'API en un coup d'œil.
- [FAQ et limites](faq.md) : questions fréquentes, et ce que Vitesse ne fait pas (encore).

## Autres ressources

- Référence de l'API générée à partir du code source : https://docs.rs/vitesse
- Code source, issues et exemples : https://github.com/maxlestage/Vitesse
- Aussi disponible en [anglais](https://github.com/maxlestage/Vitesse/blob/master/docs/en/README.md) et en [espagnol](https://github.com/maxlestage/Vitesse/blob/master/docs/es/README.md).
