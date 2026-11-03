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
pub const ECOSYSTEM: [(&str, &str); 5] = [
    ("Active", "https://active-c1883a966650.herokuapp.com/"),
    ("Vitesse", "https://vitesse-f16135654c3d.herokuapp.com/"),
    ("Securite", "https://securite-2db4c1994be7.herokuapp.com/"),
    ("Memoire", "https://memoire-7f4525f1fdaf.herokuapp.com/"),
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
        rps: [86_681.0, 138_917.0, 165_337.0],
        cpu: [22.7, 14.1, 11.0],
    },
    Scenario {
        id: "json",
        rps: [101_206.0, 152_498.0, 173_191.0],
        cpu: [19.3, 12.6, 10.4],
    },
    Scenario {
        id: "params",
        rps: [96_321.0, 152_877.0, 178_731.0],
        cpu: [20.1, 12.8, 10.0],
    },
    Scenario {
        id: "post",
        rps: [81_171.0, 115_069.0, 171_438.0],
        cpu: [24.4, 17.1, 10.8],
    },
    Scenario {
        id: "pipeline",
        rps: [171_985.0, 983_771.0, 1_770_173.0],
        cpu: [11.5, 2.0, 0.97],
    },
    Scenario {
        id: "texte",
        rps: [104_439.0, 174_475.0, 175_956.0],
        cpu: [18.8, 11.0, 10.1],
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
