//! Les textes des îlots : ils sont rendus par le serveur et repris par le code du
//! navigateur (WebAssembly), qui n'embarque ainsi que ce dont il a besoin.

use crate::i18n::{Item, Lang, item};

/// Un exemple « Express à gauche, Vitesse à droite ».
pub struct Example {
    pub label: &'static str,
    pub express: &'static str,
    pub vitesse: &'static str,
}

pub struct Labels {
    pub copy: &'static str,
    pub copied: &'static str,
    pub copy_aria: &'static str,
    // Benchmark
    pub scenarios: [Item; 6],
    pub bench_vs: &'static str,
    pub bench_note: &'static str,
    pub bench_reproduce: &'static str,
    // D'Express à Vitesse
    pub examples: [Example; 4],
    // Recherche dans la documentation
    pub search: &'static str,
    pub search_hint: &'static str,
    /// `{}` est remplacé par la recherche.
    pub no_results: &'static str,
    // Pied de page
    pub clock: &'static str,
}

pub fn labels(lang: Lang) -> &'static Labels {
    lang.pick([&FR, &EN, &ES])
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

// ----- Français -------------------------------------------------------------

const FR: Labels = Labels {
    copy: "Copier",
    copied: "Copié !",
    copy_aria: "Copier la commande",
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
    clock: "Heure locale",
    search: "Rechercher dans la documentation…",
    search_hint: "Appuyez sur / pour chercher",
    no_results: "Aucun résultat pour « {} ».",
};

// ----- English --------------------------------------------------------------

const EN: Labels = Labels {
    copy: "Copy",
    copied: "Copied!",
    copy_aria: "Copy the command",
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
    clock: "Local time",
    search: "Search the docs…",
    search_hint: "Press / to search",
    no_results: "No results for “{}”.",
};

// ----- Español --------------------------------------------------------------

const ES: Labels = Labels {
    copy: "Copiar",
    copied: "¡Copiado!",
    copy_aria: "Copiar el comando",
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
    clock: "Hora local",
    search: "Buscar en la documentación…",
    search_hint: "Pulsa / para buscar",
    no_results: "Sin resultados para «{}».",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_is_translated() {
        for lang in Lang::ALL {
            let l = labels(lang);
            assert!(l.no_results.contains("{}"), "{lang:?}");
            assert!(
                l.examples
                    .iter()
                    .all(|e| !e.express.is_empty() && !e.vitesse.is_empty())
            );
        }
        assert_ne!(FR.search, EN.search);
        assert_ne!(ES.search, EN.search);
        // Les commentaires des exemples sont traduits.
        assert_ne!(FR.examples[1].vitesse, EN.examples[1].vitesse);
    }
}
