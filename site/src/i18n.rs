//! Les langues du site (français, anglais, espagnol) : les textes des pages, le
//! format des nombres et le choix de la langue d'après le navigateur.
//!
//! Chaque langue est une constante [`Texts`] : oublier une traduction est une
//! erreur de compilation. Les textes des îlots, qui vont aussi dans le code du
//! navigateur, sont à part dans [`crate::labels`].

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Lang {
    Fr,
    En,
    Es,
}

impl Lang {
    /// Dans l'ordre du sélecteur.
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Fr, Lang::Es];

    pub fn code(self) -> &'static str {
        self.pick(["fr", "en", "es"])
    }

    pub fn name(self) -> &'static str {
        self.pick(["Français", "English", "Español"])
    }

    /// Pour formater les dates et heures (`fr-FR`).
    pub fn locale(self) -> &'static str {
        self.pick(["fr-FR", "en-US", "es-ES"])
    }

    /// Pour Open Graph (`fr_FR`).
    pub fn og_locale(self) -> &'static str {
        self.pick(["fr_FR", "en_US", "es_ES"])
    }

    /// Choisit la valeur de cette langue dans `[fr, en, es]`.
    pub fn pick<T: Copy>(self, values: [T; 3]) -> T {
        values[match self {
            Lang::Fr => 0,
            Lang::En => 1,
            Lang::Es => 2,
        }]
    }

    /// La langue d'un code exact (`fr`), comme dans les adresses.
    pub fn parse(code: &str) -> Option<Lang> {
        match code {
            "fr" => Some(Lang::Fr),
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    /// La langue d'une étiquette de langue (`fr-CA`, `ES`…).
    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::parse(&code.get(..2)?.to_ascii_lowercase())
    }

    pub fn texts(self) -> &'static Texts {
        self.pick([&FR, &EN, &ES])
    }
}

/// La langue préférée parmi celles du site, d'après un en-tête `Accept-Language`
/// (l'anglais par défaut).
pub fn pick_lang(header: &str) -> Lang {
    let mut ranked: Vec<(Lang, f32)> = header
        .split(',')
        .filter_map(|part| {
            let mut pieces = part.trim().split(";q=");
            let lang = Lang::from_code(pieces.next()?.trim())?;
            let q = pieces
                .next()
                .and_then(|q| q.trim().parse().ok())
                .unwrap_or(1.0);
            Some((lang, q))
        })
        .filter(|(_, q)| *q > 0.0)
        .collect();
    // Stable : à qualité égale, l'ordre de l'en-tête.
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.first().map_or(Lang::En, |r| r.0)
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

pub const fn item(title: &'static str, text: &'static str) -> Item {
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
    pub meta_description: &'static str,
    pub docs_meta: &'static str,
    pub skip: &'static str,
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
    pub totop: &'static str,
    pub totop_aria: &'static str,
    pub totop_cursor: &'static str,
    // Documentation
    pub docs_title: &'static str,
    pub docs_lead: &'static str,
    pub docs_toc: &'static str,
    pub docs_prev: &'static str,
    pub docs_next: &'static str,
    pub docs_edit: &'static str,
    pub docs_missing: &'static str,
    pub docs_menu: &'static str,
    pub docs_api: &'static str,
    pub docs_quick: &'static str,
    /// `{}` est remplacé par le nombre de pages.
    pub docs_pages: &'static str,
    /// Note, astuce, important, attention, prudence.
    pub callouts: [&'static str; 5],
    // Page introuvable, hors ligne, choix de la langue
    pub not_found_title: &'static str,
    pub not_found_text: &'static str,
    pub not_found_back: &'static str,
    pub offline_title: &'static str,
    pub offline_text: &'static str,
    pub lang_choose: &'static str,
}

// ----- Français -------------------------------------------------------------

pub const FR: Texts = Texts {
    meta_title: "Vitesse — le framework web Rust à la Express",
    meta_description: "Vitesse apporte l'API d'Express (app.get, req.params, res.json) au Rust natif, avec son propre moteur HTTP/1.1 : plus rapide qu'actix-web, axum et Drogon. Docs en français, anglais et espagnol.",
    docs_meta: "Documentation Vitesse",
    skip: "Aller au contenu",
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
    footer_project: ["Code source", "Benchmark", "Exemples", "Ce site (active)"],
    footer_docs: [
        "Guide",
        "Venir d'Express",
        "Pourquoi c'est rapide",
        "Déployer sur Heroku",
    ],
    footer_community: ["Signaler un bug", "Issues", "Pull requests", "Historique"],
    footer_copyright: "© 2026 Vitesse. Servi par Vitesse, écrit en Rust avec active et WebAssembly.",
    totop: "RETOUR EN HAUT • RETOUR EN HAUT • ",
    totop_aria: "Retour en haut",
    totop_cursor: "Haut",
    docs_title: "Documentation",
    docs_lead: "Tout ce qu'il faut pour construire, tester et déployer une application Vitesse, pas à pas.",
    docs_toc: "Sur cette page",
    docs_prev: "Précédent",
    docs_next: "Suivant",
    docs_edit: "Améliorer cette page sur GitHub",
    docs_missing: "Cette page n'est pas encore disponible dans cette langue : voici la version anglaise.",
    docs_menu: "Sommaire",
    docs_api: "Référence complète de l'API (docs.rs)",
    docs_quick: "Pour bien démarrer",
    docs_pages: "{} pages",
    callouts: ["Remarque", "Astuce", "Important", "Attention", "Prudence"],
    not_found_title: "Page introuvable",
    not_found_text: "Cette page n'existe pas, ou elle a changé d'adresse. L'accueil et la documentation, eux, sont toujours là.",
    not_found_back: "Retour à l'accueil",
    offline_title: "Vous êtes hors ligne",
    offline_text: "Cette page n'a pas encore été enregistrée sur cet appareil. L'accueil, le sommaire de la documentation et les pages déjà visitées restent disponibles.",
    lang_choose: "Choisissez votre langue",
};

// ----- English --------------------------------------------------------------

pub const EN: Texts = Texts {
    meta_title: "Vitesse — the Express-style web framework for Rust",
    meta_description: "Vitesse brings the Express API (app.get, req.params, res.json) to native Rust, with its own HTTP/1.1 engine: faster than actix-web, axum and Drogon. Docs in English, French and Spanish.",
    docs_meta: "Vitesse docs",
    skip: "Skip to content",
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
    footer_project: ["Source code", "Benchmark", "Examples", "This site (active)"],
    footer_docs: [
        "Guide",
        "Coming from Express",
        "Why it's fast",
        "Deploy to Heroku",
    ],
    footer_community: ["Report a bug", "Issues", "Pull requests", "History"],
    footer_copyright: "© 2026 Vitesse. Served by Vitesse, written in Rust with active and WebAssembly.",
    totop: "BACK TO TOP • BACK TO TOP • ",
    totop_aria: "Back to top",
    totop_cursor: "Top",
    docs_title: "Documentation",
    docs_lead: "Everything you need to build, test and deploy a Vitesse app, step by step.",
    docs_toc: "On this page",
    docs_prev: "Previous",
    docs_next: "Next",
    docs_edit: "Improve this page on GitHub",
    docs_missing: "This page is not available in this language yet: here is the English version.",
    docs_menu: "Contents",
    docs_api: "Full API reference (docs.rs)",
    docs_quick: "Start here",
    docs_pages: "{} pages",
    callouts: ["Note", "Tip", "Important", "Warning", "Caution"],
    not_found_title: "Page not found",
    not_found_text: "This page does not exist, or it has moved. The home page and the docs are still here.",
    not_found_back: "Back to the home page",
    offline_title: "You are offline",
    offline_text: "This page has not been saved on this device yet. The home page, the docs contents and the pages you have already visited are still available.",
    lang_choose: "Choose your language",
};

// ----- Español --------------------------------------------------------------

pub const ES: Texts = Texts {
    meta_title: "Vitesse — el framework web de Rust al estilo Express",
    meta_description: "Vitesse lleva la API de Express (app.get, req.params, res.json) a Rust nativo, con su propio motor HTTP/1.1: más rápido que actix-web, axum y Drogon. Documentación en español, inglés y francés.",
    docs_meta: "Documentación de Vitesse",
    skip: "Ir al contenido",
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
    footer_project: [
        "Código fuente",
        "Benchmark",
        "Ejemplos",
        "Este sitio (active)",
    ],
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
    footer_copyright: "© 2026 Vitesse. Servido por Vitesse, escrito en Rust con active y WebAssembly.",
    totop: "VOLVER ARRIBA • VOLVER ARRIBA • ",
    totop_aria: "Volver arriba",
    totop_cursor: "Arriba",
    docs_title: "Documentación",
    docs_lead: "Todo lo que necesitas para crear, probar y desplegar una aplicación Vitesse, paso a paso.",
    docs_toc: "En esta página",
    docs_prev: "Anterior",
    docs_next: "Siguiente",
    docs_edit: "Mejorar esta página en GitHub",
    docs_missing: "Esta página aún no está disponible en este idioma: aquí tienes la versión en inglés.",
    docs_menu: "Índice",
    docs_api: "Referencia completa de la API (docs.rs)",
    docs_quick: "Para empezar",
    docs_pages: "{} páginas",
    callouts: ["Nota", "Consejo", "Importante", "Advertencia", "Precaución"],
    not_found_title: "Página no encontrada",
    not_found_text: "Esta página no existe o ha cambiado de dirección. La portada y la documentación siguen aquí.",
    not_found_back: "Volver a la portada",
    offline_title: "Sin conexión",
    offline_text: "Esta página aún no se ha guardado en este dispositivo. La portada, el índice de la documentación y las páginas que ya visitaste siguen disponibles.",
    lang_choose: "Elige tu idioma",
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
        assert_eq!(Lang::parse("fr"), Some(Lang::Fr));
        assert_eq!(Lang::parse("fr-CA"), None);
    }

    #[test]
    fn picks_the_preferred_language() {
        assert_eq!(pick_lang("fr-FR,fr;q=0.9,en;q=0.8"), Lang::Fr);
        assert_eq!(pick_lang("de-DE,es;q=0.7,en;q=0.5"), Lang::Es);
        assert_eq!(pick_lang("en;q=0.2, fr;q=0.9"), Lang::Fr);
        assert_eq!(pick_lang("de, it"), Lang::En);
        assert_eq!(pick_lang(""), Lang::En);
        assert_eq!(pick_lang("fr;q=0"), Lang::En);
    }

    #[test]
    fn every_text_is_filled_in() {
        for lang in Lang::ALL {
            let t = lang.texts();
            for s in [
                t.meta_title,
                t.meta_description,
                t.skip,
                t.not_found_title,
                t.offline_text,
            ] {
                assert!(!s.trim().is_empty(), "{lang:?}");
            }
            assert!(t.docs_pages.contains("{}"));
            assert!(
                t.meta_description.chars().count() <= 200,
                "{lang:?}: description trop longue"
            );
        }
        assert_ne!(FR.offline_title, EN.offline_title);
        assert_ne!(ES.offline_title, EN.offline_title);
    }
}
