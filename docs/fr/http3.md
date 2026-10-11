# HTTP/3 et QUIC

HTTP/3 transporte HTTP sur QUIC, un transport construit sur UDP avec TLS 1.3 intégré : les connexions s'ouvrent plus vite, un paquet perdu ne retarde que la requête à laquelle il appartient, et une connexion survit à un changement de réseau (du Wi-Fi aux données mobiles, par exemple). Vitesse peut servir votre application en HTTP/3 à côté de HTTP/1.1 (et à côté de [HTTPS et HTTP/2](https.md)) : les mêmes routes, les mêmes middlewares et les mêmes handlers, avec une ligne de configuration en plus.

## Activer la feature `http3`

HTTP/3 est une feature Cargo à activer explicitement :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["http3"] }
```

Elle apporte [quinn](https://crates.io/crates/quinn) (QUIC), [h3](https://crates.io/crates/h3) (HTTP/3) et rustls (TLS 1.3, avec la bibliothèque cryptographique ring) : tout est en Rust, sans OpenSSL à installer. La feature est désactivée par défaut, car elle ajoute un temps de compilation dont la plupart des applications derrière un proxy n'ont pas besoin.

## Un exemple complet

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        format!("Bonjour en {:?} !", req.version()) // HTTP/1.1 ou HTTP/3.0
    });

    // QUIC utilise toujours TLS : un certificat et sa clé privée, en PEM.
    app.http3(Http3::from_pem_files("fullchain.pem", "privkey.pem")?);

    // HTTP/1.1 sur le port TCP 443, HTTP/3 sur le port UDP 443.
    app.run(443)
}
```

Ici, le port TCP 443 parle HTTP/1.1 en clair, ce qu'attend un reverse proxy placé devant. Pour y servir aussi HTTPS, ajoutez `app.tls(...)` avec le même certificat (feature `tls`) : voir [HTTPS et HTTP/3 ensemble](https.md#https-et-http3-ensemble).

`app.http3(...)` ajoute un socket UDP à côté du socket TCP. Chaque requête, quel que soit son protocole, passe par les mêmes middlewares globaux, les mêmes routes, les mêmes middlewares de routeur et le même `app.on_error`. Dans un handler, `req.version()` vaut `HTTP/3.0` pour une requête arrivée en HTTP/3, et l'en-tête `Host` est reconstruit à partir du pseudo-en-tête `:authority` : `req.header("host")` et `req.hostname()` fonctionnent donc comme en HTTP/1.1.

HTTP/3 fonctionne avec les trois façons de démarrer le serveur : `app.run(addr)`, `app.listen(addr).await` et `app.bind(addr).await?` (voir [plus bas](#bind-et-http3_addr)). Le socket UDP écoute sur la même adresse IP que le TCP.

## Les certificats

QUIC ne fonctionne pas sans TLS : Vitesse a besoin d'un certificat pour votre domaine et de sa clé privée. Trois constructeurs :

| Constructeur | Usage |
|---|---|
| `Http3::from_pem_files(chaîne, clé)` | Deux fichiers PEM, par exemple `fullchain.pem` et `privkey.pem` de Let's Encrypt |
| `Http3::from_pem(octets_chaîne, octets_clé)` | La même chose, depuis la mémoire (un gestionnaire de secrets, une variable d'environnement…) |
| `Http3::from_rustls(config)` | Votre propre `rustls::ServerConfig` |

La chaîne contient d'abord le certificat du serveur, puis les certificats intermédiaires : c'est exactement le contenu de `fullchain.pem`. La clé privée peut être au format PKCS#8, SEC1 (EC) ou PKCS#1 (RSA), en PEM. Un fichier illisible ou invalide fait renvoyer à `from_pem_files` une `io::Error`, avant le démarrage du serveur.

> [!IMPORTANT]
> Le certificat est chargé une seule fois, au démarrage. Let's Encrypt renouvelle les certificats tous les deux mois environ : redémarrez l'application après chaque renouvellement, par exemple depuis un hook de certbot (`certbot renew --deploy-hook "systemctl restart mon-app"`).

### Votre propre configuration rustls

Pour des certificats clients (TLS mutuel), plusieurs domaines sur un même serveur, ou le rechargement des certificats sans redémarrage, construisez vous-même la configuration rustls. `vitesse::http3::rustls` réexporte la version de rustls qu'utilise Vitesse : les types correspondent toujours.

```rust
use std::sync::Arc;

use vitesse::http3::rustls::pki_types::pem::PemObject;
use vitesse::http3::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::http3::{Http3, rustls};
use vitesse::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chain = CertificateDer::pem_file_iter("fullchain.pem")?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("privkey.pem")?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])? // QUIC exige TLS 1.3
        .with_no_client_auth()
        .with_single_cert(chain, key)?;

    let mut app = App::new();
    app.get("/", |_| async { "Bonjour !" });
    app.http3(Http3::from_rustls(tls)); // le protocole ALPN "h3" est ajouté s'il manque
    app.run(443)?;
    Ok(())
}
```

### En développement

Pour essayer HTTP/3 sur votre machine, générez un certificat auto-signé pour `localhost` :

- en Rust, avec la crate [rcgen](https://crates.io/crates/rcgen), au démarrage (en dépendance de développement ou dans un exemple) ;
- ou avec mkcert, qui crée un certificat reconnu par votre propre machine : `mkcert -install`, puis `mkcert localhost` écrit `localhost.pem` et `localhost-key.pem`.

```rust
// Cargo.toml : rcgen = "0.14"
let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
    .map_err(std::io::Error::other)?;
app.http3(Http3::from_pem(
    cert.cert.pem().as_bytes(),
    cert.signing_key.serialize_pem().as_bytes(),
)?);
```

L'exemple `examples/http3.rs` du dépôt fait exactement cela, et écoute sur le port 4433. Essayez-le avec un curl compilé avec la prise en charge de HTTP/3 (`curl --version` cite `HTTP3` parmi ses fonctionnalités) :

```sh
cargo run --example http3 --features http3

curl -k --http3-only https://localhost:4433/   # Hello over HTTP/3.0!
curl -i http://localhost:4433/                 # HTTP/1.1, avec l'en-tête alt-svc
```

`-k` accepte le certificat auto-signé.

## Les options

| Méthode de `Http3` | Par défaut | Effet |
|---|---|---|
| `.port(u16)` | Le même numéro que le port TCP | Le port UDP d'écoute |
| `.alt_svc(bool)` | `true` | Ajoute l'en-tête `alt-svc` aux réponses HTTP/1.1 et HTTP/2 |
| `.alt_svc_port(u16)` | Le port UDP réellement ouvert | Le port public annoncé dans `alt-svc`, quand les clients joignent le serveur sur un autre port (derrière un proxy, Docker ou du NAT : en général `443`) |

```rust
app.http3(
    Http3::from_pem_files("fullchain.pem", "privkey.pem")?
        .port(8443)          // Vitesse écoute sur UDP 8443...
        .alt_svc_port(443),  // ...que les clients joignent sur le port 443 (NAT, Docker)
);
```

## Comment les navigateurs passent à HTTP/3

Un navigateur ne commence jamais en HTTP/3. Il se connecte d'abord en TCP avec HTTPS, puis passe à HTTP/3 pour les requêtes suivantes si la réponse lui signale que le service existe, avec l'en-tête `Alt-Svc`. Vitesse ajoute cet en-tête à chaque réponse HTTP/1.1 et HTTP/2 :

```http
alt-svc: h3=":443"; ma=86400
```

Le navigateur retient alors pendant une journée (`ma=86400`) que ce site répond en HTTP/3 sur le port UDP 443.

Les navigateurs ne font confiance à `Alt-Svc` que sur un site en HTTPS : un navigateur qui joint Vitesse en HTTP simple ne bascule jamais. Deux organisations fonctionnent en production :

- **Vitesse sert HTTPS lui-même**, avec la feature `tls` : `app.tls(...)` et `app.http3(...)` avec le même certificat, ports TCP et UDP 443. Les navigateurs reçoivent `alt-svc` sur les réponses HTTPS de Vitesse et passent directement à HTTP/3, sans aucun proxy. Voir [HTTPS et HTTP/3 ensemble](https.md#https-et-http3-ensemble).
- **Derrière un reverse proxy TLS** (Caddy, Nginx) sur le port TCP 443, qui transmet les requêtes à Vitesse en HTTP/1.1, pendant que Vitesse reçoit directement le port UDP 443, avec le même certificat que le proxy. `alt-svc` annonce le port `443` : le proxy transmet l'en-tête aux navigateurs, qui parlent ensuite directement à Vitesse en HTTP/3. Cette organisation est détaillée [plus bas](#en-production-derrière-caddy-ou-nginx).

> [!TIP]
> Caddy sert HTTP/3 par défaut, avec ses propres certificats. Si Caddy est votre proxy, le plus simple est de le laisser faire HTTP/3 lui-même et de ne pas activer la feature. Activez-la quand votre proxy ne fait pas HTTP/3, ou quand vous voulez que Vitesse réponde aux clients HTTP/3 sans passer par le proxy.

Les clients hors navigateur (`curl --http3`, applications mobiles, autres services) n'ont pas besoin d'`Alt-Svc` : ils peuvent utiliser HTTP/3 directement s'ils savent que le serveur le prend en charge.

## En production derrière Caddy ou Nginx

Si Vitesse sert lui-même HTTPS, vous n'avez pas besoin de cette section : voir [HTTPS et HTTP/2](https.md). Avec un proxy, Vitesse sert HTTP/1.1 au proxy sur un port TCP privé, et HTTP/3 à Internet sur le port UDP 443 :

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |_| async { "Bonjour !" });

    // Le même certificat que le proxy (des copies lisibles par l'utilisateur du service).
    app.http3(
        Http3::from_pem_files("/etc/mon-app/tls/fullchain.pem", "/etc/mon-app/tls/privkey.pem")?
            .port(443),         // HTTP/3 : UDP 443, directement depuis Internet
    );

    // HTTP/1.1 pour le proxy sur TCP 3000. Le socket UDP utilise la même IP :
    // écoutez donc sur 0.0.0.0, et gardez TCP 3000 fermé dans le pare-feu.
    app.run("0.0.0.0:3000")
}
```

`alt-svc` annonce le port UDP réellement ouvert, ici `443`. Si les clients joignent Vitesse à travers une traduction de port (Docker `-p 443:8443/udp`, du NAT, une redirection du pare-feu), écoutez sur le port interne avec `.port(8443)` et annoncez le port public avec `.alt_svc_port(443)`.

Côté proxy, gardez la configuration habituelle (voir [Mise en production](production.md#derrière-un-reverse-proxy)) :

- **Nginx** : TLS sur TCP 443 et `proxy_pass` vers `127.0.0.1:3000`, sans `listen 443 quic`, pour que le port UDP 443 reste libre pour Vitesse. Nginx transmet l'en-tête `alt-svc` de Vitesse aux navigateurs.
- **Caddy** : désactivez son propre HTTP/3 pour libérer le port UDP 443, dans les options globales en tête du `Caddyfile` :

  ```text
  {
  	servers {
  		protocols h1 h2
  	}
  }
  ```

Sous systemd, un utilisateur non privilégié ne peut pas ouvrir le port 443 : ajoutez `AmbientCapabilities=CAP_NET_BIND_SERVICE` à la section `[Service]`. Les clés de Let's Encrypt ne sont lisibles que par root : copiez-les dans un dossier lisible par l'utilisateur du service (depuis le hook de certbot qui redémarre aussi l'application).

> [!WARNING]
> Les requêtes HTTP/3 ne passent pas par le proxy : leurs en-têtes `X-Forwarded-For` et `X-Forwarded-Proto`, s'il y en a, viennent directement du client. Déclarez le proxy par son adresse, `app.trust_proxy("loopback")`, et Vitesse ne croit ces en-têtes que lorsqu'ils viennent de lui : `req.ip()` est alors la vraie adresse du client pour les deux protocoles (voir [Mise en production](production.md#connaître-ladresse-ip-du-client)). De même, ce que fait le proxy (compression, limitation de débit, journaux d'accès, taille maximale des corps) ne s'applique pas aux requêtes HTTP/3 : utilisez les middlewares de Vitesse (`compress`, `rate_limit`, `body_limit`) et `app.body_limit`, qui s'appliquent à tous les protocoles.

## `bind` et `http3_addr`

Avec `app.bind`, `server.http3_addr()` renvoie l'adresse UDP (`None` sans `app.http3`), ce qui est pratique avec le port `0` :

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |_| async { "ok" });
    app.http3(Http3::from_pem_files("cert.pem", "key.pem")?);

    let server = app.bind("127.0.0.1:0").await?;
    println!("HTTP/1.1 sur tcp://{}", server.local_addr());
    if let Some(udp) = server.http3_addr() {
        println!("HTTP/3 sur udp://{udp}"); // le même numéro de port qu'en TCP
    }

    server.run().await
}
```

`server.run()` et `server.with_graceful_shutdown(signal)`, comme `app.run` et `app.listen`, arrêtent les deux protocoles ensemble : à l'arrêt, les connexions HTTP/3 reçoivent un `GOAWAY` (le client n'y ouvre plus de nouvelle requête) et les requêtes en cours disposent de `shutdown_timeout` (10 secondes par défaut) pour se terminer, comme en HTTP/1.1 (voir [Configuration du serveur](server.md#arrêt-propre)).

## Ce qui change, et ce qui n'est pas (encore) pris en charge

- **Les corps de requête** arrivent au handler en flux, au fur et à mesure, comme en HTTP/1.1 et en HTTP/2 : `req.take_body()` est un vrai flux, `field.save(chemin)` écrit un [fichier envoyé](requests.md#envois-de-fichiers-multipart) sur le disque à mesure qu'il arrive, et la limite de taille du corps est celle de la requête, `app.body_limit` ou le [`middleware::body_limit`](requests.md#taille-maximale-du-corps-et-413) de la route (`413` au-delà).
- **Les réponses** sont envoyées en flux comme en HTTP/1.1 (`Body::from_stream`, fichiers…). Vitesse ajoute `date` et `content-length` (quand la taille est connue), et retire les en-têtes propres à HTTP/1.1 (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`).
- **Les threads** : avec `app.run` en mode un thread par cœur (le défaut sous Linux), HTTP/3 est servi par un seul thread, à côté des threads HTTP/1.1. Si l'essentiel de votre trafic passe en HTTP/3, comparez avec `app.thread_per_core(false)`.
- **WebSocket sur HTTP/3** (RFC 9220) n'est pas pris en charge : le crate h3 ne l'implémente pas, et les navigateurs ne s'en servent pas. Ils ouvrent leurs [WebSockets](websocket.md#websocket-sur-http2) en HTTP/2 ou en HTTP/1.1, sur TCP, même quand la page est arrivée en HTTP/3.
- **Le 0-RTT** (*early data*) n'est pas pris en charge, volontairement : il permet à un client qui revient d'envoyer sa première requête avant la fin de la poignée de main, mais un attaquant qui enregistre cette requête peut la rejouer, et un `POST` rejoué pourrait, par exemple, passer deux fois la même commande. Vitesse attend la fin de la poignée de main, qui ne coûte qu'un aller-retour avec QUIC.
- HTTP/2 est disponible en TCP, avec la feature `http2` : voir [HTTPS et HTTP/2](https.md).

## Pare-feu et hébergement

QUIC utilise **UDP** : ouvrir le port TCP 443 ne suffit pas.

- **Pare-feu** : ouvrez le port UDP, par exemple `sudo ufw allow 443/udp`, ou une règle UDP dans le groupe de sécurité de votre fournisseur cloud.
- **Docker** : déclarez le port avec `EXPOSE 443/udp` et publiez-le avec `-p 443:443/udp` (voir [Docker](docker.md#http3--publier-le-port-udp)).
- **Heroku** : son routeur ne transmet pas l'UDP aux dynos, HTTP/3 n'est donc pas disponible sur Heroku. Laissez la feature désactivée.
- **Cloud Run, Render et les plateformes du même genre** terminent HTTP/3 à leur périphérie, quand elles le proposent, et parlent HTTP/1.1 à votre conteneur : la feature y est inutile, là aussi.

Si HTTP/3 est bloqué quelque part sur le chemin (un pare-feu d'entreprise qui rejette l'UDP, par exemple), les navigateurs se rabattent sans bruit sur HTTP/1.1 ou HTTP/2 en TCP : activer HTTP/3 ne coupe jamais personne.
