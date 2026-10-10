//! Le contenu du site : chiffres du benchmark, fonctionnalités, exemples.

pub const GITHUB: &str = "https://github.com/maxlestage/Vitesse";

/// Un scénario du benchmark : requêtes par seconde de chaque serveur, dans
/// l'ordre de [`SERVERS`], et temps CPU par requête de Vitesse.
pub struct Scenario {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub rps: [f64; 5],
    pub cpu: [f64; 5],
}

pub const SERVERS: [&str; 5] = ["Express 5", "Drogon (C++)", "axum", "actix-web", "Vitesse"];

pub const SCENARIOS: [Scenario; 6] = [
    Scenario {
        id: "navigateur",
        label: "Navigateur",
        detail: "GET /json envoyé avec les 12 en-têtes d'un vrai navigateur",
        rps: [5_627.0, 92_041.0, 131_710.0, 179_505.0, 290_478.0],
        cpu: [188.0, 21.7, 15.0, 10.9, 6.8],
    },
    Scenario {
        id: "json",
        label: "JSON",
        detail: "GET /json : sérialisation d'un petit objet",
        rps: [5_765.0, 120_489.0, 165_988.0, 248_987.0, 313_438.0],
        cpu: [184.0, 16.5, 11.9, 7.9, 5.9],
    },
    Scenario {
        id: "params",
        label: "Paramètres",
        detail: "GET /users/:id : routage, paramètre et JSON",
        rps: [5_627.0, 98_486.0, 151_303.0, 209_173.0, 304_323.0],
        cpu: [187.0, 20.1, 13.1, 9.4, 6.2],
    },
    Scenario {
        id: "post",
        label: "POST JSON",
        detail: "POST /echo : lit un corps JSON et le renvoie",
        rps: [4_538.0, 69_876.0, 105_570.0, 170_129.0, 253_941.0],
        cpu: [235.0, 28.4, 18.8, 11.7, 7.6],
    },
    Scenario {
        id: "pipeline",
        label: "Pipeline ×16",
        detail: "GET / : seize requêtes envoyées d'un coup (TechEmpower)",
        rps: [8_352.0, 724_059.0, 213_678.0, 1_272_510.0, 2_781_541.0],
        cpu: [128.0, 2.7, 9.3, 1.6, 0.64],
    },
    Scenario {
        id: "texte",
        label: "Hello World",
        detail: "GET / : la réponse la plus simple possible",
        rps: [5_921.0, 203_244.0, 168_165.0, 274_195.0, 291_415.0],
        cpu: [180.0, 9.7, 11.7, 7.1, 6.1],
    },
];

pub struct Feature {
    pub icon: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    pub tag: &'static str,
}

pub const FEATURES: [Feature; 8] = [
    Feature {
        icon: "engine",
        title: "Moteur HTTP/1.1 maison",
        text: "Têtes analysées en SIMD, en-têtes lus sans copie, un seul read et un seul write par requête.",
        tag: "src/http1.rs",
    },
    Feature {
        icon: "cores",
        title: "Un thread par cœur",
        text: "Chaque cœur a sa boucle d'événements et son socket SO_REUSEPORT. Aucune synchronisation.",
        tag: "app.run(3000)",
    },
    Feature {
        icon: "tree",
        title: "Routeur sans regex",
        text: "Un arbre de segments : paramètres :id, jokers *chemin, priorité statique avant tout.",
        tag: "/users/:id",
    },
    Feature {
        icon: "layers",
        title: "Middlewares à la Express",
        text: "next.run(req).await, et logger, cors, helmet, timeout livrés prêts à l'emploi.",
        tag: "app.middleware(..)",
    },
    Feature {
        icon: "files",
        title: "Fichiers statiques",
        text: "Types MIME, ETag et 304, requêtes partielles pour la vidéo, protection contre ../.",
        tag: "app.static_dir(..)",
    },
    Feature {
        icon: "shield",
        title: "Erreurs sans surprise",
        text: "Le ? partout, des erreurs HTTP typées, et les paniques transformées en 500.",
        tag: "Error::not_found(..)",
    },
    Feature {
        icon: "flask",
        title: "Tests sans réseau",
        text: "Un TestClient qui appelle votre application en mémoire, en une ligne.",
        tag: "client.get(\"/\").await",
    },
    Feature {
        icon: "power",
        title: "Arrêt propre",
        text: "Ctrl+C ou SIGTERM : les requêtes en cours se terminent, puis le serveur s'arrête.",
        tag: "SIGTERM",
    },
];

/// Un exemple « Express à gauche, Vitesse à droite ».
pub struct Example {
    pub label: &'static str,
    pub express: &'static str,
    pub vitesse: &'static str,
}

pub const EXAMPLES: [Example; 4] = [
    Example {
        label: "Hello World",
        express: r#"const express = require("express");
const app = express();

app.get("/", (req, res) => {
  res.send("Hello World!");
});

app.listen(3000);"#,
        vitesse: r#"use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}"#,
    },
    Example {
        label: "Routes",
        express: r#"app.get("/users/:id", (req, res) => {
  const id = Number(req.params.id);
  if (Number.isNaN(id)) {
    return res.status(400).send("id invalide");
  }
  res.json({ id, name: "Ada" });
});"#,
        vitesse: r#"app.get("/users/:id", |req: Request| async move {
    // 400 automatique si ce n'est pas un nombre
    let id: u64 = req.param_as("id")?;
    Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
});"#,
    },
    Example {
        label: "Middleware",
        express: r#"app.use((req, res, next) => {
  if (req.get("authorization") !== "Bearer secret") {
    return res.status(401).json({ error: "connectez-vous" });
  }
  next();
});"#,
        vitesse: r#"app.middleware(|req: Request, next: Next| async move {
    if req.header("authorization") != Some("Bearer secret") {
        return Error::unauthorized("connectez-vous").into_response();
    }
    next.run(req).await
});"#,
    },
    Example {
        label: "JSON",
        express: r#"app.use(express.json());

app.post("/users", (req, res) => {
  const user = req.body;
  res.status(201).json(user);
});"#,
        vitesse: r#"#[derive(Serialize, Deserialize)]
struct User { name: String }

app.post("/users", |req: Request| async move {
    let user: User = req.json().await?;
    Ok::<_, Error>((201, Json(user)))
});"#,
    },
];

/// Les étapes du parcours d'une requête dans le moteur.
pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub metric: &'static str,
}

pub const JOURNEY: [Step; 5] = [
    Step {
        title: "Lecture",
        text: "Un seul appel système lit tout ce qui est arrivé : une requête, ou seize à la fois.",
        metric: "1 read",
    },
    Step {
        title: "Analyse",
        text: "httparse découpe la tête en SIMD. Les en-têtes ne sont que des positions dans le tampon.",
        metric: "0 copie",
    },
    Step {
        title: "Routage",
        text: "L'arbre de segments trouve la route sans regex et écrit les paramètres dans un tampon recyclé.",
        metric: "0 allocation",
    },
    Step {
        title: "Handler",
        text: "S'il répond tout de suite, tout reste synchrone : pas de Future intermédiaire, pas de copie.",
        metric: "8 octets déplacés",
    },
    Step {
        title: "Écriture",
        text: "Statut et en-têtes pré-calculés, Date en cache : toutes les réponses du lot partent ensemble.",
        metric: "1 write",
    },
];
