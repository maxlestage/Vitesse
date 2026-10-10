//! Le sommaire de la documentation : catégories et pages, dans l'ordre de
//! lecture. Les pages elles-mêmes sont les fichiers `docs/<langue>/<slug>.md`
//! du dépôt.

use crate::i18n::Lang;

/// Une catégorie : titres `[fr, en, es]` et icône.
pub struct Category {
    pub title: [&'static str; 3],
    pub icon: &'static str,
}

pub struct Page {
    pub slug: &'static str,
    pub category: usize,
    pub title: [&'static str; 3],
    pub summary: [&'static str; 3],
}

impl Page {
    pub fn title(&self, lang: Lang) -> &'static str {
        lang.pick(self.title)
    }

    pub fn summary(&self, lang: Lang) -> &'static str {
        lang.pick(self.summary)
    }
}

pub const CATEGORIES: [Category; 5] = [
    Category {
        title: ["Premiers pas", "Getting started", "Primeros pasos"],
        icon: "engine",
    },
    Category {
        title: ["L'essentiel", "Essentials", "Lo esencial"],
        icon: "layers",
    },
    Category {
        title: ["Aller plus loin", "Going further", "Profundizando"],
        icon: "cores",
    },
    Category {
        title: ["Déploiement", "Deployment", "Despliegue"],
        icon: "power",
    },
    Category {
        title: ["Référence", "Reference", "Referencia"],
        icon: "files",
    },
];

pub const PAGES: [Page; 20] = [
    Page {
        slug: "introduction",
        category: 0,
        title: ["Introduction", "Introduction", "Introducción"],
        summary: [
            "Ce qu'est Vitesse, pour qui, et comment lire cette documentation.",
            "What Vitesse is, who it is for, and how to read these docs.",
            "Qué es Vitesse, para quién es y cómo leer esta documentación.",
        ],
    },
    Page {
        slug: "installation",
        category: 0,
        title: ["Installation", "Installation", "Instalación"],
        summary: [
            "Installer Rust, créer un projet et ajouter Vitesse.",
            "Install Rust, create a project and add Vitesse.",
            "Instala Rust, crea un proyecto y añade Vitesse.",
        ],
    },
    Page {
        slug: "first-app",
        category: 0,
        title: [
            "Votre première application",
            "Your first app",
            "Tu primera aplicación",
        ],
        summary: [
            "De « Hello World » à une petite API JSON, pas à pas.",
            "From “Hello World” to a small JSON API, step by step.",
            "De «Hello World» a una pequeña API JSON, paso a paso.",
        ],
    },
    Page {
        slug: "routing",
        category: 1,
        title: ["Routage", "Routing", "Enrutamiento"],
        summary: [
            "Méthodes HTTP, paramètres, jokers et priorité des routes.",
            "HTTP methods, parameters, wildcards and route priority.",
            "Métodos HTTP, parámetros, comodines y prioridad de rutas.",
        ],
    },
    Page {
        slug: "requests",
        category: 1,
        title: ["Lire la requête", "Reading requests", "Leer la petición"],
        summary: [
            "Paramètres, query string, en-têtes, cookies et corps.",
            "Parameters, query strings, headers, cookies and bodies.",
            "Parámetros, query string, cabeceras, cookies y cuerpo.",
        ],
    },
    Page {
        slug: "responses",
        category: 1,
        title: ["Répondre", "Sending responses", "Responder"],
        summary: [
            "Texte, JSON, HTML, statuts, en-têtes, cookies et redirections.",
            "Text, JSON, HTML, status codes, headers, cookies and redirects.",
            "Texto, JSON, HTML, códigos de estado, cabeceras, cookies y redirecciones.",
        ],
    },
    Page {
        slug: "middleware",
        category: 1,
        title: ["Middlewares", "Middleware", "Middlewares"],
        summary: [
            "next.run(req), l'ordre d'exécution et les middlewares fournis.",
            "next.run(req), execution order and the built-in middleware.",
            "next.run(req), el orden de ejecución y los middlewares incluidos.",
        ],
    },
    Page {
        slug: "routers",
        category: 1,
        title: ["Routeurs et sous-applications", "Routers", "Routers"],
        summary: [
            "Découper une application avec Router et app.mount.",
            "Split an application with Router and app.mount.",
            "Divide una aplicación con Router y app.mount.",
        ],
    },
    Page {
        slug: "state",
        category: 1,
        title: ["État partagé", "Shared state", "Estado compartido"],
        summary: [
            "Partager configuration, compteurs ou base de données.",
            "Share configuration, counters or a database.",
            "Comparte configuración, contadores o una base de datos.",
        ],
    },
    Page {
        slug: "errors",
        category: 1,
        title: ["Gestion des erreurs", "Error handling", "Manejo de errores"],
        summary: [
            "Le ?, les erreurs HTTP, on_error, la page 404 et les paniques.",
            "The ? operator, HTTP errors, on_error, the 404 page and panics.",
            "El operador ?, los errores HTTP, on_error, la página 404 y los pánicos.",
        ],
    },
    Page {
        slug: "static-files",
        category: 1,
        title: ["Fichiers statiques", "Static files", "Archivos estáticos"],
        summary: [
            "Servir un dossier : types MIME, cache, ETag et sécurité.",
            "Serve a folder: MIME types, caching, ETag and security.",
            "Sirve una carpeta: tipos MIME, caché, ETag y seguridad.",
        ],
    },
    Page {
        slug: "testing",
        category: 2,
        title: ["Tests", "Testing", "Pruebas"],
        summary: [
            "Tester sans réseau avec TestClient, ou sur un vrai port.",
            "Test without a network using TestClient, or on a real port.",
            "Prueba sin red con TestClient, o en un puerto real.",
        ],
    },
    Page {
        slug: "server",
        category: 2,
        title: [
            "Configuration du serveur",
            "Server configuration",
            "Configuración del servidor",
        ],
        summary: [
            "Adresses, threads, limites, délais et arrêt propre.",
            "Addresses, threads, limits, timeouts and graceful shutdown.",
            "Direcciones, hilos, límites, tiempos de espera y apagado limpio.",
        ],
    },
    Page {
        slug: "performance",
        category: 2,
        title: ["Performances", "Performance", "Rendimiento"],
        summary: [
            "Le benchmark, pourquoi c'est rapide, et comment le rester.",
            "The benchmark, why it is fast, and how to keep your app fast.",
            "El benchmark, por qué es rápido y cómo mantener tu aplicación rápida.",
        ],
    },
    Page {
        slug: "from-express",
        category: 2,
        title: [
            "Venir d'Express",
            "Coming from Express",
            "Viniendo de Express",
        ],
        summary: [
            "Le guide de migration, concept par concept.",
            "The migration guide, concept by concept.",
            "La guía de migración, concepto a concepto.",
        ],
    },
    Page {
        slug: "heroku-mobile",
        category: 3,
        title: [
            "Déployer sur Heroku depuis un téléphone",
            "Deploy to Heroku from your phone",
            "Desplegar en Heroku desde el móvil",
        ],
        summary: [
            "En ligne sans ordinateur : bouton de déploiement ou GitHub Actions.",
            "Go live without a computer: the Deploy button or GitHub Actions.",
            "En producción sin ordenador: el botón de despliegue o GitHub Actions.",
        ],
    },
    Page {
        slug: "docker",
        category: 3,
        title: ["Docker", "Docker", "Docker"],
        summary: [
            "L'image de production et les plateformes compatibles.",
            "The production image and the platforms that run it.",
            "La imagen de producción y las plataformas compatibles.",
        ],
    },
    Page {
        slug: "production",
        category: 3,
        title: [
            "Mise en production",
            "Going to production",
            "Puesta en producción",
        ],
        summary: [
            "Reverse proxy, TLS, systemd, sécurité et supervision.",
            "Reverse proxy, TLS, systemd, security and monitoring.",
            "Proxy inverso, TLS, systemd, seguridad y monitorización.",
        ],
    },
    Page {
        slug: "cheatsheet",
        category: 4,
        title: [
            "Aide-mémoire de l'API",
            "API cheat sheet",
            "Chuleta de la API",
        ],
        summary: [
            "Toute l'API publique en une page.",
            "The whole public API on one page.",
            "Toda la API pública en una página.",
        ],
    },
    Page {
        slug: "faq",
        category: 4,
        title: [
            "FAQ et limites",
            "FAQ & limitations",
            "Preguntas frecuentes y límites",
        ],
        summary: [
            "Les questions fréquentes et ce que Vitesse ne fait pas (encore).",
            "Common questions and what Vitesse does not do (yet).",
            "Preguntas habituales y lo que Vitesse no hace (todavía).",
        ],
    },
];

pub fn find(slug: &str) -> Option<(usize, &'static Page)> {
    PAGES.iter().enumerate().find(|(_, p)| p.slug == slug)
}
