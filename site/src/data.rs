//! Les données du site qui ne dépendent pas de la langue : chiffres du
//! benchmark, icônes et étiquettes de code (les textes sont dans `i18n` et
//! `labels`).

pub const GITHUB: &str = "https://github.com/maxlestage/Vitesse";

/// La commande d'installation du héros.
pub const INSTALL: &str = "cargo add vitesse";

/// Le bouton « Deploy to Heroku » (Heroku Button).
pub const HEROKU_DEPLOY: &str =
    "https://www.heroku.com/deploy?template=https://github.com/maxlestage/Vitesse";

/// Un scénario du benchmark : requêtes par seconde et temps CPU par requête
/// de chaque serveur, dans l'ordre de [`SERVERS`].
pub struct Scenario {
    pub id: &'static str,
    pub rps: [f64; 5],
    pub cpu: [f64; 5],
}

pub const SERVERS: [&str; 5] = ["Express 5", "Drogon (C++)", "axum", "actix-web", "Vitesse"];

pub const SCENARIOS: [Scenario; 6] = [
    Scenario {
        id: "navigateur",
        rps: [5_627.0, 92_041.0, 131_710.0, 179_505.0, 290_478.0],
        cpu: [188.0, 21.7, 15.0, 10.9, 6.8],
    },
    Scenario {
        id: "json",
        rps: [5_765.0, 120_489.0, 165_988.0, 248_987.0, 313_438.0],
        cpu: [184.0, 16.5, 11.9, 7.9, 5.9],
    },
    Scenario {
        id: "params",
        rps: [5_627.0, 98_486.0, 151_303.0, 209_173.0, 304_323.0],
        cpu: [187.0, 20.1, 13.1, 9.4, 6.2],
    },
    Scenario {
        id: "post",
        rps: [4_538.0, 69_876.0, 105_570.0, 170_129.0, 253_941.0],
        cpu: [235.0, 28.4, 18.8, 11.7, 7.6],
    },
    Scenario {
        id: "pipeline",
        rps: [8_352.0, 724_059.0, 213_678.0, 1_272_510.0, 2_781_541.0],
        cpu: [128.0, 2.7, 9.3, 1.6, 0.64],
    },
    Scenario {
        id: "texte",
        rps: [5_921.0, 203_244.0, 168_165.0, 274_195.0, 291_415.0],
        cpu: [180.0, 9.7, 11.7, 7.1, 6.1],
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
