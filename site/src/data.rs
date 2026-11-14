//! Les données du site qui ne dépendent pas de la langue : chiffres du
//! benchmark, icônes et étiquettes de code (les textes sont dans `i18n` et
//! `labels`).

pub const CRATES: &str = "https://github.com/maxlestage/Vitesse";
pub const DOCS_RS: &str = "https://docs.rs/vitesse";

/// La licence, servie par le site lui-même (une copie du LICENSE du dépôt) : propriétaire,
/// tous droits réservés.
pub const LICENSE_PATH: &str = "/assets/LICENSE.txt";

/// Qui a conçu et développé Vitesse et ses projets frères.
pub const AUTHOR: &str = "Maxime Nathan Lestage";

/// Les projets frères de Vitesse, du même auteur : (nom, site). Le pied de page les
/// présente dans cet ordre ; `i18n::Texts::footer_projects` décrit chacun dans le même ordre.
pub const ECOSYSTEM: [(&str, &str); 6] = [
    ("Active", "https://active-c1883a966650.herokuapp.com/"),
    ("Vitesse", "https://vitesse-f16135654c3d.herokuapp.com/"),
    ("Securite", "https://securite-2db4c1994be7.herokuapp.com/"),
    ("Memoire", "https://memoire-7f4525f1fdaf.herokuapp.com/"),
    ("Nuage DB", "https://github.com/maxlestage/Nuage"),
    ("Paquets", "https://paquets-90dfc06b6fea.herokuapp.com/"),
];

/// La commande d'installation du héros.
pub const INSTALL: &str = "cargo add vitesse --git https://github.com/maxlestage/Vitesse";

/// Un scénario du benchmark : requêtes par seconde et temps CPU par requête
/// de chaque serveur, dans l'ordre de [`SERVERS`].
pub struct Scenario {
    pub id: &'static str,
    pub rps: [f64; 3],
    pub cpu: [f64; 3],
}

/// Les serveurs mesurés dans la même session (le 3 novembre 2026, bench/run.sh),
/// Vitesse en dernier ; [`AXUM`], [`ACTIX`] et [`VITESSE`] sont leurs places.
pub const SERVERS: [&str; 3] = ["axum", "actix-web", "Vitesse"];
pub const AXUM: usize = 0;
pub const ACTIX: usize = 1;
pub const VITESSE: usize = 2;

pub const SCENARIOS: [Scenario; 6] = [
    Scenario {
        id: "navigateur",
        rps: [179_989.0, 256_421.0, 349_424.0],
        cpu: [10.96, 7.71, 5.35],
    },
    Scenario {
        id: "json",
        rps: [215_706.0, 346_652.0, 350_972.0],
        cpu: [9.08, 5.64, 5.11],
    },
    Scenario {
        id: "params",
        rps: [204_665.0, 304_271.0, 359_307.0],
        cpu: [9.64, 6.43, 5.15],
    },
    Scenario {
        id: "post",
        rps: [164_843.0, 227_272.0, 365_321.0],
        cpu: [12.0, 8.66, 5.27],
    },
    Scenario {
        id: "pipeline",
        rps: [366_673.0, 1_612_135.0, 3_288_486.0],
        cpu: [5.38, 1.23, 0.51],
    },
    Scenario {
        id: "texte",
        rps: [233_459.0, 359_229.0, 347_836.0],
        cpu: [8.38, 5.37, 5.05],
    },
];

/// L'icône et l'étiquette de code de chaque fonctionnalité.
pub const FEATURES: [(&str, &str); 8] = [
    ("engine", "src/http1.rs"),
    ("cores", "app.run(3000)"),
    ("tree", "/users/:id"),
    ("layers", "app.middleware(..)"),
    ("files", "app.static_dir(..)"),
    ("shield", "Error::not_found(..)"),
    ("flask", "client.get(\"/\").await"),
    ("power", "SIGTERM"),
];
