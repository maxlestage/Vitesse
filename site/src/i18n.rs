//! Les langues du site (français, anglais, espagnol) : tous les textes,
//! le format des nombres, et le choix de la langue (mémorisé, sinon déduit du
//! navigateur).
//!
//! Chaque langue est une constante [`Texts`] : oublier une traduction est une
//! erreur de compilation.

use yew::prelude::*;

use crate::dom;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lang {
    Fr,
    En,
    Es,
}

const STORAGE_KEY: &str = "vitesse-lang";

impl Lang {
    /// Dans l'ordre du sélecteur.
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Fr, Lang::Es];

    pub fn code(self) -> &'static str {
        self.pick(["fr", "en", "es"])
    }

    pub fn name(self) -> &'static str {
        self.pick(["Français", "English", "Español"])
    }

    pub fn locale(self) -> &'static str {
        self.pick(["fr-FR", "en-US", "es-ES"])
    }

    /// Choisit la valeur de cette langue dans `[fr, en, es]`.
    pub fn pick<T: Copy>(self, values: [T; 3]) -> T {
        values[match self {
            Lang::Fr => 0,
            Lang::En => 1,
            Lang::Es => 2,
        }]
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code.get(..2)?.to_ascii_lowercase().as_str() {
            "fr" => Some(Lang::Fr),
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    pub fn texts(self) -> &'static Texts {
        self.pick([&FR, &EN, &ES])
    }

    /// La langue choisie la dernière fois, sinon celle du navigateur, sinon
    /// l'anglais.
    pub fn detect() -> Lang {
        let win = dom::window();
        let stored = win
            .local_storage()
            .ok()
            .flatten()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .and_then(|c| Lang::from_code(&c));
        if let Some(lang) = stored {
            return lang;
        }
        let navigator = win.navigator();
        navigator
            .languages()
            .iter()
            .filter_map(|l| l.as_string())
            .chain(navigator.language())
            .find_map(|l| Lang::from_code(&l))
            .unwrap_or(Lang::En)
    }

    pub fn save(self) {
        if let Some(storage) = dom::window().local_storage().ok().flatten() {
            let _ = storage.set_item(STORAGE_KEY, self.code());
        }
    }
}

/// La langue courante, partagée par contexte.
#[derive(Clone, PartialEq)]
pub struct LangCtx {
    pub lang: Lang,
    pub set: Callback<Lang>,
}

#[hook]
pub fn use_lang() -> LangCtx {
    use_context::<LangCtx>().expect("LangCtx manquant")
}

/// Formate un nombre selon la langue : `2 781 541` / `2,781,541` /
/// `2.781.541`, et `6,1` / `6.1`.
pub fn number(lang: Lang, value: f64, decimals: usize) -> String {
    let (group, point) = lang.pick([('\u{202f}', ','), (',', '.'), ('.', ',')]);
    let s = format!("{value:.decimals$}");
    let (int, frac) = s
        .split_once('.')
        .map_or((s.as_str(), None), |(i, f)| (i, Some(f)));
    let mut out = String::new();
    let digits: Vec<char> = int.chars().collect();
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(group);
        }
        out.push(*c);
    }
    if let Some(f) = frac {
        out.push(point);
        out.push_str(f);
    }
    out
}

/// Un titre et son texte.
pub struct Item {
    pub title: &'static str,
    pub text: &'static str,
}

pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub metric: &'static str,
}

/// Un exemple « Express à gauche, Vitesse à droite ».
pub struct Example {
    pub label: &'static str,
    pub express: &'static str,
    pub vitesse: &'static str,
}

const fn item(title: &'static str, text: &'static str) -> Item {
    Item { title, text }
}

const fn step(title: &'static str, text: &'static str, metric: &'static str) -> Step {
    Step {
        title,
        text,
        metric,
    }
}

pub struct Texts {
    // Méta
    pub meta_title: &'static str,
    pub docs_meta: &'static str,
    // Navigation
    pub nav_links: [&'static str; 4],
    pub nav_docs: &'static str,
    pub nav_aria: &'static str,
    pub brand_aria: &'static str,
    pub menu: &'static str,
    pub lang_aria: &'static str,
    // Héros
    pub hero_badge: &'static str,
    pub hero_title: [&'static str; 3],
    pub hero_sub: [&'static str; 2],
    pub hero_words: [&'static str; 5],
    pub hero_lead: [&'static str; 2],
    pub hero_start: &'static str,
    pub hero_perf: &'static str,
    pub copy: &'static str,
    pub copied: &'static str,
    pub copy_aria: &'static str,
    pub scroll: &'static str,
    pub scroll_aria: &'static str,
    // Bandeaux
    pub marquee_aria: &'static str,
    pub marquee: [&'static str; 8],
    // Chiffres
    pub stats_label: &'static str,
    pub stats_title: &'static str,
    pub stats: [Item; 4],
    // Benchmark
    pub bench_label: &'static str,
    pub bench_title: &'static str,
    pub bench_lead: &'static str,
    pub scenarios: [Item; 6],
    pub bench_vs: &'static str,
    pub bench_note: &'static str,
    pub bench_reproduce: &'static str,
    // Fonctionnalités
    pub features_label: &'static str,
    pub features_title: &'static str,
    pub features_lead: &'static str,
    pub features: [Item; 8],
    // Sous le capot
    pub journey_label: &'static str,
    pub journey_title: &'static str,
    pub journey: [Step; 5],
    pub journey_end: &'static str,
    pub journey_cta: &'static str,
    // D'Express à Vitesse
    pub compare_label: &'static str,
    pub compare_title: &'static str,
    pub compare_lead: &'static str,
    pub examples: [Example; 4],
    // Démarrer
    pub start_label: &'static str,
    pub start_title: &'static str,
    pub start_steps: [&'static str; 3],
    pub start_listening: &'static str,
    pub start_docs: &'static str,
    pub start_heroku: &'static str,
    // Appel à l'action
    pub cta_title: [&'static str; 2],
    pub cta_start: &'static str,
    pub cta_star: &'static str,
    // Pied de page
    pub footer_eyebrow: &'static str,
    pub footer_lead: &'static str,
    pub footer_cta: &'static str,
    pub footer_cols: [&'static str; 3],
    pub footer_project: [&'static str; 4],
    pub footer_docs: [&'static str; 4],
    pub footer_community: [&'static str; 4],
    pub footer_copyright: &'static str,
    pub clock: &'static str,
    pub totop: &'static str,
    pub totop_aria: &'static str,
    pub totop_cursor: &'static str,
    // Documentation
    pub docs_title: &'static str,
    pub docs_lead: &'static str,
    pub docs_search: &'static str,
    pub docs_search_hint: &'static str,
    /// `{}` est remplacé par la recherche.
    pub docs_no_results: &'static str,
    pub docs_toc: &'static str,
    pub docs_prev: &'static str,
    pub docs_next: &'static str,
    pub docs_edit: &'static str,
    pub docs_loading: &'static str,
    pub docs_missing: &'static str,
    pub docs_read_en: &'static str,
    pub docs_menu: &'static str,
    pub docs_api: &'static str,
    pub docs_quick: &'static str,
    /// `{}` est remplacé par le nombre de pages.
    pub docs_pages: &'static str,
    /// Note, astuce, important, attention, prudence.
    pub callouts: [&'static str; 5],
}

// ----- Exemples de code (les commentaires et messages sont traduits) ----------

const HELLO_EXPRESS: &str = r#"const express = require("express");
const app = express();

app.get("/", (req, res) => {
  res.send("Hello World!");
});

app.listen(3000);"#;

const HELLO_VITESSE: &str = r#"use vitesse::prelude::*;

fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.get("/", |_| async { "Hello World!" });

    app.run(3000)
}"#;

const JSON_EXPRESS: &str = r#"app.use(express.json());

app.post("/users", (req, res) => {
  const user = req.body;
  res.status(201).json(user);
});"#;

const JSON_VITESSE: &str = r#"#[derive(Serialize, Deserialize)]
struct User { name: String }

app.post("/users", |req: Request| async move {
    let user: User = req.json().await?;
    Ok::<_, Error>((201, Json(user)))
});"#;

// ----- Français ------------------------------------------------------------------

pub const FR: Texts = Texts {
    meta_title: "Vitesse — le framework web Rust à la Express",
    docs_meta: "Documentation Vitesse",
    nav_links: [
        "Performances",
        "Fonctionnalités",
        "Venir d'Express",
        "Démarrer",
    ],
    nav_docs: "Docs",
    nav_aria: "Navigation principale",
    brand_aria: "Vitesse, retour en haut",
    menu: "Menu",
    lang_aria: "Langue",
    hero_badge: "Nouveau · 2,78 millions de requêtes/s sur 2 cœurs",
    hero_title: ["Le confort d'Express.", "La vitesse de", "Rust."],
    hero_sub: ["Un framework web ", " pour Rust."],
    hero_words: ["natif", "minimaliste", "ultra-rapide", "sûr", "familier"],
    hero_lead: [
        "Vitesse reprend l'API que vous connaissez déjà — ",
        " — et la propulse avec son propre moteur HTTP/1.1. Plus rapide qu'actix-web, axum et Drogon.",
    ],
    hero_start: "Commencer",
    hero_perf: "Voir les performances",
    copy: "Copier",
    copied: "Copié !",
    copy_aria: "Copier la commande",
    scroll: "Défiler",
    scroll_aria: "Défiler vers la suite",
    marquee_aria: "Points forts",
    marquee: [
        "L'API d'Express",
        "2,78 M req/s",
        "Moteur HTTP/1.1 maison",
        "Zéro regex",
        "Un thread par cœur",
        "100 % Rust",
        "WebSocket · HTTP/3",
        "Tests sans réseau",
    ],
    stats_label: "En chiffres",
    stats_title: "Des chiffres qui parlent d'eux-mêmes.",
    stats: [
        item(
            "requêtes par seconde",
            "Sur deux cœurs, en pipeline : le test « plaintext » du TechEmpower.",
        ),
        item(
            "plus rapide qu'actix-web",
            "Avec une vraie requête de navigateur, et ×2,2 en pipeline.",
        ),
        item(
            "de CPU par requête",
            "Contre 7,1 µs pour actix-web et 11,7 µs pour axum.",
        ),
        item(
            "plus rapide qu'Express",
            "De 49 à 56 fois selon le scénario, sur le même matériel.",
        ),
    ],
    bench_label: "Performances",
    bench_title: "Plus rapide qu'actix\u{2011}web. Bien plus rapide que le reste.",
    bench_lead: "Mêmes routes, même machine, même session : Express, Drogon (C++), axum, actix-web et Vitesse face au même générateur de charge.",
    scenarios: [
        item(
            "Navigateur",
            "GET /json envoyé avec les 12 en-têtes d'un vrai navigateur",
        ),
        item("JSON", "GET /json : sérialisation d'un petit objet"),
        item("Paramètres", "GET /users/:id : routage, paramètre et JSON"),
        item("POST JSON", "POST /echo : lit un corps JSON et le renvoie"),
        item(
            "Pipeline ×16",
            "GET / : seize requêtes envoyées d'un coup (TechEmpower)",
        ),
        item("Hello World", "GET / : la réponse la plus simple possible"),
    ],
    bench_vs: "face à",
    bench_note: "VM 4 vCPU : serveur sur 2 cœurs, wrk sur les 2 autres, 128 connexions keep-alive, 10 s par scénario. À droite, le temps CPU consommé par le serveur pour chaque requête. ",
    bench_reproduce: "Reproduire le benchmark",
    features_label: "Fonctionnalités",
    features_title: "Tout ce qu'Express sait faire. En Rust, sans compromis.",
    features_lead: "Un cœur volontairement petit, comme Express, et tout ce qu'il faut pour une vraie application.",
    features: [
        item(
            "Moteur HTTP/1.1 maison",
            "Têtes analysées en SIMD, en-têtes lus sans copie, un seul read et un seul write par requête.",
        ),
        item(
            "Un thread par cœur",
            "Chaque cœur a sa boucle d'événements et son socket SO_REUSEPORT. Aucune synchronisation.",
        ),
        item(
            "Routeur sans regex",
            "Un arbre de segments : paramètres :id, jokers *chemin, priorité statique avant tout.",
        ),
        item(
            "Middlewares à la Express",
            "next.run(req).await, et logger, cors, helmet, timeout livrés prêts à l'emploi.",
        ),
        item(
            "Fichiers statiques",
            "Types MIME, ETag et 304, requêtes partielles pour la vidéo, protection contre ../.",
        ),
        item(
            "Erreurs sans surprise",
            "Le ? partout, des erreurs HTTP typées, et les paniques transformées en 500.",
        ),
        item(
            "Tests sans réseau",
            "Un TestClient qui appelle votre application en mémoire, en une ligne.",
        ),
        item(
            "Arrêt propre",
            "Ctrl+C ou SIGTERM : les requêtes en cours se terminent, puis le serveur s'arrête.",
        ),
    ],
    journey_label: "Sous le capot",
    journey_title: "Le voyage d'une requête, en six microsecondes.",
    journey: [
        step(
            "Lecture",
            "Un seul appel système lit tout ce qui est arrivé : une requête, ou seize à la fois.",
            "1 read",
        ),
        step(
            "Analyse",
            "httparse découpe la tête en SIMD. Les en-têtes ne sont que des positions dans le tampon.",
            "0 copie",
        ),
        step(
            "Routage",
            "L'arbre de segments trouve la route sans regex et écrit les paramètres dans un tampon recyclé.",
            "0 allocation",
        ),
        step(
            "Handler",
            "S'il répond tout de suite, tout reste synchrone : pas de Future intermédiaire, pas de copie.",
            "8 octets déplacés",
        ),
        step(
            "Écriture",
            "Statut et en-têtes pré-calculés, Date en cache : toutes les réponses du lot partent ensemble.",
            "1 write",
        ),
    ],
    journey_end: "de CPU par requête, du premier octet lu au dernier octet écrit.",
    journey_cta: "Voir le code",
    compare_label: "D'Express à Vitesse",
    compare_title: "Vous savez déjà écrire du Vitesse.",
    compare_lead: "Mêmes idées, mêmes noms, même façon de penser. La différence : un compilateur qui vérifie tout, et des performances natives.",
    examples: [
        Example {
            label: "Hello World",
            express: HELLO_EXPRESS,
            vitesse: HELLO_VITESSE,
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
            express: JSON_EXPRESS,
            vitesse: JSON_VITESSE,
        },
    ],
    start_label: "Démarrer",
    start_title: "En ligne en trente secondes.",
    start_steps: ["Ajoutez Vitesse", "Écrivez votre application", "Lancez-la"],
    start_listening: "⚡ Vitesse écoute sur http://localhost:3000",
    start_docs: "Lire la documentation",
    start_heroku: "Déployer sur Heroku",
    cta_title: ["Prêt à aller", "plus vite ?"],
    cta_start: "Démarrer un projet",
    cta_star: "Étoiler sur GitHub",
    footer_eyebrow: "Le framework web Rust à la Express",
    footer_lead: "Écrivez du code comme avec Express. Servez-le à la vitesse du métal.",
    footer_cta: "Commencer maintenant",
    footer_cols: ["Projet", "Documentation", "Communauté"],
    footer_project: ["Code source", "Benchmark", "Exemples", "Ce site (Yew)"],
    footer_docs: [
        "Guide",
        "Venir d'Express",
        "Pourquoi c'est rapide",
        "Déployer sur Heroku",
    ],
    footer_community: ["Signaler un bug", "Issues", "Pull requests", "Historique"],
    footer_copyright: "© 2026 Vitesse. Fait en Rust avec Yew et WebAssembly.",
    clock: "Heure locale",
    totop: "RETOUR EN HAUT • RETOUR EN HAUT • ",
    totop_aria: "Retour en haut",
    totop_cursor: "Haut",
    docs_title: "Documentation",
    docs_lead: "Tout ce qu'il faut pour construire, tester et déployer une application Vitesse, pas à pas.",
    docs_search: "Rechercher dans la documentation…",
    docs_search_hint: "Appuyez sur / pour chercher",
    docs_no_results: "Aucun résultat pour « {} ».",
    docs_toc: "Sur cette page",
    docs_prev: "Précédent",
    docs_next: "Suivant",
    docs_edit: "Améliorer cette page sur GitHub",
    docs_loading: "Chargement…",
    docs_missing: "Cette page n'est pas encore disponible dans cette langue.",
    docs_read_en: "Lire la version anglaise",
    docs_menu: "Sommaire",
    docs_api: "Référence complète de l'API (docs.rs)",
    docs_quick: "Pour bien démarrer",
    docs_pages: "{} pages",
    callouts: ["Remarque", "Astuce", "Important", "Attention", "Prudence"],
};

// ----- English -------------------------------------------------------------------

pub const EN: Texts = Texts {
    meta_title: "Vitesse — the Express-style web framework for Rust",
    docs_meta: "Vitesse docs",
    nav_links: ["Performance", "Features", "From Express", "Get started"],
    nav_docs: "Docs",
    nav_aria: "Main navigation",
    brand_aria: "Vitesse, back to top",
    menu: "Menu",
    lang_aria: "Language",
    hero_badge: "New · 2.78 million requests/s on 2 cores",
    hero_title: ["The comfort of Express.", "The speed of", "Rust."],
    hero_sub: ["A ", " web framework for Rust."],
    hero_words: ["native", "minimalist", "blazing-fast", "safe", "familiar"],
    hero_lead: [
        "Vitesse takes the API you already know — ",
        " — and powers it with its own HTTP/1.1 engine. Faster than actix-web, axum and Drogon.",
    ],
    hero_start: "Get started",
    hero_perf: "See the benchmarks",
    copy: "Copy",
    copied: "Copied!",
    copy_aria: "Copy the command",
    scroll: "Scroll",
    scroll_aria: "Scroll down",
    marquee_aria: "Highlights",
    marquee: [
        "The Express API",
        "2.78M req/s",
        "Home-grown HTTP/1.1 engine",
        "Zero regex",
        "One thread per core",
        "100% Rust",
        "WebSocket · HTTP/3",
        "Network-free tests",
    ],
    stats_label: "By the numbers",
    stats_title: "Numbers that speak for themselves.",
    stats: [
        item(
            "requests per second",
            "On two cores, pipelined: the TechEmpower “plaintext” test.",
        ),
        item(
            "faster than actix-web",
            "With a real browser request, and ×2.2 when pipelined.",
        ),
        item(
            "of CPU per request",
            "Versus 7.1 µs for actix-web and 11.7 µs for axum.",
        ),
        item(
            "faster than Express",
            "49 to 56 times depending on the scenario, on the same hardware.",
        ),
    ],
    bench_label: "Performance",
    bench_title: "Faster than actix\u{2011}web. Far faster than the rest.",
    bench_lead: "Same routes, same machine, same session: Express, Drogon (C++), axum, actix-web and Vitesse against the same load generator.",
    scenarios: [
        item(
            "Browser",
            "GET /json sent with the 12 headers of a real browser",
        ),
        item("JSON", "GET /json: serialising a small object"),
        item(
            "Parameters",
            "GET /users/:id: routing, a parameter and JSON",
        ),
        item(
            "POST JSON",
            "POST /echo: reads a JSON body and sends it back",
        ),
        item(
            "Pipeline ×16",
            "GET /: sixteen requests sent at once (TechEmpower)",
        ),
        item("Hello World", "GET /: the simplest possible response"),
    ],
    bench_vs: "vs",
    bench_note: "4 vCPU VM: server on 2 cores, wrk on the other 2, 128 keep-alive connections, 10 s per scenario. On the right, the CPU time the server spends on each request. ",
    bench_reproduce: "Reproduce the benchmark",
    features_label: "Features",
    features_title: "Everything Express can do. In Rust, without compromise.",
    features_lead: "A deliberately small core, like Express, and everything a real application needs.",
    features: [
        item(
            "Home-grown HTTP/1.1 engine",
            "Heads parsed with SIMD, headers read without copying, a single read and a single write per request.",
        ),
        item(
            "One thread per core",
            "Each core has its own event loop and SO_REUSEPORT socket. No synchronisation.",
        ),
        item(
            "Regex-free router",
            "A segment tree: :id parameters, *path wildcards, static routes always win.",
        ),
        item(
            "Express-style middleware",
            "next.run(req).await, plus logger, cors, helmet and timeout ready to use.",
        ),
        item(
            "Static files",
            "MIME types, ETag and 304, range requests for video, protection against ../.",
        ),
        item(
            "Errors without surprises",
            "? everywhere, typed HTTP errors, and panics turned into 500s.",
        ),
        item(
            "Network-free tests",
            "A TestClient that calls your app in memory, in one line.",
        ),
        item(
            "Graceful shutdown",
            "Ctrl+C or SIGTERM: in-flight requests finish, then the server stops.",
        ),
    ],
    journey_label: "Under the hood",
    journey_title: "The journey of a request, in six microseconds.",
    journey: [
        step(
            "Read",
            "A single system call reads everything that arrived: one request, or sixteen at once.",
            "1 read",
        ),
        step(
            "Parse",
            "httparse splits the head with SIMD. Headers are just positions in the buffer.",
            "0 copies",
        ),
        step(
            "Route",
            "The segment tree finds the route without regex and writes the parameters into a recycled buffer.",
            "0 allocations",
        ),
        step(
            "Handler",
            "If it answers right away, everything stays synchronous: no intermediate Future, no copy.",
            "8 bytes moved",
        ),
        step(
            "Write",
            "Precomputed status and headers, cached Date: every response in the batch leaves together.",
            "1 write",
        ),
    ],
    journey_end: "of CPU per request, from the first byte read to the last byte written.",
    journey_cta: "See the code",
    compare_label: "From Express to Vitesse",
    compare_title: "You already know how to write Vitesse.",
    compare_lead: "Same ideas, same names, same way of thinking. The difference: a compiler that checks everything, and native performance.",
    examples: [
        Example {
            label: "Hello World",
            express: HELLO_EXPRESS,
            vitesse: HELLO_VITESSE,
        },
        Example {
            label: "Routes",
            express: r#"app.get("/users/:id", (req, res) => {
  const id = Number(req.params.id);
  if (Number.isNaN(id)) {
    return res.status(400).send("invalid id");
  }
  res.json({ id, name: "Ada" });
});"#,
            vitesse: r#"app.get("/users/:id", |req: Request| async move {
    // automatic 400 if it is not a number
    let id: u64 = req.param_as("id")?;
    Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
});"#,
        },
        Example {
            label: "Middleware",
            express: r#"app.use((req, res, next) => {
  if (req.get("authorization") !== "Bearer secret") {
    return res.status(401).json({ error: "please log in" });
  }
  next();
});"#,
            vitesse: r#"app.middleware(|req: Request, next: Next| async move {
    if req.header("authorization") != Some("Bearer secret") {
        return Error::unauthorized("please log in").into_response();
    }
    next.run(req).await
});"#,
        },
        Example {
            label: "JSON",
            express: JSON_EXPRESS,
            vitesse: JSON_VITESSE,
        },
    ],
    start_label: "Get started",
    start_title: "Up and running in thirty seconds.",
    start_steps: ["Add Vitesse", "Write your app", "Run it"],
    start_listening: "⚡ Vitesse listening on http://localhost:3000",
    start_docs: "Read the docs",
    start_heroku: "Deploy to Heroku",
    cta_title: ["Ready to go", "faster?"],
    cta_start: "Start a project",
    cta_star: "Star on GitHub",
    footer_eyebrow: "The Express-style web framework for Rust",
    footer_lead: "Write code like you do with Express. Serve it at bare-metal speed.",
    footer_cta: "Get started now",
    footer_cols: ["Project", "Documentation", "Community"],
    footer_project: ["Source code", "Benchmark", "Examples", "This site (Yew)"],
    footer_docs: [
        "Guide",
        "Coming from Express",
        "Why it's fast",
        "Deploy to Heroku",
    ],
    footer_community: ["Report a bug", "Issues", "Pull requests", "History"],
    footer_copyright: "© 2026 Vitesse. Made in Rust with Yew and WebAssembly.",
    clock: "Local time",
    totop: "BACK TO TOP • BACK TO TOP • ",
    totop_aria: "Back to top",
    totop_cursor: "Top",
    docs_title: "Documentation",
    docs_lead: "Everything you need to build, test and deploy a Vitesse app, step by step.",
    docs_search: "Search the docs…",
    docs_search_hint: "Press / to search",
    docs_no_results: "No results for “{}”.",
    docs_toc: "On this page",
    docs_prev: "Previous",
    docs_next: "Next",
    docs_edit: "Improve this page on GitHub",
    docs_loading: "Loading…",
    docs_missing: "This page is not available in this language yet.",
    docs_read_en: "Read the English version",
    docs_menu: "Contents",
    docs_api: "Full API reference (docs.rs)",
    docs_quick: "Start here",
    docs_pages: "{} pages",
    callouts: ["Note", "Tip", "Important", "Warning", "Caution"],
};

// ----- Español -------------------------------------------------------------------

pub const ES: Texts = Texts {
    meta_title: "Vitesse — el framework web de Rust al estilo Express",
    docs_meta: "Documentación de Vitesse",
    nav_links: ["Rendimiento", "Funciones", "Desde Express", "Empezar"],
    nav_docs: "Docs",
    nav_aria: "Navegación principal",
    brand_aria: "Vitesse, volver arriba",
    menu: "Menú",
    lang_aria: "Idioma",
    hero_badge: "Novedad · 2,78 millones de peticiones/s en 2 núcleos",
    hero_title: ["La comodidad de Express.", "La velocidad de", "Rust."],
    hero_sub: ["Un framework web ", " para Rust."],
    hero_words: [
        "nativo",
        "minimalista",
        "ultrarrápido",
        "seguro",
        "familiar",
    ],
    hero_lead: [
        "Vitesse retoma la API que ya conoces — ",
        " — y la impulsa con su propio motor HTTP/1.1. Más rápido que actix-web, axum y Drogon.",
    ],
    hero_start: "Empezar",
    hero_perf: "Ver el rendimiento",
    copy: "Copiar",
    copied: "¡Copiado!",
    copy_aria: "Copiar el comando",
    scroll: "Desplázate",
    scroll_aria: "Desplázate hacia abajo",
    marquee_aria: "Puntos fuertes",
    marquee: [
        "La API de Express",
        "2,78 M pet/s",
        "Motor HTTP/1.1 propio",
        "Cero regex",
        "Un hilo por núcleo",
        "100 % Rust",
        "WebSocket · HTTP/3",
        "Pruebas sin red",
    ],
    stats_label: "En cifras",
    stats_title: "Cifras que hablan por sí solas.",
    stats: [
        item(
            "peticiones por segundo",
            "En dos núcleos, en pipeline: la prueba «plaintext» de TechEmpower.",
        ),
        item(
            "más rápido que actix-web",
            "Con una petición real de navegador, y ×2,2 en pipeline.",
        ),
        item(
            "de CPU por petición",
            "Frente a 7,1 µs de actix-web y 11,7 µs de axum.",
        ),
        item(
            "más rápido que Express",
            "De 49 a 56 veces según el escenario, en el mismo hardware.",
        ),
    ],
    bench_label: "Rendimiento",
    bench_title: "Más rápido que actix\u{2011}web. Mucho más rápido que el resto.",
    bench_lead: "Mismas rutas, misma máquina, misma sesión: Express, Drogon (C++), axum, actix-web y Vitesse frente al mismo generador de carga.",
    scenarios: [
        item(
            "Navegador",
            "GET /json enviado con las 12 cabeceras de un navegador real",
        ),
        item("JSON", "GET /json: serialización de un objeto pequeño"),
        item(
            "Parámetros",
            "GET /users/:id: enrutamiento, parámetro y JSON",
        ),
        item("POST JSON", "POST /echo: lee un cuerpo JSON y lo devuelve"),
        item(
            "Pipeline ×16",
            "GET /: dieciséis peticiones enviadas de golpe (TechEmpower)",
        ),
        item("Hello World", "GET /: la respuesta más simple posible"),
    ],
    bench_vs: "frente a",
    bench_note: "VM de 4 vCPU: servidor en 2 núcleos, wrk en los otros 2, 128 conexiones keep-alive, 10 s por escenario. A la derecha, el tiempo de CPU que el servidor dedica a cada petición. ",
    bench_reproduce: "Reproducir el benchmark",
    features_label: "Funcionalidades",
    features_title: "Todo lo que hace Express. En Rust, sin concesiones.",
    features_lead: "Un núcleo deliberadamente pequeño, como Express, y todo lo necesario para una aplicación real.",
    features: [
        item(
            "Motor HTTP/1.1 propio",
            "Cabeceras analizadas con SIMD y leídas sin copias, un solo read y un solo write por petición.",
        ),
        item(
            "Un hilo por núcleo",
            "Cada núcleo tiene su propio bucle de eventos y su socket SO_REUSEPORT. Sin sincronización.",
        ),
        item(
            "Router sin regex",
            "Un árbol de segmentos: parámetros :id, comodines *ruta, las rutas estáticas siempre primero.",
        ),
        item(
            "Middlewares al estilo Express",
            "next.run(req).await, y logger, cors, helmet y timeout listos para usar.",
        ),
        item(
            "Archivos estáticos",
            "Tipos MIME, ETag y 304, peticiones parciales para vídeo, protección contra ../.",
        ),
        item(
            "Errores sin sorpresas",
            "El ? en todas partes, errores HTTP tipados y los pánicos convertidos en 500.",
        ),
        item(
            "Pruebas sin red",
            "Un TestClient que llama a tu aplicación en memoria, en una línea.",
        ),
        item(
            "Apagado limpio",
            "Ctrl+C o SIGTERM: las peticiones en curso terminan y luego el servidor se detiene.",
        ),
    ],
    journey_label: "Bajo el capó",
    journey_title: "El viaje de una petición, en seis microsegundos.",
    journey: [
        step(
            "Lectura",
            "Una sola llamada al sistema lee todo lo que ha llegado: una petición, o dieciséis a la vez.",
            "1 read",
        ),
        step(
            "Análisis",
            "httparse divide la cabecera con SIMD. Las cabeceras son solo posiciones en el búfer.",
            "0 copias",
        ),
        step(
            "Enrutamiento",
            "El árbol de segmentos encuentra la ruta sin regex y escribe los parámetros en un búfer reciclado.",
            "0 asignaciones",
        ),
        step(
            "Handler",
            "Si responde de inmediato, todo sigue siendo síncrono: sin Future intermedio, sin copias.",
            "8 bytes movidos",
        ),
        step(
            "Escritura",
            "Estado y cabeceras precalculados, Date en caché: todas las respuestas del lote salen juntas.",
            "1 write",
        ),
    ],
    journey_end: "de CPU por petición, del primer byte leído al último byte escrito.",
    journey_cta: "Ver el código",
    compare_label: "De Express a Vitesse",
    compare_title: "Ya sabes escribir Vitesse.",
    compare_lead: "Las mismas ideas, los mismos nombres, la misma forma de pensar. La diferencia: un compilador que lo comprueba todo y un rendimiento nativo.",
    examples: [
        Example {
            label: "Hello World",
            express: HELLO_EXPRESS,
            vitesse: HELLO_VITESSE,
        },
        Example {
            label: "Rutas",
            express: r#"app.get("/users/:id", (req, res) => {
  const id = Number(req.params.id);
  if (Number.isNaN(id)) {
    return res.status(400).send("id no válido");
  }
  res.json({ id, name: "Ada" });
});"#,
            vitesse: r#"app.get("/users/:id", |req: Request| async move {
    // 400 automático si no es un número
    let id: u64 = req.param_as("id")?;
    Ok::<_, Error>(Json(json!({ "id": id, "name": "Ada" })))
});"#,
        },
        Example {
            label: "Middleware",
            express: r#"app.use((req, res, next) => {
  if (req.get("authorization") !== "Bearer secret") {
    return res.status(401).json({ error: "inicia sesión" });
  }
  next();
});"#,
            vitesse: r#"app.middleware(|req: Request, next: Next| async move {
    if req.header("authorization") != Some("Bearer secret") {
        return Error::unauthorized("inicia sesión").into_response();
    }
    next.run(req).await
});"#,
        },
        Example {
            label: "JSON",
            express: JSON_EXPRESS,
            vitesse: JSON_VITESSE,
        },
    ],
    start_label: "Empezar",
    start_title: "En marcha en treinta segundos.",
    start_steps: ["Añade Vitesse", "Escribe tu aplicación", "Ejecútala"],
    start_listening: "⚡ Vitesse escuchando en http://localhost:3000",
    start_docs: "Leer la documentación",
    start_heroku: "Desplegar en Heroku",
    cta_title: ["¿Listo para ir", "más rápido?"],
    cta_start: "Empezar un proyecto",
    cta_star: "Dar una estrella en GitHub",
    footer_eyebrow: "El framework web de Rust al estilo Express",
    footer_lead: "Escribe código como con Express. Sírvelo a la velocidad del metal.",
    footer_cta: "Empezar ahora",
    footer_cols: ["Proyecto", "Documentación", "Comunidad"],
    footer_project: ["Código fuente", "Benchmark", "Ejemplos", "Este sitio (Yew)"],
    footer_docs: [
        "Guía",
        "Viniendo de Express",
        "Por qué es rápido",
        "Desplegar en Heroku",
    ],
    footer_community: [
        "Informar de un error",
        "Issues",
        "Pull requests",
        "Historial",
    ],
    footer_copyright: "© 2026 Vitesse. Hecho en Rust con Yew y WebAssembly.",
    clock: "Hora local",
    totop: "VOLVER ARRIBA • VOLVER ARRIBA • ",
    totop_aria: "Volver arriba",
    totop_cursor: "Arriba",
    docs_title: "Documentación",
    docs_lead: "Todo lo que necesitas para crear, probar y desplegar una aplicación Vitesse, paso a paso.",
    docs_search: "Buscar en la documentación…",
    docs_search_hint: "Pulsa / para buscar",
    docs_no_results: "Sin resultados para «{}».",
    docs_toc: "En esta página",
    docs_prev: "Anterior",
    docs_next: "Siguiente",
    docs_edit: "Mejorar esta página en GitHub",
    docs_loading: "Cargando…",
    docs_missing: "Esta página aún no está disponible en este idioma.",
    docs_read_en: "Leer la versión en inglés",
    docs_menu: "Índice",
    docs_api: "Referencia completa de la API (docs.rs)",
    docs_quick: "Para empezar",
    docs_pages: "{} páginas",
    callouts: ["Nota", "Consejo", "Importante", "Advertencia", "Precaución"],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_per_language() {
        assert_eq!(number(Lang::Fr, 2_781_541.0, 0), "2\u{202f}781\u{202f}541");
        assert_eq!(number(Lang::En, 2_781_541.0, 0), "2,781,541");
        assert_eq!(number(Lang::Es, 2_781_541.0, 0), "2.781.541");
        assert_eq!(number(Lang::Fr, 6.14, 1), "6,1");
        assert_eq!(number(Lang::En, 6.14, 1), "6.1");
        assert_eq!(number(Lang::Es, 0.64, 2), "0,64");
    }

    #[test]
    fn detects_language_codes() {
        assert_eq!(Lang::from_code("fr-CA"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("ES"), Some(Lang::Es));
        assert_eq!(Lang::from_code("en"), Some(Lang::En));
        assert_eq!(Lang::from_code("de-DE"), None);
        assert_eq!(Lang::from_code("e"), None);
    }
}
