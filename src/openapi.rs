//! OpenAPI 3.1 documents generated from the routes (FastAPI's `/docs`,
//! `@fastify/swagger`), with an optional Swagger UI page.
//!
//! Every route of the application is listed (mounted routers included), with
//! its path parameters taken from its pattern (`/users/:id(\d+)` becomes
//! `/users/{id}`, with the constraint as the parameter's `pattern`). Describe
//! a route with [`.doc(Operation)`](crate::App::doc), right after it, like
//! [`.name(…)`](crate::App::name); schemas are plain JSON Schema, written
//! with [`json!`](crate::json).
//!
//! ```
//! use vitesse::prelude::*;
//! use vitesse::openapi::{OpenApi, Operation};
//!
//! let mut app = App::new();
//! app.get("/users/:id(\\d+)", |req: Request| async move {
//!     Json(json!({ "id": req.param("id") }))
//! })
//! .doc(
//!     Operation::new()
//!         .summary("Get a user")
//!         .tag("users")
//!         .response_json(200, "The user", json!({ "$ref": "#/components/schemas/User" }))
//!         .response(404, "No such user"),
//! );
//! app.post("/users", |_| async { StatusCode::CREATED })
//!     .doc(Operation::new().summary("Create a user").tag("users").body(json!({
//!         "type": "object",
//!         "required": ["name"],
//!         "properties": { "name": { "type": "string" } }
//!     })));
//!
//! // GET /openapi.json: the document; GET /docs: Swagger UI.
//! app.openapi(
//!     "/openapi.json",
//!     OpenApi::new("Users API", "1.0.0")
//!         .description("Everything about users")
//!         .schema("User", json!({
//!             "type": "object",
//!             "properties": { "id": { "type": "integer" }, "name": { "type": "string" } }
//!         }))
//!         .swagger_ui("/docs"),
//! );
//! ```
//!
//! The document is built once, when the application starts, and served
//! from memory: it costs nothing per request. Routes for every method
//! (`app.all`), `HEAD` and `OPTIONS` routes, and wildcard routes (static
//! files) are left out unless they have a `.doc(…)`;
//! [`Operation::hidden`] leaves out a route.

use std::collections::BTreeMap;

use http::Method;
use serde_json::{Map, Value, json};

use crate::tree::{Part, Segment, parse_pattern, variants};

/// The description of the whole API: title, version, servers, shared
/// schemas… Served with [`App::openapi`](crate::App::openapi).
#[derive(Debug, Clone)]
pub struct OpenApi {
    info: Map<String, Value>,
    servers: Vec<Value>,
    tags: Vec<Value>,
    schemas: Map<String, Value>,
    security_schemes: Map<String, Value>,
    security: Vec<Value>,
    extra: Option<Value>,
    pub(crate) swagger_ui: Option<String>,
}

impl OpenApi {
    /// An API named `title`, at version `version` (of your API, not of
    /// OpenAPI).
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        let mut info = Map::new();
        info.insert("title".into(), Value::String(title.into()));
        info.insert("version".into(), Value::String(version.into()));
        OpenApi {
            info,
            servers: Vec::new(),
            tags: Vec::new(),
            schemas: Map::new(),
            security_schemes: Map::new(),
            security: Vec::new(),
            extra: None,
            swagger_ui: None,
        }
    }

    /// A longer description of the API (Markdown).
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.info
            .insert("description".into(), Value::String(description.into()));
        self
    }

    /// A server where the API is available (`https://api.example.com`).
    pub fn server(mut self, url: impl Into<String>) -> Self {
        self.servers.push(json!({ "url": url.into() }));
        self
    }

    /// Describes a tag used by the operations.
    pub fn tag(mut self, name: impl Into<String>, description: impl Into<String>) -> Self {
        self.tags
            .push(json!({ "name": name.into(), "description": description.into() }));
        self
    }

    /// A shared schema, referenced with
    /// `json!({ "$ref": "#/components/schemas/<name>" })`.
    pub fn schema(mut self, name: impl Into<String>, schema: Value) -> Self {
        self.schemas.insert(name.into(), schema);
        self
    }

    /// A security scheme, such as
    /// `json!({ "type": "http", "scheme": "bearer" })`, used by
    /// [`Operation::security`] or, for every operation, by
    /// [`OpenApi::security`].
    pub fn security_scheme(mut self, name: impl Into<String>, scheme: Value) -> Self {
        self.security_schemes.insert(name.into(), scheme);
        self
    }

    /// Every operation requires this security scheme (unless it says
    /// otherwise).
    pub fn security(mut self, scheme: impl Into<String>) -> Self {
        self.security.push(requirement(scheme.into()));
        self
    }

    /// Merges raw JSON into the document (any field this builder doesn't
    /// cover: `externalDocs`, `info.contact`, `webhooks`…). Objects are
    /// merged recursively, other values replace.
    pub fn extend(mut self, value: Value) -> Self {
        match &mut self.extra {
            Some(extra) => merge(extra, value),
            None => self.extra = Some(value),
        }
        self
    }

    /// Also serves a Swagger UI page at `path`, to read and try the API in
    /// a browser. The page loads Swagger UI from the jsDelivr CDN.
    pub fn swagger_ui(mut self, path: impl Into<String>) -> Self {
        self.swagger_ui = Some(path.into());
        self
    }
}

/// The description of one route: summary, parameters, body, responses…
/// Attached with [`.doc(…)`](crate::App::doc).
#[derive(Debug, Clone, Default)]
pub struct Operation {
    fields: Map<String, Value>,
    tags: Vec<Value>,
    parameters: Vec<Value>,
    responses: BTreeMap<String, Value>,
    hidden: bool,
}

impl Operation {
    /// An empty description.
    pub fn new() -> Self {
        Operation::default()
    }

    /// A short summary.
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.fields
            .insert("summary".into(), Value::String(summary.into()));
        self
    }

    /// A longer description (Markdown).
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.fields
            .insert("description".into(), Value::String(description.into()));
        self
    }

    /// A unique identifier, used by client generators as the function name.
    pub fn operation_id(mut self, id: impl Into<String>) -> Self {
        self.fields
            .insert("operationId".into(), Value::String(id.into()));
        self
    }

    /// Groups the operation under a tag (several are allowed).
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(Value::String(tag.into()));
        self
    }

    /// Marks the operation as deprecated.
    pub fn deprecated(mut self) -> Self {
        self.fields.insert("deprecated".into(), Value::Bool(true));
        self
    }

    /// Leaves the route out of the document.
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// Describes a path parameter (they are listed anyway, as strings, from
    /// the pattern): its description and schema
    /// (`json!({ "type": "integer" })`).
    pub fn path_param(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
    ) -> Self {
        self.parameter("path", name.into(), description.into(), schema, true)
    }

    /// An optional query string parameter.
    pub fn query(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
    ) -> Self {
        self.parameter("query", name.into(), description.into(), schema, false)
    }

    /// A required query string parameter.
    pub fn query_required(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
    ) -> Self {
        self.parameter("query", name.into(), description.into(), schema, true)
    }

    /// A request header.
    pub fn header(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
        required: bool,
    ) -> Self {
        self.parameter("header", name.into(), description.into(), schema, required)
    }

    fn parameter(
        mut self,
        location: &str,
        name: String,
        description: String,
        schema: Value,
        required: bool,
    ) -> Self {
        self.parameters
            .retain(|p| !(p["in"] == location && p["name"] == name));
        let mut parameter =
            json!({ "name": name, "in": location, "required": required, "schema": schema });
        if !description.is_empty() {
            parameter["description"] = Value::String(description);
        }
        self.parameters.push(parameter);
        self
    }

    /// A required JSON request body, described by `schema`.
    pub fn body(self, schema: Value) -> Self {
        self.body_with("application/json", schema)
    }

    /// A required request body of another media type
    /// (`application/x-www-form-urlencoded`, `multipart/form-data`…).
    pub fn body_with(mut self, content_type: &str, schema: Value) -> Self {
        let body = self
            .fields
            .entry("requestBody")
            .or_insert_with(|| json!({ "required": true, "content": {} }));
        body["content"][content_type] = json!({ "schema": schema });
        self
    }

    /// A possible response, without a body.
    pub fn response(mut self, status: u16, description: impl Into<String>) -> Self {
        self.responses.insert(
            status.to_string(),
            json!({ "description": description.into() }),
        );
        self
    }

    /// A possible response with a JSON body described by `schema`.
    pub fn response_json(
        mut self,
        status: u16,
        description: impl Into<String>,
        schema: Value,
    ) -> Self {
        self.responses.insert(
            status.to_string(),
            json!({
                "description": description.into(),
                "content": { "application/json": { "schema": schema } }
            }),
        );
        self
    }

    /// The operation requires this security scheme (declared with
    /// [`OpenApi::security_scheme`]).
    pub fn security(mut self, scheme: impl Into<String>) -> Self {
        let security = self
            .fields
            .entry("security")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(list) = security {
            list.push(requirement(scheme.into()));
        }
        self
    }

    /// The operation needs no authentication, even if the API does
    /// ([`OpenApi::security`]).
    pub fn public(mut self) -> Self {
        self.fields
            .insert("security".into(), Value::Array(Vec::new()));
        self
    }

    /// Merges raw JSON into the operation (`externalDocs`, `callbacks`…).
    pub fn extend(mut self, value: Value) -> Self {
        let mut fields = Value::Object(std::mem::take(&mut self.fields));
        merge(&mut fields, value);
        if let Value::Object(fields) = fields {
            self.fields = fields;
        }
        self
    }
}

/// A route, as the document sees it.
pub(crate) struct ApiRoute {
    pub(crate) method: Option<Method>,
    pub(crate) pattern: String,
    pub(crate) doc: Option<Operation>,
}

/// Builds the document from the routes.
pub(crate) fn document(spec: &OpenApi, routes: &[ApiRoute]) -> Value {
    let mut paths: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for route in routes {
        if route.doc.as_ref().is_some_and(|d| d.hidden) {
            continue;
        }
        let methods: Vec<&str> = match &route.method {
            Some(m) if route.doc.is_some() => vec![m.as_str()],
            Some(m) if *m == Method::HEAD || *m == Method::OPTIONS => continue,
            Some(m) => vec![m.as_str()],
            None if route.doc.is_some() => vec!["GET", "POST", "PUT", "PATCH", "DELETE"],
            None => continue,
        };
        let Ok(segments) = parse_pattern(&route.pattern) else {
            continue;
        };
        if route.doc.is_none() && matches!(segments.last(), Some(Segment::Wildcard(_))) {
            continue;
        }
        for variant in variants(&segments) {
            let (path, params) = openapi_path(&variant);
            let operation = operation(route.doc.as_ref(), params);
            let item = paths.entry(path).or_default();
            for method in &methods {
                let key = method.to_ascii_lowercase();
                if !item.contains_key(&key) {
                    item.insert(key, operation.clone());
                }
            }
        }
    }
    let mut doc = json!({
        "openapi": "3.1.0",
        "info": spec.info,
        "paths": paths,
    });
    if !spec.servers.is_empty() {
        doc["servers"] = spec.servers.clone().into();
    }
    if !spec.tags.is_empty() {
        doc["tags"] = spec.tags.clone().into();
    }
    if !spec.security.is_empty() {
        doc["security"] = spec.security.clone().into();
    }
    if !spec.schemas.is_empty() {
        doc["components"]["schemas"] = spec.schemas.clone().into();
    }
    if !spec.security_schemes.is_empty() {
        doc["components"]["securitySchemes"] = spec.security_schemes.clone().into();
    }
    if let Some(extra) = &spec.extra {
        merge(&mut doc, extra.clone());
    }
    doc
}

/// `/users/:id(\d+)/files/*path` → `/users/{id}/files/{path}`, and the path
/// parameters.
fn openapi_path(segments: &[Segment<'_>]) -> (String, Vec<Value>) {
    let mut path = String::new();
    let mut params = Vec::new();
    let mut param = |path: &mut String, name: &str, constraint: Option<&str>| {
        let name = if name == "*" { "wildcard" } else { name };
        path.push('{');
        path.push_str(name);
        path.push('}');
        let mut schema = json!({ "type": "string" });
        if let Some(c) = constraint {
            schema["pattern"] = Value::String(format!("^(?:{c})$"));
        }
        params.push(json!({ "name": name, "in": "path", "required": true, "schema": schema }));
    };
    for segment in segments {
        path.push('/');
        match segment {
            Segment::Static(s) => path.push_str(s),
            Segment::Param {
                name, constraint, ..
            } => param(&mut path, name, *constraint),
            Segment::Wildcard(name) => param(&mut path, name, None),
            Segment::Pattern(parts) => {
                for part in parts {
                    match part {
                        Part::Lit(lit) => path.push_str(lit),
                        Part::Param { name, constraint } => param(&mut path, name, *constraint),
                    }
                }
            }
        }
    }
    if path.is_empty() {
        path.push('/');
    }
    (path, params)
}

/// The operation object of a route: its description, with the parameters
/// of its path.
fn operation(doc: Option<&Operation>, path_params: Vec<Value>) -> Value {
    let mut op = Map::new();
    let mut parameters = path_params;
    let mut responses = Map::new();
    if let Some(doc) = doc {
        op = doc.fields.clone();
        if !doc.tags.is_empty() {
            op.insert("tags".into(), doc.tags.clone().into());
        }
        for p in &doc.parameters {
            match parameters
                .iter_mut()
                .find(|q| q["in"] == p["in"] && q["name"] == p["name"])
            {
                Some(q) => *q = p.clone(),
                None if p["in"] == "path" => {}
                None => parameters.push(p.clone()),
            }
        }
        for (status, response) in &doc.responses {
            responses.insert(status.clone(), response.clone());
        }
    }
    if !parameters.is_empty() {
        op.insert("parameters".into(), parameters.into());
    }
    if responses.is_empty() {
        responses.insert("200".into(), json!({ "description": "OK" }));
    }
    op.insert("responses".into(), responses.into());
    Value::Object(op)
}

/// A security requirement: `{ "<scheme>": [] }`.
fn requirement(scheme: String) -> Value {
    let mut map = Map::new();
    map.insert(scheme, Value::Array(Vec::new()));
    Value::Object(map)
}

/// Merges `extra` into `target`: objects field by field, recursively.
fn merge(target: &mut Value, extra: Value) {
    match (target, extra) {
        (Value::Object(target), Value::Object(extra)) => {
            for (key, value) in extra {
                match target.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, extra) => *target = extra,
    }
}

/// The Swagger UI page for the document at `spec_url`.
pub(crate) fn swagger_page(title: &str, spec_url: &str) -> String {
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    // The URL ends up in a JavaScript string: JSON escapes it.
    let url = serde_json::to_string(spec_url)
        .unwrap_or_default()
        .replace('<', "\\u003c");
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14/swagger-ui.css">
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14/swagger-ui-bundle.js" crossorigin></script>
<script>
window.onload = () => {{
  window.ui = SwaggerUIBundle({{ url: {url}, dom_id: "#swagger-ui" }});
}};
</script>
</body>
</html>
"##,
        title = escape(title),
    )
}

impl OpenApi {
    pub(crate) fn title(&self) -> &str {
        self.info
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("API")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(method: Option<Method>, pattern: &str, doc: Option<Operation>) -> ApiRoute {
        ApiRoute {
            method,
            pattern: pattern.into(),
            doc,
        }
    }

    #[test]
    fn paths_and_parameters() {
        let routes = [
            route(Some(Method::GET), "/", None),
            route(Some(Method::GET), "/users/:id(\\d+)", None),
            route(Some(Method::GET), "/flights/:from-:to", None),
            route(Some(Method::GET), "/archive/:year?", None),
            route(Some(Method::HEAD), "/head", None),
            route(None, "/any", None),
            route(Some(Method::GET), "/static/*", None),
            route(
                Some(Method::GET),
                "/hidden",
                Some(Operation::new().hidden()),
            ),
            route(
                Some(Method::PUT),
                "/users/:id",
                Some(
                    Operation::new()
                        .summary("Replace")
                        .path_param("id", "The id", json!({ "type": "integer" }))
                        .query("dry", "", json!({ "type": "boolean" }))
                        .body(json!({ "type": "object" }))
                        .response(204, "Done"),
                ),
            ),
        ];
        let doc = document(&OpenApi::new("T", "1"), &routes);
        let paths = doc["paths"].as_object().unwrap();
        let keys: Vec<&str> = paths.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "/",
                "/archive",
                "/archive/{year}",
                "/flights/{from}-{to}",
                "/users/{id}"
            ]
        );
        let get = &doc["paths"]["/users/{id}"]["get"];
        assert_eq!(get["parameters"][0]["schema"]["pattern"], "^(?:\\d+)$");
        assert_eq!(get["responses"]["200"]["description"], "OK");
        let put = &doc["paths"]["/users/{id}"]["put"];
        assert_eq!(put["summary"], "Replace");
        assert_eq!(put["parameters"][0]["schema"]["type"], "integer");
        assert_eq!(put["parameters"][1]["in"], "query");
        assert_eq!(put["requestBody"]["required"], true);
        assert!(put["responses"]["200"].is_null());
        assert_eq!(
            doc["paths"]["/flights/{from}-{to}"]["get"]["parameters"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn top_level_fields() {
        let spec = OpenApi::new("Pets", "2.0")
            .description("All the pets")
            .server("https://api.example.com")
            .tag("pets", "Pets")
            .schema("Pet", json!({ "type": "object" }))
            .security_scheme("bearer", json!({ "type": "http", "scheme": "bearer" }))
            .security("bearer")
            .extend(json!({ "info": { "contact": { "name": "Ada" } } }));
        let doc = document(&spec, &[]);
        assert_eq!(doc["openapi"], "3.1.0");
        assert_eq!(doc["info"]["title"], "Pets");
        assert_eq!(doc["info"]["contact"]["name"], "Ada");
        assert_eq!(doc["servers"][0]["url"], "https://api.example.com");
        assert_eq!(doc["components"]["schemas"]["Pet"]["type"], "object");
        assert_eq!(doc["security"][0]["bearer"], json!([]));
    }

    #[test]
    fn swagger_page_escapes() {
        let page = swagger_page("<API>", "/spec</script>.json");
        assert!(page.contains("&lt;API&gt;"));
        assert!(!page.contains("</script>.json"));
    }
}
