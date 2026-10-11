# HTTPS et HTTP/2

Vitesse peut servir HTTPS lui-même, avec TLS sur TCP, et parler HTTP/2 aux clients qui le proposent : c'est le cas de tous les navigateurs. Aucun reverse proxy n'est nécessaire, ni pour l'un ni pour l'autre, même si un proxy reste une bonne option (voir [la fin de cette page](#tls-en-direct-ou-reverse-proxy-)). Les routes, les middlewares et les handlers restent les mêmes : seule la configuration change.

## Activer les features `tls` et `http2`

Ce sont deux features Cargo à activer explicitement :

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2"] }
```

| Feature | Ajoute | Embarque |
|---|---|---|
| `tls` | `app.tls(...)` et le module `vitesse::tls` : HTTPS, TLS 1.2 et 1.3 | [rustls](https://crates.io/crates/rustls) (avec la bibliothèque cryptographique ring) et tokio-rustls : tout est en Rust, sans OpenSSL à installer |
| `http2` | HTTP/2 : à travers TLS avec `tls`, et en clair pour les clients qui commencent en HTTP/2 | [h2](https://crates.io/crates/h2), l'implémentation de HTTP/2 qu'utilise hyper |

Les deux features sont indépendantes. `tls` seule sert HTTPS en HTTP/1.1 ; `http2` seule sert HTTP/2 sans TLS ([h2c](#http2-en-clair-h2c)) ; ensemble, elles donnent aux navigateurs ce qu'ils attendent d'un site moderne.

## Un exemple complet

```rust
use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |req: Request| async move {
        // "Bonjour en HTTP/2.0 (https) !" dans un navigateur
        format!("Bonjour en {:?} ({}) !", req.version(), req.protocol())
    });

    // Un certificat et sa clé privée, en PEM.
    app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);

    app.run(443) // HTTPS sur le port TCP 443
}
```

Une fois `app.tls(...)` configuré, chaque connexion TCP sur le port commence par une poignée de main TLS (un client dispose de 10 secondes pour la terminer). Avec la feature `http2`, les clients qui proposent HTTP/2 pendant la poignée de main (ALPN) l'obtiennent, et les autres restent en HTTP/1.1, sur le même port.

HTTPS fonctionne avec les trois façons de démarrer le serveur : `app.run(addr)`, `app.listen(addr).await` et `app.bind(addr).await?` (voir [Configuration du serveur](server.md#trois-façons-de-démarrer-le-serveur)). Le port ne parle que TLS : une requête `http://` simple n'y reçoit aucune réponse. Pour rediriger HTTP vers HTTPS, ouvrez aussi le port 80 ([plus bas](#rediriger-http-vers-https)).

## Les certificats

Vitesse a besoin d'un certificat pour votre domaine et de sa clé privée. Trois constructeurs :

| Constructeur | Usage |
|---|---|
| `Tls::from_pem_files(chaîne, clé)` | Deux fichiers PEM, par exemple `fullchain.pem` et `privkey.pem` de Let's Encrypt |
| `Tls::from_pem(octets_chaîne, octets_clé)` | La même chose, depuis la mémoire (un gestionnaire de secrets, une variable d'environnement…) |
| `Tls::from_rustls(config)` | Votre propre `rustls::ServerConfig` : certificats clients, plusieurs domaines… |

`tls.config()` renvoie la configuration rustls utilisée. La chaîne contient d'abord le certificat du serveur, puis les certificats intermédiaires : c'est exactement le contenu de `fullchain.pem`. La clé privée peut être au format PKCS#8, SEC1 (EC) ou PKCS#1 (RSA), en PEM. Un fichier illisible ou invalide fait renvoyer à `from_pem_files` une `io::Error`, avant le démarrage du serveur. `from_pem` et `from_pem_files` acceptent TLS 1.2 et 1.3.

### Let's Encrypt

[certbot](https://certbot.eff.org) obtient des certificats gratuits auprès de Let's Encrypt. Comme Vitesse occupe le port 80 (pour la [redirection](#rediriger-http-vers-https)), utilisez le mode webroot : certbot écrit un fichier dans un dossier, et Let's Encrypt vient le chercher en HTTP.

```sh
sudo certbot certonly --webroot -w /var/www/acme -d example.com -d www.example.com
```

L'application de redirection ci-dessous sert ce dossier sous `/.well-known/acme-challenge/`. Le certificat couvre chaque nom passé avec `-d`, et arrive dans `/etc/letsencrypt/live/example.com/` (`fullchain.pem` et `privkey.pem`).

> [!IMPORTANT]
> Le certificat est chargé une seule fois, au démarrage. Let's Encrypt renouvelle les certificats tous les deux mois environ : redémarrez l'application après chaque renouvellement, depuis un hook de certbot (`certbot renew --deploy-hook "systemctl restart mon-app"`). Les clés de Let's Encrypt ne sont lisibles que par root : copiez-les dans un dossier lisible par l'utilisateur du service, depuis le même hook.

### Votre propre configuration rustls

Pour des certificats clients (TLS mutuel), un certificat par domaine, ou le rechargement des certificats sans redémarrage, construisez vous-même la configuration rustls. `vitesse::tls::rustls` réexporte la version de rustls qu'utilise Vitesse : les types correspondent toujours. Ici, seuls les clients munis d'un certificat signé par votre propre autorité peuvent entrer :

```rust
use std::sync::Arc;

use vitesse::prelude::*;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::tls::rustls::server::WebPkiClientVerifier;
use vitesse::tls::{Tls, rustls};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chain = CertificateDer::pem_file_iter("fullchain.pem")?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file("privkey.pem")?;

    // L'autorité qui signe les certificats de vos clients.
    let mut client_ca = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_file_iter("clients-ca.pem")? {
        client_ca.add(cert?)?;
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(client_ca), provider.clone()).build()?;
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()? // TLS 1.2 et 1.3
        .with_client_cert_verifier(verifier) // pas de certificat valide, pas de connexion
        .with_single_cert(chain, key)?;

    let mut app = App::new();
    app.get("/", |_| async { "Bonjour, client de confiance !" });
    app.tls(Tls::from_rustls(config)); // ALPN "h2" et "http/1.1" ajoutés s'ils manquent
    app.run(443)?;
    Ok(())
}
```

Si votre configuration ne liste aucun protocole ALPN, `Tls::from_rustls` propose `h2` (avec la feature `http2`) et `http/1.1`. Si vous réglez `alpn_protocols` vous-même, incluez `b"h2"` pour garder HTTP/2.

**Plusieurs domaines.** Un seul certificat peut couvrir plusieurs noms (`-d example.com -d example.org` avec certbot) : c'est l'option la plus simple. Pour utiliser un certificat par domaine, laissez rustls le choisir d'après le nom que demande le client (SNI) :

```rust
use std::sync::Arc;

use vitesse::prelude::*;
use vitesse::tls::rustls::pki_types::pem::PemObject;
use vitesse::tls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use vitesse::tls::rustls::server::ResolvesServerCertUsingSni;
use vitesse::tls::rustls::sign::CertifiedKey;
use vitesse::tls::{Tls, rustls};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let load = |dir: &str| -> Result<CertifiedKey, Box<dyn std::error::Error>> {
        let chain = CertificateDer::pem_file_iter(format!("{dir}/fullchain.pem"))?
            .collect::<Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_file(format!("{dir}/privkey.pem"))?;
        Ok(CertifiedKey::from_der(chain, key, &provider)?)
    };

    let mut certificates = ResolvesServerCertUsingSni::new();
    certificates.add("example.com", load("/etc/mon-app/tls/example.com")?)?;
    certificates.add("example.org", load("/etc/mon-app/tls/example.org")?)?;

    let config = rustls::ServerConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(certificates));

    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("Bonjour de {} !", req.hostname().unwrap_or("?"))
    });
    app.tls(Tls::from_rustls(config));
    app.run(443)?;
    Ok(())
}
```

Pour recharger les certificats sans redémarrer, écrivez votre propre résolveur : un type qui implémente `rustls::server::ResolvesServerCert` et relit les fichiers quand ils changent.

### En développement

Pour essayer HTTPS sur votre machine, générez un certificat auto-signé pour `localhost` :

- en Rust, avec la crate [rcgen](https://crates.io/crates/rcgen), au démarrage (en dépendance de développement ou dans un exemple) ;
- ou avec mkcert, qui crée un certificat reconnu par votre propre machine : `mkcert -install`, puis `mkcert localhost` écrit `localhost.pem` et `localhost-key.pem`, à charger avec `Tls::from_pem_files("localhost.pem", "localhost-key.pem")`.

```rust
// Cargo.toml : rcgen = "0.14"
let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
    .map_err(std::io::Error::other)?;
app.tls(Tls::from_pem(
    cert.cert.pem().as_bytes(),
    cert.signing_key.serialize_pem().as_bytes(),
)?);
```

## Le port 443 et la redirection de HTTP vers HTTPS

Les navigateurs attendent HTTPS sur le port **443**, et envoient les personnes qui tapent `example.com` sur le port **80**, en HTTP simple. Servez l'application sur le port 443, et lancez sur le port 80 une deuxième application, minuscule, qui redirige tout vers HTTPS.

### Rediriger HTTP vers HTTPS

```rust
use vitesse::prelude::*;
use vitesse::tls::Tls;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // L'application, en HTTPS.
    let mut app = App::new();
    app.middleware(middleware::helmet()); // Strict-Transport-Security, entre autres
    app.get("/", |_| async { "Bonjour en HTTPS !" });
    app.tls(Tls::from_pem_files("fullchain.pem", "privkey.pem")?);

    // Une application minuscule sur le port 80, qui envoie tout le monde vers HTTPS.
    let mut redirect = App::new();
    // Les challenges de Let's Encrypt (certbot --webroot -w /var/www/acme).
    redirect.static_dir("/.well-known/acme-challenge", "/var/www/acme/.well-known/acme-challenge");
    redirect.fallback(|req: Request| async move {
        let host = req.hostname().unwrap_or("example.com").to_owned();
        let path = req.uri().path_and_query().map_or("/", |pq| pq.as_str());
        Redirect::permanent(format!("https://{host}{path}"))
    });

    // Les deux s'arrêtent ensemble sur Ctrl+C / SIGTERM ; si l'une ne peut pas démarrer, l'erreur est renvoyée.
    tokio::try_join!(app.listen(443), redirect.listen(80))?;
    Ok(())
}
```

Il faut alors tokio dans votre `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`). `app.listen` fait tourner l'application sur votre runtime ; avec `app.bind(...).await?`, vous obtenez deux `Server`, et vous attendez `https.run()` et `http.run()` ensemble de la même façon.

> [!TIP]
> `listen` n'utilise pas le mode un thread par cœur de `app.run`. Pour le garder pour l'application principale, démarrez plutôt la redirection sur son propre thread : `std::thread::spawn(move || redirect.run(80));`, puis `app.run(443)`.

`helmet()` ajoute `Strict-Transport-Security` : les navigateurs vont alors directement en HTTPS pendant un an, sans passer par la redirection.

### Les ports inférieurs à 1024

Sous Linux, seul root peut ouvrir les ports inférieurs à 1024. Ne lancez pas l'application en root : sous systemd, ajoutez `AmbientCapabilities=CAP_NET_BIND_SERVICE` à la section `[Service]` (voir [Mise en production](production.md#lancer-lapplication-comme-un-service-avec-systemd)). Dans un conteneur, vous pouvez aussi écouter sur des ports non privilégiés, comme 8443 et 8080, et les publier sur 443 et 80 (`docker run -p 443:8443 -p 80:8080`).

## HTTP/2 sur TLS : les navigateurs

Les navigateurs n'utilisent HTTP/2 que sur TLS, et le choisissent pendant la poignée de main TLS, grâce à ALPN. Avec les features `tls` et `http2`, il n'y a rien d'autre à configurer : Vitesse propose `h2` et `http/1.1`, et chaque client choisit le meilleur qu'il connaît.

Ce que HTTP/2 change pour votre application :

- plusieurs requêtes circulent en même temps sur une seule connexion (multiplexage) : une page charge ses styles, ses scripts et ses images en parallèle, sans ouvrir six connexions ;
- chaque requête est traitée dans sa propre tâche, par les mêmes middlewares globaux, les mêmes routes, les mêmes middlewares de routeur et le même `app.on_error` qu'en HTTP/1.1 ;
- `req.version()` vaut `HTTP/2.0` (`vitesse::http::Version::HTTP_2`), et l'en-tête `Host` est reconstruit à partir du pseudo-en-tête `:authority` : `req.header("host")`, `req.host()` et `req.hostname()` fonctionnent donc comme en HTTP/1.1 ;
- les corps de requête sont transmis au handler en flux, comme en HTTP/1.1, dans la limite de `app.body_limit` (ou de la [limite de la route](requests.md#taille-maximale-du-corps-et-413)) ;
- les réponses sont aussi envoyées en flux ([Server-Sent Events](responses.md#server-sent-events), fichiers, `Body::from_stream`) ; Vitesse ajoute `date` et `content-length` (quand la taille est connue), et retire les en-têtes propres à HTTP/1.1 (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`).

## HTTP/2 en clair (h2c)

Avec la feature `http2` et sans TLS, un client qui ouvre la connexion directement en HTTP/2 (avec la préface HTTP/2, en « prior knowledge ») obtient HTTP/2, sur le même port que HTTP/1.1. Les navigateurs ne le font jamais, mais certains clients et proxys le font :

- les clients gRPC, et Envoy ;
- Caddy avec un upstream `h2c://` (`reverse_proxy h2c://127.0.0.1:3000`), Nginx avec `grpc_pass` ;
- les plateformes qui parlent HTTP/2 au conteneur, comme l'option HTTP/2 de bout en bout de Cloud Run ;
- `curl --http2-prior-knowledge`.

```rust
use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |req: Request| async move { format!("{:?}", req.version()) });
    app.run(3000) // HTTP/1.1 et h2c sur le même port (feature `http2`)
}
```

```sh
curl http://localhost:3000/                            # HTTP/1.1
curl --http2-prior-knowledge http://localhost:3000/    # HTTP/2.0
```

L'autre façon de démarrer HTTP/2 en clair, une requête HTTP/1.1 avec `Upgrade: h2c`, n'est pas prise en charge : ces requêtes reçoivent simplement une réponse en HTTP/1.1, ce que le protocole permet.

## Ce que dit la requête

| | `req.version()` | `req.protocol()` | `req.secure()` |
|---|---|---|---|
| HTTP/1.1 | `HTTP/1.1` | `"http"` | `false` |
| HTTPS, HTTP/1.1 | `HTTP/1.1` | `"https"` | `true` |
| HTTPS, HTTP/2 | `HTTP/2.0` | `"https"` | `true` |
| HTTP/2 en clair (h2c) | `HTTP/2.0` | `"http"` | `false` |
| [HTTP/3](http3.md) | `HTTP/3.0` | `"https"` | `true` |

Derrière un reverse proxy qui gère TLS, la connexion entre le proxy et Vitesse est en HTTP simple : déclarez le proxy avec `app.trust_proxy(...)`, et `req.protocol()` et `req.secure()` suivent son en-tête `X-Forwarded-Proto` (voir [Mise en production](production.md#connaître-ladresse-ip-du-client)).

## WebSocket sur TLS

Les routes [WebSocket](websocket.md) fonctionnent en HTTPS sans aucun changement : les clients se connectent avec `wss://` au lieu de `ws://`.

```js
const socket = new WebSocket("wss://example.com/chat");
```

La connexion WebSocket elle-même utilise HTTP/1.1 à l'intérieur de TLS : les navigateurs ouvrent pour elle une connexion HTTP/1.1 séparée. WebSocket sur HTTP/2 (RFC 8441) n'est pas pris en charge : une réponse `101 Switching Protocols` sur une connexion HTTP/2 devient `505 HTTP Version Not Supported`.

## HTTPS et HTTP/3 ensemble

Avec la feature `http3` en plus, un seul binaire sert tous les protocoles. Les réponses envoyées en HTTPS (HTTP/1.1 et HTTP/2) portent alors l'en-tête `alt-svc`, et les navigateurs passent directement à [HTTP/3](http3.md), sans proxy devant :

```toml
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["tls", "http2", "http3"] }
```

```rust
use vitesse::http3::Http3;
use vitesse::prelude::*;
use vitesse::tls::Tls;

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.get("/", |req: Request| async move {
        format!("Bonjour en {:?} !", req.version()) // HTTP/1.1, HTTP/2.0 ou HTTP/3.0
    });

    // Le même certificat pour les deux.
    let (cert, key) = ("/etc/mon-app/tls/fullchain.pem", "/etc/mon-app/tls/privkey.pem");
    app.tls(Tls::from_pem_files(cert, key)?); // TCP 443 : HTTPS, HTTP/2 et HTTP/1.1
    app.http3(Http3::from_pem_files(cert, key)?); // UDP 443 : HTTP/3

    app.run(443)
}
```

Ouvrez le port TCP 443 **et** le port UDP 443 dans le pare-feu (`sudo ufw allow 443`, qui couvre les deux). Voir [HTTP/3 et QUIC](http3.md) pour les options.

## Limites et ce qui n'est pas pris en charge

| HTTP/2 | Valeur |
|---|---|
| Requêtes en cours sur une même connexion | 256 |
| Taille des en-têtes d'une requête | 64 Kio |
| Corps qu'un client peut envoyer avant que le handler le lise (contrôle de flux) | 256 Kio par requête, 1 Mio par connexion |
| Connexion inactive | Fermée après le délai de keep-alive ([`app.keep_alive`](server.md#délais-et-connexions), 60 s par défaut) |
| Arrêt propre | Le client reçoit un `GOAWAY`, et les requêtes en cours disposent du [`shutdown_timeout`](server.md#arrêt-propre) pour se terminer |
| Poignée de main TLS | 10 secondes au plus |

`app.max_connections` compte les connexions, pas les requêtes : en HTTP/2, une connexion peut transporter 256 requêtes à la fois. Pour plafonner les requêtes elles-mêmes, utilisez [`middleware::concurrency_limit`](middleware.md#concurrency_limit).

Non pris en charge :

- **`Upgrade: h2c`** : ces requêtes reçoivent une réponse en HTTP/1.1 ; utilisez le « prior knowledge », ou TLS ;
- **le server push** : les navigateurs l'ont de toute façon abandonné ;
- **WebSocket sur HTTP/2** : WebSocket utilise HTTP/1.1, avec ou sans TLS.

## Essayer en local

L'exemple `examples/https.rs` du dépôt génère un certificat auto-signé pour `localhost` au démarrage et écoute sur le port 3443 :

```sh
cargo run --example https --features tls,http2

curl -k https://localhost:3443/            # Hello over HTTP/2.0 (https), from 127.0.0.1!
curl -k --http1.1 https://localhost:3443/  # Hello over HTTP/1.1 (https), from 127.0.0.1!
```

`-k` accepte le certificat auto-signé, et `--http1.1` interdit HTTP/2. Avec `curl -v`, la poignée de main affiche le protocole choisi (`ALPN: server accepted h2`). Dans un navigateur, ouvrez `https://localhost:3443/` et acceptez l'avertissement.

## TLS en direct ou reverse proxy ?

Les deux fonctionnent ; choisissez en fonction de votre hébergement.

**Vitesse sert HTTPS lui-même** (cette page) :

- un processus de moins, un saut réseau de moins, et rien d'autre à configurer ;
- HTTPS, HTTP/2 et HTTP/3 depuis un seul binaire, avec `alt-svc` envoyé directement aux navigateurs ;
- `req.ip()` est la vraie adresse du client, sans `trust_proxy` ;
- mais c'est à vous de gérer les certificats : le renouvellement avec certbot, puis un redémarrage (une fraction de seconde, pendant laquelle les nouvelles connexions échouent), ou votre propre résolveur rustls pour les recharger ;
- ainsi que la redirection depuis HTTP, les ports privilégiés, et la limitation de débit et la compression avec les middlewares de Vitesse (`rate_limit`, `compress`).

**Un reverse proxy (Caddy, Nginx) devant** :

- Caddy obtient et renouvelle les certificats automatiquement, sans rien redémarrer ;
- une même machine peut héberger plusieurs applications et domaines derrière les mêmes ports 80 et 443 ;
- le proxy peut répartir la charge entre plusieurs instances et permettre des déploiements sans interruption ;
- mais c'est un composant de plus, et Vitesse a besoin de [`trust_proxy`](production.md#connaître-ladresse-ip-du-client) pour connaître l'adresse du client.

**Sur une plateforme** (Heroku, Cloud Run, Render, Fly.io…), TLS est géré à la périphérie de la plateforme : laissez la feature `tls` désactivée, et écoutez sur le `PORT` qu'elle vous donne.

La configuration avec un reverse proxy est décrite dans [Mise en production](production.md#derrière-un-reverse-proxy).
