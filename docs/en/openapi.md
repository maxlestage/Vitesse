# OpenAPI and Swagger UI

Vitesse generates the [OpenAPI 3.1](https://spec.openapis.org/oas/v3.1.0) document of your application from its routes, like FastAPI or `@fastify/swagger`, and can serve a Swagger UI page to read and try the API in a browser. Client generators, Postman, API gateways and documentation tools all read that document.

## Serving the document

One call, anywhere in the setup of the application:

```rust
use vitesse::prelude::*;
use vitesse::openapi::OpenApi;

let mut app = App::new();
app.get("/users/:id", show_user);
app.post("/users", create_user);

app.openapi(
    "/openapi.json",
    OpenApi::new("Users API", "1.0.0").swagger_ui("/docs"),
);
app.run(3000)
```

`GET /openapi.json` now answers the document, and `GET /docs` the Swagger UI page (it loads Swagger UI from the jsDelivr CDN). Every route is listed, mounted routers and groups included, with the parameters of its path: `/users/:id` becomes `/users/{id}`, and a constraint such as `r"/users/:id(\d+)"` becomes the `pattern` of the parameter's schema. Optional parameters give one path per variant (`/archive` and `/archive/{year}`).

The document is built once, when the application starts, and served from memory: it costs nothing per request, and nothing at all if you don't call `app.openapi`.

## Describing a route

`.doc(…)` follows a route, like `.name(…)`, and describes it with an `Operation`:

```rust
use vitesse::openapi::Operation;

app.get(r"/users/:id(\d+)", show_user)
    .name("user")
    .doc(
        Operation::new()
            .summary("Get a user")
            .tag("users")
            .path_param("id", "The user's id", json!({ "type": "integer" }))
            .response_json(200, "The user", json!({ "$ref": "#/components/schemas/User" }))
            .response(404, "No such user"),
    );

app.post("/users", create_user).doc(
    Operation::new()
        .summary("Create a user")
        .tag("users")
        .body(json!({
            "type": "object",
            "required": ["email"],
            "properties": { "email": { "type": "string", "format": "email" } }
        }))
        .response(201, "Created")
        .response(422, "Invalid fields"),
);
```

Schemas are plain JSON Schema, written with `json!`: there are no macros to learn, and what you write is what clients read.

| `Operation` method | Sets |
|---|---|
| `.summary(…)`, `.description(…)` | The summary and the description (Markdown) |
| `.tag(…)` | A tag, to group operations (several are allowed) |
| `.operation_id(…)` | The identifier that client generators use as a function name |
| `.path_param(name, description, schema)` | Describes a path parameter (otherwise listed as a string) |
| `.query(name, description, schema)`, `.query_required(…)` | A query string parameter |
| `.header(name, description, schema, required)` | A request header |
| `.body(schema)`, `.body_with(content_type, schema)` | A required request body, JSON or of another media type |
| `.response(status, description)`, `.response_json(status, description, schema)` | A possible response (`200 OK` when none is given) |
| `.security(scheme)`, `.public()` | Requires a security scheme, or none |
| `.deprecated()` | Marks the operation as deprecated |
| `.hidden()` | Leaves the route out of the document |
| `.extend(json!({ … }))` | Merges anything else (`externalDocs`, `callbacks`…) |

`.doc` works on `App` and on `Router`: a router's descriptions follow its routes when it is mounted.

## Describing the API

`OpenApi` holds what applies to the whole API:

```rust
let spec = OpenApi::new("Users API", "1.0.0")
    .description("Everything about users.")
    .server("https://api.example.com")
    .tag("users", "Accounts and profiles")
    .schema("User", json!({
        "type": "object",
        "properties": { "id": { "type": "integer" }, "email": { "type": "string" } }
    }))
    .security_scheme("bearer", json!({ "type": "http", "scheme": "bearer" }))
    .security("bearer") // every operation, unless `.public()`
    .extend(json!({ "info": { "contact": { "email": "api@example.com" } } }))
    .swagger_ui("/docs");
app.openapi("/openapi.json", spec);
```

Shared schemas are referenced with `json!({ "$ref": "#/components/schemas/User" })`.

## Schemas from your types (`schemars` feature)

Writing JSON Schema by hand repeats your structs. With the `schemars` Cargo feature, the schemas are derived from the types, like FastAPI does with Pydantic models: derive `JsonSchema` next to `Deserialize` or `Serialize`, then name the types in the operation.

```toml
[dependencies]
vitesse = { git = "https://github.com/maxlestage/Vitesse", features = ["schemars"] }
schemars = "1"
```

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vitesse::openapi::Operation;

#[derive(Deserialize, JsonSchema)]
struct NewPet {
    /// The pet's name.
    name: String,
    age: Option<u8>,
}

#[derive(Serialize, JsonSchema)]
struct Pet {
    id: u64,
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct Search {
    /// Words to look for.
    q: String,
    page: Option<u32>,
}

app.post("/pets", create_pet).doc(
    Operation::new()
        .summary("Add a pet")
        .validated_body::<NewPet>() // the body, and the 422 of its validation
        .response_type::<Pet>(201, "The new pet"),
);
app.get("/pets", search_pets).doc(Operation::new().query_type::<Search>());
```

| Method | Describes |
|---|---|
| `body_type::<T>()`, `form_type::<T>()` | A JSON or form request body |
| `validated_body::<T>()` | A body that also implements `Validate`, with its `422` response |
| `response_type::<T>(status, description)` | A JSON response |
| `query_type::<T>()` | One query parameter per field, in alphabetical order, required unless it is an `Option` or has a default |
| `OpenApi::schema_type::<T>()` | A shared schema, to reference yourself |

Named types land in `components/schemas` (with the types they use) and are referenced with `$ref`; doc comments become descriptions. A schema given with `OpenApi::schema` under the same name wins.

## Scalar, Redoc and a self-hosted Swagger UI

Besides Swagger UI, the document can be served with [Scalar](https://scalar.com), a modern reference to read the API and try it, and [Redoc](https://redocly.com/redoc), a three-column reference:

```rust
let spec = OpenApi::new("Pets", "1.0.0")
    .swagger_ui("/docs")
    .scalar("/reference")
    .redoc("/redoc");
```

These pages load their scripts from the jsDelivr CDN. On a network without Internet access, or with a strict `Content-Security-Policy`, serve Swagger UI's files yourself (the `dist` folder of the `swagger-ui-dist` npm package) and point the page to them:

```rust
app.static_dir("/vendor/swagger-ui", "vendor/swagger-ui-dist");
app.openapi(
    "/openapi.json",
    OpenApi::new("Pets", "1.0.0").swagger_ui("/docs").swagger_ui_assets("/vendor/swagger-ui"),
);
```

## What is listed

- Every route with a method, from the application, its routers and its groups.
- Not listed unless they have a `.doc(…)`: routes for every method (`app.all`), `HEAD` and `OPTIONS` routes, and wildcard routes such as static files. A route with `.doc(Operation::new().hidden())` is never listed.
- Neither the document itself nor the Swagger UI page.

## Exporting the document

`app.openapi_document(&spec)` returns the document of the routes added so far as a `serde_json::Value`, without serving it: write it to a file in a build step, or check it in a test.

```rust
let doc = app.openapi_document(&OpenApi::new("Users API", "1.0.0"));
std::fs::write("openapi.json", serde_json::to_vec_pretty(&doc)?)?;
```

In production, you may want to keep the document and the Swagger UI page private: guard their paths with an [`on_request` hook](middleware.md#lifecycle-hooks) or a global middleware that checks the credentials, or only call `app.openapi` in development builds.
