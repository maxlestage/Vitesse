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
    /// Other documentation pages: (kind, path).
    pub(crate) pages: Vec<(DocsUi, String)>,
    /// Where Swagger UI's files are served from (the CDN by default).
    pub(crate) assets: Option<String>,
}

/// The documentation pages [`OpenApi`] can serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocsUi {
    Scalar,
    Redoc,
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
            pages: Vec::new(),
            assets: None,
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

    /// Also serves a [Scalar](https://scalar.com) API reference at `path`
    /// (Hono's and Elysia's favourite): a modern page to read the API and
    /// send requests from the browser. The page loads Scalar from the
    /// jsDelivr CDN.
    ///
    /// ```
    /// use vitesse::openapi::OpenApi;
    ///
    /// let spec = OpenApi::new("Pets", "1.0.0")
    ///     .swagger_ui("/docs")
    ///     .scalar("/reference")
    ///     .redoc("/redoc");
    /// ```
    pub fn scalar(mut self, path: impl Into<String>) -> Self {
        self.pages.push((DocsUi::Scalar, path.into()));
        self
    }

    /// Also serves a [Redoc](https://redocly.com/redoc) page at `path`: a
    /// three-column reference, read-only. The page loads Redoc from the
    /// jsDelivr CDN.
    pub fn redoc(mut self, path: impl Into<String>) -> Self {
        self.pages.push((DocsUi::Redoc, path.into()));
        self
    }

    /// Loads Swagger UI's files (`swagger-ui.css` and
    /// `swagger-ui-bundle.js`) from `base` instead of the CDN: for a
    /// network without Internet access, or a strict
    /// `Content-Security-Policy`. Serve the `dist` folder of the
    /// `swagger-ui-dist` npm package there, with
    /// [`static_dir`](crate::App::static_dir).
    ///
    /// ```
    /// use vitesse::prelude::*;
    /// use vitesse::openapi::OpenApi;
    ///
    /// let mut app = App::new();
    /// app.static_dir("/vendor/swagger-ui", "vendor/swagger-ui-dist");
    /// app.openapi(
    ///     "/openapi.json",
    ///     OpenApi::new("Pets", "1.0.0")
    ///         .swagger_ui("/docs")
    ///         .swagger_ui_assets("/vendor/swagger-ui"),
    /// );
    /// ```
    pub fn swagger_ui_assets(mut self, base: impl Into<String>) -> Self {
        self.assets = Some(base.into().trim_end_matches('/').to_owned());
        self
    }

    /// Adds the schema of `T` to the shared schemas (with the `schemars`
    /// feature), with those of the types it uses: reference it with
    /// `json!({ "$ref": "#/components/schemas/<name of T>" })`.
    #[cfg(feature = "schemars")]
    pub fn schema_type<T: schemars::JsonSchema>(mut self) -> Self {
        let (schema, definitions) = typed::schema_for::<T>(typed::Contract::Serialize);
        if let Some(name) = typed::reference_name(&schema) {
            if !definitions.contains_key(&name) {
                self.schemas.insert(name, schema);
            }
        }
        self.schemas.extend(definitions);
        self
    }

    /// Also serves a Swagger UI page at `path`, to read and try the API in
    /// a browser. The page loads Swagger UI from the jsDelivr CDN (or from
    /// [`swagger_ui_assets`](Self::swagger_ui_assets)).
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
    /// The shared schemas of the types used by the operation.
    schemas: Map<String, Value>,
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

    /// The JSON request body is a `T`, described by its schema (with the
    /// `schemars` feature and `#[derive(JsonSchema)]`, FastAPI-style): the
    /// schemas of `T` and of the types it uses land in
    /// `components/schemas`, the body references them.
    ///
    /// ```
    /// use schemars::JsonSchema;
    /// use serde::{Deserialize, Serialize};
    /// use vitesse::prelude::*;
    /// use vitesse::openapi::{OpenApi, Operation};
    ///
    /// #[derive(Deserialize, JsonSchema)]
    /// struct NewPet {
    ///     /// The pet's name.
    ///     name: String,
    ///     age: Option<u8>,
    /// }
    ///
    /// #[derive(Serialize, JsonSchema)]
    /// struct Pet {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// let mut app = App::new();
    /// app.post("/pets", |_| async { (201, Json(json!({ "id": 1, "name": "Rex" }))) })
    ///     .doc(
    ///         Operation::new()
    ///             .summary("Add a pet")
    ///             .body_type::<NewPet>()
    ///             .response_type::<Pet>(201, "The new pet"),
    ///     );
    /// let doc = app.openapi_document(&OpenApi::new("Pets", "1.0.0"));
    /// assert_eq!(doc["components"]["schemas"]["NewPet"]["required"][0], "name");
    /// ```
    #[cfg(feature = "schemars")]
    pub fn body_type<T: schemars::JsonSchema>(self) -> Self {
        self.body_type_with::<T>("application/json")
    }

    /// The request body is a `T` sent as an HTML form
    /// (`application/x-www-form-urlencoded`), read with
    /// [`req.form()`](crate::Request::form). See
    /// [`body_type`](Self::body_type).
    #[cfg(feature = "schemars")]
    pub fn form_type<T: schemars::JsonSchema>(self) -> Self {
        self.body_type_with::<T>("application/x-www-form-urlencoded")
    }

    #[cfg(feature = "schemars")]
    fn body_type_with<T: schemars::JsonSchema>(mut self, content_type: &str) -> Self {
        let (schema, definitions) = typed::schema_for::<T>(typed::Contract::Deserialize);
        self.schemas.extend(definitions);
        self.body_with(content_type, schema)
    }

    /// Like [`body_type`](Self::body_type) for a type that also implements
    /// [`Validate`](crate::validate::Validate), read with
    /// [`req.valid_json()`](crate::Request::valid_json): adds the `422`
    /// response and the shape of its validation errors.
    #[cfg(feature = "schemars")]
    pub fn validated_body<T>(self) -> Self
    where
        T: schemars::JsonSchema + crate::validate::Validate,
    {
        self.body_type::<T>().response_json(
            422,
            "Validation failed: the messages of each invalid field",
            json!({
                "type": "object",
                "required": ["error", "details"],
                "properties": {
                    "error": { "type": "string" },
                    "details": {
                        "type": "object",
                        "additionalProperties": { "type": "array", "items": { "type": "string" } }
                    }
                }
            }),
        )
    }

    /// A possible response with a JSON body of type `T` (with the
    /// `schemars` feature). See [`body_type`](Self::body_type).
    #[cfg(feature = "schemars")]
    pub fn response_type<T: schemars::JsonSchema>(
        mut self,
        status: u16,
        description: impl Into<String>,
    ) -> Self {
        let (schema, definitions) = typed::schema_for::<T>(typed::Contract::Serialize);
        self.schemas.extend(definitions);
        self.response_json(status, description, schema)
    }

    /// The query string parameters are the fields of `T`, read with
    /// [`req.query_as()`](crate::Request::query_as) (with the `schemars`
    /// feature): one parameter per field, in alphabetical order, required
    /// unless it is an `Option` or has a default, with its schema and doc
    /// comment.
    ///
    /// ```
    /// use schemars::JsonSchema;
    /// use serde::Deserialize;
    /// use vitesse::prelude::*;
    /// use vitesse::openapi::{OpenApi, Operation};
    ///
    /// #[derive(Deserialize, JsonSchema)]
    /// struct Search {
    ///     /// Words to look for.
    ///     q: String,
    ///     size: Option<u32>,
    /// }
    ///
    /// let mut app = App::new();
    /// app.get("/search", |_| async { "…" }).doc(Operation::new().query_type::<Search>());
    /// let doc = app.openapi_document(&OpenApi::new("Search", "1"));
    /// let params = &doc["paths"]["/search"]["get"]["parameters"];
    /// assert_eq!(params[0]["name"], "q");
    /// assert_eq!(params[0]["required"], true);
    /// assert_eq!(params[1]["name"], "size");
    /// assert_eq!(params[1]["required"], false);
    /// ```
    #[cfg(feature = "schemars")]
    pub fn query_type<T: schemars::JsonSchema>(mut self) -> Self {
        let schema = typed::inline_schema_for::<T>();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if let Some(properties) = schema["properties"].as_object() {
            for (name, property) in properties {
                let mut property = property.clone();
                let description = property
                    .as_object_mut()
                    .and_then(|p| p.remove("description"))
                    .and_then(|d| d.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let required = required.contains(&name.as_str());
                self = self.parameter("query", name.clone(), description, property, required);
            }
        }
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
    // The schemas of the types used by the operations, then those of the
    // spec, which win.
    let mut schemas = Map::new();
    for route in routes {
        if let Some(doc) = &route.doc {
            if !doc.hidden {
                schemas.extend(doc.schemas.clone());
            }
        }
    }
    schemas.extend(spec.schemas.clone());
    if !schemas.is_empty() {
        doc["components"]["schemas"] = schemas.into();
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

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A string for a JavaScript literal, safe in a `<script>`.
fn js_string(s: &str) -> String {
    serde_json::to_string(s)
        .unwrap_or_default()
        .replace('<', "\\u003c")
}

/// The Scalar or Redoc page for the document at `spec_url`.
pub(crate) fn docs_page(ui: DocsUi, title: &str, spec_url: &str) -> String {
    let title = escape_html(title);
    let url = escape_html(spec_url);
    match ui {
        DocsUi::Scalar => format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
</head>
<body>
<script id="api-reference" data-url="{url}"></script>
<script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.25.11" crossorigin></script>
</body>
</html>
"#
        ),
        DocsUi::Redoc => format!(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>body {{ margin: 0; }}</style>
</head>
<body>
<redoc spec-url="{url}"></redoc>
<script src="https://cdn.jsdelivr.net/npm/redoc@2.1.5/bundles/redoc.standalone.js" crossorigin></script>
</body>
</html>
"#
        ),
    }
}

/// The Swagger UI page for the document at `spec_url`, with Swagger UI's
/// files from `assets` (the CDN by default).
pub(crate) fn swagger_page(title: &str, spec_url: &str, assets: Option<&str>) -> String {
    let escape = escape_html;
    // The URL ends up in a JavaScript string: JSON escapes it.
    let url = js_string(spec_url);
    let assets = escape(assets.unwrap_or("https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.17.14"));
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="{assets}/swagger-ui.css">
</head>
<body>
<div id="swagger-ui"></div>
<script src="{assets}/swagger-ui-bundle.js" crossorigin></script>
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

/// Schemas generated from Rust types with schemars.
#[cfg(feature = "schemars")]
mod typed {
    use schemars::JsonSchema;
    use schemars::generate::SchemaSettings;
    use serde_json::{Map, Value};

    #[derive(Clone, Copy)]
    pub(super) enum Contract {
        Deserialize,
        Serialize,
    }

    fn settings(contract: Contract) -> SchemaSettings {
        let settings = SchemaSettings::draft2020_12().with(|s| {
            s.definitions_path = "/components/schemas".into();
            s.meta_schema = None;
        });
        match contract {
            Contract::Deserialize => settings.for_deserialize(),
            Contract::Serialize => settings.for_serialize(),
        }
    }

    /// The schema of `T` (a `$ref` for named types) and the shared schemas
    /// it needs.
    pub(super) fn schema_for<T: JsonSchema>(contract: Contract) -> (Value, Map<String, Value>) {
        let mut generator = settings(contract).into_generator();
        let schema = generator.subschema_for::<T>();
        let definitions = generator.take_definitions(true);
        (schema.into(), definitions)
    }

    /// The schema of `T`, with nothing referenced.
    pub(super) fn inline_schema_for<T: JsonSchema>() -> Value {
        settings(Contract::Deserialize)
            .with(|s| s.inline_subschemas = true)
            .into_generator()
            .into_root_schema_for::<T>()
            .into()
    }

    /// `Pet` for `{"$ref": "#/components/schemas/Pet"}`.
    pub(super) fn reference_name(schema: &Value) -> Option<String> {
        schema["$ref"]
            .as_str()?
            .strip_prefix("#/components/schemas/")
            .map(str::to_owned)
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
        let page = swagger_page("<API>", "/spec</script>.json", None);
        assert!(page.contains("&lt;API&gt;"));
        assert!(!page.contains("</script>.json"));
    }
}
